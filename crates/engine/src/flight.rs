//! `engine::flight` — free-flight dynamics + select-to-focus (spec §2).
//!
//! The default play state is free flight with real physical momentum;
//! select-to-focus flies a planned easing when the player picks a
//! target. Both modes share one [`ShipState`] (6D state vector +
//! orientation quaternion, spec §10 persistence shape).
//!
//! Pure dynamics over `f64`, headless-testable. Input is abstract
//! ([`ThrustInput`] — the game crate binds devices per `controls.md`);
//! cameras are never touched here (game-crate integration feeds
//! `OrbitCamera::set_anchor` from ship position later). Gravity is a
//! caller-supplied closure (ADR-018 per-scale model); compression
//! substeps call [`step_free_flight`] per physics step.
//!
//! ```
//! use game_engine::flight::{step_free_flight, CraftParams, ShipState, ThrustInput};
//! use game_engine::frames::{BodyId, FrameChain, FrameId, FrameLink};
//! use glam::{DQuat, DVec3};
//!
//! // Coast in the solar-system frame: no thrust, no gravity —
//! // momentum is bit-exact (no fixed pacing in free flight).
//! let mut ship = ShipState {
//!     chain: FrameChain::new(FrameId::SolarSystem, DVec3::ZERO, DQuat::IDENTITY,
//!         vec![FrameLink::identity(); 4]),
//!     vel: DVec3::new(1.0, 0.0, 0.0),
//!     mass_kg: 5_000.0,
//!     fuel: f64::INFINITY,
//! };
//! let idle = ThrustInput::none();
//! step_free_flight(&mut ship, &CraftParams::default(), &idle, 60.0, &|_| DVec3::ZERO);
//! assert_eq!(ship.vel, DVec3::new(1.0, 0.0, 0.0));
//! ```

use crate::frames::{
    FrameChain, FrameId, FrameLink, FrameTransition, TransitionError, TransitionReason,
};
use crate::physics::{FrameUnits, IntegratorState, verlet_step};
use crate::time::{CompressionSnapshot, SnapshotError as CompressionSnapshotError};
use glam::{DQuat, DVec3};

/// Seconds of fly-to per decade of distance (spec §1 constant time per
/// decade). Tunable; the planner pins the formula, not the value.
pub const SECONDS_PER_DECADE: f64 = 10.0;
/// Arrival sphere radius in meters: decades count down to this.
pub const ARRIVAL_M: f64 = 1_000.0;
/// Snapshot codec version.
pub const SHIP_SNAPSHOT_VERSION: u8 = 1;

/// Player craft parameters — the PO envelope (spec §10 addendum
/// *Resolved for v0.1.0*). All fields tunable; these are the v0.1.0
/// reference values, not constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CraftParams {
    /// Reference dry mass in kg.
    pub dry_mass_kg: f64,
    /// Max thrust acceleration in m/s² at reference mass.
    pub max_accel_ms2: f64,
    /// Orientation hold (PO: ON).
    pub rotation_stabilization: bool,
    /// Velocity damping assist (PO: OFF by default).
    pub translation_damping: bool,
    /// Damping rate in 1/s when enabled.
    pub damping_rate: f64,
    /// Infinite propellant (PO: fuel tracked, burn disabled).
    pub fuel_infinite: bool,
}

impl Default for CraftParams {
    fn default() -> Self {
        Self {
            dry_mass_kg: 5_000.0,
            max_accel_ms2: 30.0,
            rotation_stabilization: true,
            translation_damping: false,
            damping_rate: 0.5,
            fuel_infinite: true,
        }
    }
}

/// Abstract thrust/attitude input (device binding lives in the game
/// crate per the `controls.md` unified action map).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrustInput {
    /// Throttle 0..1 (clamped).
    pub throttle: f64,
    /// Thrust direction in ship body axes (normalized internally;
    /// zero vector = no thrust).
    pub body_axis: DVec3,
    /// Desired orientation (applied when stabilization is ON).
    pub attitude: Option<DQuat>,
    /// Body angular rates in rad/s (integrated when stabilization OFF).
    pub attitude_rate: DVec3,
}

impl ThrustInput {
    /// No thrust, no attitude change.
    pub fn none() -> Self {
        Self {
            throttle: 0.0,
            body_axis: DVec3::ZERO,
            attitude: None,
            attitude_rate: DVec3::ZERO,
        }
    }
}

/// Ship state: frame chain (position + orientation) + active-frame
/// velocity + mass/fuel. Velocity is active-frame length units per
/// SI second (time stays SI in every frame; only length rescales).
#[derive(Clone, Debug, PartialEq)]
pub struct ShipState {
    /// Position + orientation chain.
    pub chain: FrameChain,
    /// Velocity in active-frame length units per second.
    pub vel: DVec3,
    /// Mass in kg.
    pub mass_kg: f64,
    /// Fuel (tracked; burn disabled while `fuel_infinite`).
    pub fuel: f64,
}

impl ShipState {
    /// Thrust acceleration in active-frame units/s².
    pub fn thrust_accel_frame(&self, params: &CraftParams, input: &ThrustInput) -> DVec3 {
        let units = FrameUnits::of(self.chain.active());
        let throttle = input.throttle.clamp(0.0, 1.0);
        let axis = if input.body_axis.length_squared() > 1e-24 {
            input.body_axis.normalize()
        } else {
            DVec3::ZERO
        };
        let a_si = self.chain.orientation() * axis * (throttle * params.max_accel_ms2);
        a_si * (units.time_s.powi(2) / units.length_m)
    }

    /// Commit into the parent frame, converting position, orientation
    /// (chain) and velocity (linear map — momentum conserved by
    /// re-expression, exact up to fp).
    pub fn commit_to_parent(
        &mut self,
        reason: TransitionReason,
        sim_time_s: f64,
    ) -> Result<FrameTransition, TransitionError> {
        let v_parent = map_vel_to_parent(&self.chain, self.vel);
        let event = self.chain.commit_to_parent(reason, sim_time_s)?;
        self.vel = v_parent;
        Ok(event)
    }

    /// Commit into a direct child frame (see
    /// [`FrameChain::commit_to_child`]). Non-adjacent targets return
    /// `NotAdjacent` without touching state (adjacency checked before
    /// any mapping — no panic path).
    pub fn commit_to_child(
        &mut self,
        child: FrameId,
        link: FrameLink,
        reason: TransitionReason,
        sim_time_s: f64,
    ) -> Result<FrameTransition, TransitionError> {
        if child.parent() != Some(self.chain.active()) {
            return Err(TransitionError::NotAdjacent {
                active: self.chain.active(),
                target: child,
            });
        }
        let v_child = map_vel_to_child(self.chain.active(), child, &link, self.vel);
        let event = self
            .chain
            .commit_to_child(child, link, reason, sim_time_s)?;
        self.vel = v_child;
        Ok(event)
    }
}

/// Map an active-frame velocity into the parent frame (length units/s).
fn map_vel_to_parent(chain: &FrameChain, vel: DVec3) -> DVec3 {
    let frame = chain.active();
    let parent = frame.parent().expect("root has no parent velocity");
    let link = chain.link_to_parent().expect("chain missing parent link");
    let ratio = frame.meters_per_unit() / parent.meters_per_unit();
    link.rotation * (vel * ratio)
}

/// Map a parent-frame velocity into a direct child frame. Caller
/// guarantees adjacency (see [`ShipState::commit_to_child`]).
fn map_vel_to_child(parent: FrameId, child: FrameId, link: &FrameLink, vel: DVec3) -> DVec3 {
    let ratio = parent.meters_per_unit() / child.meters_per_unit();
    link.rotation.conjugate() * (vel * ratio)
}

/// First-order-exact attitude integration for piecewise-constant body
/// rates: exact quaternion step `q(Δt) = [axis·sin(|ω|Δt/2),
/// cos(|ω|Δt/2)]`, no Euler-angle drift, no accumulation error beyond fp.
fn integrate_attitude(q: DQuat, rate: DVec3, dt: f64) -> DQuat {
    let angle = rate.length() * dt;
    if angle < 1e-12 {
        return q;
    }
    (DQuat::from_axis_angle(rate.normalize(), angle) * q).normalize()
}

/// One free-flight physics step (`dt_s` real seconds at ratio 1 —
/// compression callers substep through here): attitude, optional
/// damping, thrust + gravity on velocity Verlet.
pub fn step_free_flight(
    state: &mut ShipState,
    params: &CraftParams,
    input: &ThrustInput,
    dt_s: f64,
    gravity: &impl Fn(DVec3) -> DVec3,
) {
    debug_assert!(dt_s > 0.0 && dt_s.is_finite());
    if params.rotation_stabilization {
        if let Some(att) = input.attitude {
            state.chain.set_orientation(att);
        }
    } else {
        state.chain.set_orientation(integrate_attitude(
            state.chain.orientation(),
            input.attitude_rate,
            dt_s,
        ));
    }
    let a_thrust = state.thrust_accel_frame(params, input);
    if params.translation_damping {
        state.vel *= (-params.damping_rate * dt_s).exp();
    }
    let mut s = IntegratorState {
        pos: state.chain.position(),
        vel: state.vel,
    };
    verlet_step(&mut s, dt_s, &|p| gravity(p) + a_thrust);
    state.chain.set_position(s.pos);
    state.vel = s.vel;
}

/// Fly-to target: a frame + a position in that frame's units.
/// Catalog-backed resolution (region cells, named bodies) lands with
/// `star-catalog-streaming`; callers map cells → positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// Frame containing `position`.
    pub frame: FrameId,
    /// Target position in frame units.
    pub position: DVec3,
}

impl Target {
    /// Checked constructor (failure has a surface — UX).
    pub fn new(frame: FrameId, position: DVec3) -> Result<Self, SelectError> {
        if !position.is_finite() {
            return Err(SelectError::NonFinitePosition);
        }
        Ok(Self { frame, position })
    }
}

/// Target selection / planning failure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelectError {
    /// Position contains NaN/inf.
    NonFinitePosition,
    /// Start and target coincide within the arrival sphere.
    TooClose {
        /// Start–target distance in meters.
        distance_m: f64,
    },
    /// Cross-frame legs are separate plans (replan on handoff).
    CrossFrame {
        /// Planning frame.
        from: FrameId,
        /// Target frame.
        to: FrameId,
    },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::NonFinitePosition => write!(f, "target position is not finite"),
            SelectError::TooClose { distance_m } => {
                write!(f, "target {distance_m} m away is inside the arrival sphere")
            }
            SelectError::CrossFrame { from, to } => write!(
                f,
                "cross-frame legs need separate plans ({} → {})",
                from.name(),
                to.name()
            ),
        }
    }
}

impl std::error::Error for SelectError {}

/// Smootherstep easing `6t⁵−15t⁴+10t³`: zero 1st + 2nd derivatives at
/// both ends — the ship departs and arrives at rest, pacing confined
/// to the transition (spec §2).
pub fn smootherstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Smootherstep first derivative (velocity shape).
pub fn smootherstep_vel(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    30.0 * t * t * t * t - 60.0 * t * t * t + 30.0 * t * t
}

/// Committed fly-to plan: straight segment in ONE frame with
/// constant-time-per-decade duration. Cross-frame journeys chain
/// per-leg plans via [`replan_on_handoff`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlyToPlan {
    /// Planning frame (start and target share it).
    pub frame: FrameId,
    /// Segment start in frame units.
    pub from: DVec3,
    /// Segment end in frame units.
    pub to: DVec3,
    /// Segment start time (sim seconds).
    pub t_start_s: f64,
    /// Segment duration (sim seconds).
    pub duration_s: f64,
}

impl FlyToPlan {
    /// Segment end time.
    pub fn t_end_s(self) -> f64 {
        self.t_start_s + self.duration_s
    }

    /// Eased position at sim time `t` (clamped to the segment).
    pub fn position_at(self, t: f64) -> DVec3 {
        let u = ((t - self.t_start_s) / self.duration_s).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * smootherstep(u)
    }

    /// Eased velocity at sim time `t` (frame units/s; zero at both
    /// ends — arrival and cancellation are rest-continuous).
    pub fn velocity_at(self, t: f64) -> DVec3 {
        let u = ((t - self.t_start_s) / self.duration_s).clamp(0.0, 1.0);
        (self.to - self.from) / self.duration_s * smootherstep_vel(u)
    }

    /// Distance remaining in meters at sim time `t`.
    pub fn remaining_m(self, t: f64) -> f64 {
        (self.to - self.position_at(t)).length() * self.frame.meters_per_unit()
    }

    /// ETA in sim seconds at sim time `t` (HUD display).
    pub fn eta_s(self, t: f64) -> f64 {
        (self.t_end_s() - t).max(0.0)
    }
}

/// Plan a single-frame leg: duration = `SECONDS_PER_DECADE ×
/// log10(start_distance_m / ARRIVAL_M)`.
pub fn plan_fly_to(
    frame: FrameId,
    from: DVec3,
    target: &Target,
    t_start_s: f64,
) -> Result<FlyToPlan, SelectError> {
    if frame != target.frame {
        return Err(SelectError::CrossFrame {
            from: frame,
            to: target.frame,
        });
    }
    if !from.is_finite() {
        return Err(SelectError::NonFinitePosition);
    }
    let distance_m = (target.position - from).length() * frame.meters_per_unit();
    if !distance_m.is_finite() || distance_m <= 0.0 {
        return Err(SelectError::NonFinitePosition);
    }
    let decades = (distance_m / ARRIVAL_M).log10();
    if decades <= 0.0 {
        return Err(SelectError::TooClose { distance_m });
    }
    Ok(FlyToPlan {
        frame,
        from,
        to: target.position,
        t_start_s,
        duration_s: SECONDS_PER_DECADE * decades,
    })
}

/// Executor lifecycle failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecError {
    /// `update`/`replan` before `commit`.
    NotCommitted,
    /// Double `commit`.
    AlreadyCommitted,
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::NotCommitted => write!(f, "fly-to plan is not committed"),
            ExecError::AlreadyCommitted => write!(f, "fly-to plan is already committed"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Select-to-focus executor: armed → committed → executing, cancellable
/// at any moment (commit/cancel mirrors `gameplay.md` transit rules).
#[derive(Clone, Debug, PartialEq)]
pub struct FlyToExec {
    plan: FlyToPlan,
    committed: bool,
}

impl FlyToExec {
    /// Arm a plan (uncommitted).
    pub fn new(plan: FlyToPlan) -> Self {
        Self {
            plan,
            committed: false,
        }
    }

    /// Commit (arm → executing). Double commit is an error.
    pub fn commit(&mut self) -> Result<(), ExecError> {
        if self.committed {
            return Err(ExecError::AlreadyCommitted);
        }
        self.committed = true;
        Ok(())
    }

    /// Cancel at sim time `t`: disarm and return the eased
    /// (position, velocity) to resume free flight — continuous by
    /// construction (same values the executor just produced).
    pub fn cancel(&mut self, t: f64) -> Result<(DVec3, DVec3), ExecError> {
        let state = self.update(t)?;
        self.committed = false;
        Ok(state)
    }

    /// Eased (position, velocity) at sim time `t`. Errors before commit.
    pub fn update(&self, t: f64) -> Result<(DVec3, DVec3), ExecError> {
        if !self.committed {
            return Err(ExecError::NotCommitted);
        }
        Ok((self.plan.position_at(t), self.plan.velocity_at(t)))
    }

    /// True once the segment end is reached.
    pub fn is_complete(&self, t: f64) -> bool {
        t >= self.plan.t_end_s()
    }

    /// Active plan.
    pub fn plan(&self) -> FlyToPlan {
        self.plan
    }
}

/// Start the next single-frame leg after a handoff commit (HOF-007
/// rule): blending governs acceleration, the plan adapts. `new_from`
/// must be the converted joint state (the caller owns the chain);
/// continuity at the joint is exact when it is. The new leg starts at
/// `t_now`, uncommitted.
pub fn replan_on_handoff(
    t_now: f64,
    new_frame: FrameId,
    new_from: DVec3,
    new_target: &Target,
) -> Result<FlyToPlan, SelectError> {
    plan_fly_to(new_frame, new_from, new_target, t_now)
}

/// Ship mode for HUD display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipMode {
    /// Piloted free flight.
    FreeFlight,
    /// Automated select-to-focus leg.
    FlyTo,
}

/// Mode from executor presence.
pub fn mode_of(exec: Option<&FlyToExec>) -> ShipMode {
    match exec {
        Some(e) if e.committed => ShipMode::FlyTo,
        _ => ShipMode::FreeFlight,
    }
}

/// Save payload for `autosave-persistence` (ADR-004, frozen shape):
/// frame code + 6D state + orientation + mass/fuel + optional active
/// plan + compression snapshot. Layout: version u8, frame discriminant
/// u8, body u64 LE, pos/vel 3×f64 LE each, quat (x,y,z,w) 4×f64 LE,
/// mass/fuel f64 LE, has_plan u8, [plan 73 B if present], compression
/// 24 B. Lengths: 131 bare, 204 with plan.
#[derive(Clone, Debug, PartialEq)]
pub struct ShipSnapshot {
    /// Active frame.
    pub frame: FrameId,
    /// Position in frame units.
    pub position: DVec3,
    /// Velocity in frame units/s.
    pub velocity: DVec3,
    /// Orientation in frame axes.
    pub orientation: DQuat,
    /// Mass in kg.
    pub mass_kg: f64,
    /// Fuel.
    pub fuel: f64,
    /// Active fly-to plan, if any.
    pub plan: Option<FlyToPlan>,
    /// Compression state.
    pub compression: CompressionSnapshot,
}

impl ShipSnapshot {
    /// Capture from live state (+ optional exec plan + clock).
    pub fn capture(
        ship: &ShipState,
        plan: Option<FlyToPlan>,
        clock: &crate::time::CompressionClock,
    ) -> Self {
        Self {
            frame: ship.chain.active(),
            position: ship.chain.position(),
            velocity: ship.vel,
            orientation: ship.chain.orientation(),
            mass_kg: ship.mass_kg,
            fuel: ship.fuel,
            plan,
            compression: CompressionSnapshot::capture(clock),
        }
    }

    /// Encode (131 or 204 bytes).
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(204);
        out.push(SHIP_SNAPSHOT_VERSION);
        let (d, b) = self.frame.code();
        out.push(d);
        out.extend_from_slice(&b.to_le_bytes());
        for v in [self.position, self.velocity] {
            for c in [v.x, v.y, v.z] {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        for c in [
            self.orientation.x,
            self.orientation.y,
            self.orientation.z,
            self.orientation.w,
        ] {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out.extend_from_slice(&self.mass_kg.to_le_bytes());
        out.extend_from_slice(&self.fuel.to_le_bytes());
        match self.plan {
            Some(plan) => {
                out.push(1);
                out.extend_from_slice(&plan_to_bytes(plan));
            }
            None => out.push(0),
        }
        out.extend_from_slice(&self.compression.to_bytes());
        out
    }

    /// Decode, rejecting malformed input (persistence contract: corrupt
    /// saves fail clean, never boot-loop).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() != 131 && bytes.len() != 204 {
            return Err(SnapshotError::BadLength(bytes.len()));
        }
        if bytes[0] != SHIP_SNAPSHOT_VERSION {
            return Err(SnapshotError::BadVersion(bytes[0]));
        }
        let frame = FrameId::from_code(
            bytes[1],
            u64::from_le_bytes(bytes[2..10].try_into().expect("len checked")),
        )
        .ok_or(SnapshotError::BadFrame(bytes[1]))?;
        let f64_at = |range: std::ops::Range<usize>| {
            f64::from_le_bytes(bytes[range].try_into().expect("layout fixed"))
        };
        let position = DVec3::new(f64_at(10..18), f64_at(18..26), f64_at(26..34));
        let velocity = DVec3::new(f64_at(34..42), f64_at(42..50), f64_at(50..58));
        let orientation = DQuat::from_xyzw(
            f64_at(58..66),
            f64_at(66..74),
            f64_at(74..82),
            f64_at(82..90),
        );
        let mass_kg = f64_at(90..98);
        let fuel = f64_at(98..106);
        let has_plan = bytes[106] == 1;
        if has_plan != (bytes.len() == 204) {
            return Err(SnapshotError::BadLength(bytes.len()));
        }
        let plan = if has_plan {
            Some(plan_from_bytes(&bytes[107..180])?)
        } else {
            None
        };
        let comp_base = if has_plan { 180 } else { 107 };
        let compression = CompressionSnapshot::from_bytes(&bytes[comp_base..comp_base + 24])
            .map_err(SnapshotError::BadCompression)?;
        for v in [
            position.x, position.y, position.z, velocity.x, velocity.y, velocity.z, mass_kg,
        ] {
            if !v.is_finite() {
                return Err(SnapshotError::BadValue);
            }
        }
        // Infinite fuel is first-class PO state (infinite propellant in
        // v0.1.0); NaN / −inf fuel is corrupt.
        if !(fuel.is_finite() || fuel == f64::INFINITY) {
            return Err(SnapshotError::BadValue);
        }
        if !orientation.is_normalized() {
            return Err(SnapshotError::BadValue);
        }
        Ok(Self {
            frame,
            position,
            velocity,
            orientation,
            mass_kg,
            fuel,
            plan,
            compression,
        })
    }
}

/// Fixed 73-byte plan encoding.
fn plan_to_bytes(plan: FlyToPlan) -> [u8; 73] {
    let mut out = [0u8; 73];
    let (d, b) = plan.frame.code();
    out[0] = d;
    out[1..9].copy_from_slice(&b.to_le_bytes());
    let mut at = 9;
    for v in [plan.from, plan.to] {
        for c in [v.x, v.y, v.z] {
            out[at..at + 8].copy_from_slice(&c.to_le_bytes());
            at += 8;
        }
    }
    out[at..at + 8].copy_from_slice(&plan.t_start_s.to_le_bytes());
    at += 8;
    out[at..at + 8].copy_from_slice(&plan.duration_s.to_le_bytes());
    out
}

/// Decode a 73-byte plan.
fn plan_from_bytes(bytes: &[u8]) -> Result<FlyToPlan, SnapshotError> {
    if bytes.len() != 73 {
        return Err(SnapshotError::BadLength(bytes.len()));
    }
    let frame = FrameId::from_code(
        bytes[0],
        u64::from_le_bytes(bytes[1..9].try_into().expect("len checked")),
    )
    .ok_or(SnapshotError::BadFrame(bytes[0]))?;
    let f64_at = |range: std::ops::Range<usize>| {
        f64::from_le_bytes(bytes[range].try_into().expect("layout fixed"))
    };
    let from = DVec3::new(f64_at(9..17), f64_at(17..25), f64_at(25..33));
    let to = DVec3::new(f64_at(33..41), f64_at(41..49), f64_at(49..57));
    let t_start_s = f64_at(57..65);
    let duration_s = f64_at(65..73);
    for v in [
        from.x, from.y, from.z, to.x, to.y, to.z, t_start_s, duration_s,
    ] {
        if !v.is_finite() {
            return Err(SnapshotError::BadValue);
        }
    }
    if duration_s <= 0.0 {
        return Err(SnapshotError::BadValue);
    }
    Ok(FlyToPlan {
        frame,
        from,
        to,
        t_start_s,
        duration_s,
    })
}

/// Snapshot decode failure.
#[derive(Clone, Debug, PartialEq)]
pub enum SnapshotError {
    /// Buffer is neither 131 nor 204 bytes (plan length included).
    BadLength(usize),
    /// Unknown snapshot version.
    BadVersion(u8),
    /// Unknown frame discriminant.
    BadFrame(u8),
    /// Non-finite state, denormalized quaternion, or non-positive plan.
    BadValue,
    /// Embedded compression snapshot rejected.
    BadCompression(CompressionSnapshotError),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::BadLength(n) => {
                write!(f, "ship snapshot must be 131 or 204 bytes, got {n}")
            }
            SnapshotError::BadVersion(v) => write!(f, "unknown ship snapshot version {v}"),
            SnapshotError::BadFrame(d) => write!(f, "unknown frame discriminant {d}"),
            SnapshotError::BadValue => write!(f, "non-finite ship state in snapshot"),
            SnapshotError::BadCompression(e) => write!(f, "bad compression snapshot: {e}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::BodyId;
    use crate::time::CompressionClock;

    const EARTH: BodyId = BodyId::EARTH;

    fn solar_ship() -> ShipState {
        ShipState {
            chain: FrameChain::new(
                FrameId::SolarSystem,
                DVec3::new(1.0, 0.0, 0.0),
                DQuat::IDENTITY,
                vec![FrameLink::identity(); 4],
            ),
            vel: DVec3::new(0.0, 0.1, 0.0),
            mass_kg: 5_000.0,
            fuel: f64::INFINITY,
        }
    }

    fn planetary_ship() -> ShipState {
        ShipState {
            chain: FrameChain::new(
                FrameId::Planetocentric(EARTH),
                DVec3::new(7_000.0, 0.0, 0.0),
                DQuat::IDENTITY,
                vec![FrameLink::identity(); 5],
            ),
            vel: DVec3::new(0.0, 7.0, 0.0),
            mass_kg: 5_000.0,
            fuel: f64::INFINITY,
        }
    }

    #[test]
    fn coast_keeps_momentum_bit_exact_in_both_frames() {
        // DoD 1 + no-fixed-pacing acceptance: unthrusted coast changes
        // nothing about velocity, in solar-system AND planetary frames.
        for mut ship in [solar_ship(), planetary_ship()] {
            let v0 = ship.vel;
            for _ in 0..100 {
                step_free_flight(
                    &mut ship,
                    &CraftParams::default(),
                    &ThrustInput::none(),
                    60.0,
                    &|_| DVec3::ZERO,
                );
            }
            assert_eq!(ship.vel, v0, "free flight must not pace the ship");
        }
    }

    #[test]
    fn thrust_produces_expected_dv_in_both_frames() {
        // Full throttle 30 m/s² for one frame-time-unit: Δv matches the
        // frame-unit conversion exactly (constant accel ⇒ Verlet exact).
        let input = ThrustInput {
            throttle: 1.0,
            body_axis: DVec3::X,
            attitude: None,
            attitude_rate: DVec3::ZERO,
        };
        // Solar: factor = day²/AU; one day of thrust.
        let mut solar = solar_ship();
        step_free_flight(
            &mut solar,
            &CraftParams::default(),
            &input,
            86_400.0,
            &|_| DVec3::ZERO,
        );
        let solar_factor = 86_400.0f64.powi(2) / 1.495_978_707e11;
        let expected_solar = DVec3::new(0.0, 0.1, 0.0) + DVec3::X * 30.0 * solar_factor * 86_400.0;
        assert!((solar.vel - expected_solar).length() / expected_solar.length() < 1e-9);
        // Planetary: factor = 1/1000 (km, s); 100 s of thrust.
        let mut planet = planetary_ship();
        step_free_flight(&mut planet, &CraftParams::default(), &input, 100.0, &|_| {
            DVec3::ZERO
        });
        let expected_planet = DVec3::new(0.0, 7.0, 0.0) + DVec3::X * 30.0 * 0.001 * 100.0;
        assert!((planet.vel - expected_planet).length() / expected_planet.length() < 1e-9);
    }

    #[test]
    fn commit_conserves_momentum_across_frames() {
        // Physical speed (|v| × length_m) is invariant under commit:
        // momentum conserved by re-expression, up and back down.
        let mut ship = solar_ship();
        let speed_before = ship.vel.length() * ship.chain.active().meters_per_unit();
        ship.commit_to_parent(TransitionReason::Manual, 0.0)
            .unwrap();
        assert_eq!(ship.chain.active(), FrameId::StellarNeighborhood);
        let speed_mid = ship.vel.length() * ship.chain.active().meters_per_unit();
        assert!((speed_mid - speed_before).abs() / speed_before < 1e-12);
        // Back down: SolarSystem is StellarNeighborhood's direct child.
        ship.commit_to_child(
            FrameId::SolarSystem,
            FrameLink::identity(),
            TransitionReason::Manual,
            0.0,
        )
        .unwrap();
        assert_eq!(ship.chain.active(), FrameId::SolarSystem);
        let speed_after = ship.vel.length() * ship.chain.active().meters_per_unit();
        assert!((speed_after - speed_before).abs() / speed_before < 1e-12);
        // Non-adjacent targets are rejected without touching state: from
        // StellarNeighborhood, Planetocentric (a SolarSystem child) is
        // not adjacent.
        ship.commit_to_parent(TransitionReason::Manual, 0.0)
            .unwrap();
        let before = ship.clone();
        assert!(matches!(
            ship.commit_to_child(
                FrameId::Planetocentric(EARTH),
                FrameLink::identity(),
                TransitionReason::Manual,
                0.0
            ),
            Err(TransitionError::NotAdjacent { .. })
        ));
        assert_eq!(ship.chain, before.chain);
        assert_eq!(ship.vel, before.vel);
    }

    #[test]
    fn planner_uses_decades_and_documented_easing() {
        // 1e14 m → 1e6 m in-frame (SolarSystem): 11 decades × 10 s.
        let target = Target::new(FrameId::SolarSystem, DVec3::new(668.0, 0.0, 0.0)).unwrap();
        let from = DVec3::ZERO;
        let plan = plan_fly_to(FrameId::SolarSystem, from, &target, 100.0).unwrap();
        let d_m = 668.0 * 1.495_978_707e11;
        let decades = (d_m / ARRIVAL_M).log10();
        assert!((plan.duration_s - SECONDS_PER_DECADE * decades).abs() < 1e-9);
        assert!((100.0..120.0).contains(&plan.duration_s));
        // Documented easing: endpoints exact, rest at both ends,
        // smootherstep(0.25) = 0.103515625.
        assert_eq!(plan.position_at(100.0), from);
        assert_eq!(plan.position_at(plan.t_end_s()), target.position);
        assert_eq!(plan.velocity_at(100.0), DVec3::ZERO);
        assert_eq!(plan.velocity_at(plan.t_end_s()), DVec3::ZERO);
        let mid = plan.position_at(100.0 + plan.duration_s * 0.25);
        let frac = (mid - from).length() / (target.position - from).length();
        assert!((frac - 0.103_515_625).abs() < 1e-9, "easing frac = {frac}");
    }

    #[test]
    fn executor_completes_and_cancel_is_clean() {
        // DoD 2: full leg executes to rest at the target…
        let target = Target::new(FrameId::SolarSystem, DVec3::new(10.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &target, 0.0).unwrap();
        let mut exec = FlyToExec::new(plan);
        assert_eq!(exec.update(1.0), Err(ExecError::NotCommitted));
        exec.commit().unwrap();
        assert_eq!(exec.commit(), Err(ExecError::AlreadyCommitted));
        assert!(!exec.is_complete(0.0));
        let (end_pos, end_vel) = exec.update(plan.t_end_s()).unwrap();
        assert_eq!(end_pos, target.position);
        assert_eq!(end_vel, DVec3::ZERO);
        assert!(exec.is_complete(plan.t_end_s()));
        // …and mid-leg cancel resumes free flight with the eased state:
        // position exact, velocity = analytic easing derivative
        // (1.875 × segment / T at u = 0.5).
        let mut exec2 = FlyToExec::new(plan);
        exec2.commit().unwrap();
        let t_half = plan.duration_s * 0.5;
        let (pos, vel) = exec2.cancel(t_half).unwrap();
        assert_eq!(pos, plan.position_at(t_half));
        let expected_vel = (target.position - DVec3::ZERO) / plan.duration_s * 1.875;
        assert!((vel - expected_vel).length() / expected_vel.length() < 1e-12);
        assert_eq!(mode_of(Some(&exec2)), ShipMode::FreeFlight);
        assert_eq!(mode_of(None), ShipMode::FreeFlight);
    }

    #[test]
    fn replan_on_handoff_is_position_exact() {
        // HOF-007: mid-leg frame commit → next leg starts exactly at the
        // converted joint. Solar leg eased to u = 0.4, committed up to
        // StellarNeighborhood through a real chain, replanned there.
        let target = Target::new(FrameId::SolarSystem, DVec3::new(10.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &target, 0.0).unwrap();
        let t_now = plan.duration_s * 0.4;
        let mut chain = FrameChain::new(
            FrameId::SolarSystem,
            plan.position_at(t_now),
            DQuat::IDENTITY,
            vec![FrameLink::identity(); 4],
        );
        chain
            .commit_to_parent(TransitionReason::BoundaryCrossing, t_now)
            .unwrap();
        assert_eq!(chain.active(), FrameId::StellarNeighborhood);
        let new_target =
            Target::new(FrameId::StellarNeighborhood, DVec3::new(5.0, 1.0, 0.0)).unwrap();
        let new_plan = replan_on_handoff(
            t_now,
            FrameId::StellarNeighborhood,
            chain.position(),
            &new_target,
        )
        .unwrap();
        assert_eq!(new_plan.from, chain.position());
        assert_eq!(new_plan.t_start_s, t_now);
    }

    #[test]
    fn hud_accessors_report_mode_target_distance_eta() {
        // UX acceptance: everything spec §10 select-to-focus display
        // needs is observable.
        let target = Target::new(FrameId::SolarSystem, DVec3::new(10.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &target, 0.0).unwrap();
        let mut exec = FlyToExec::new(plan);
        exec.commit().unwrap();
        assert_eq!(mode_of(Some(&exec)), ShipMode::FlyTo);
        assert_eq!(exec.plan().frame, FrameId::SolarSystem);
        assert_eq!(exec.plan().to, target.position);
        let d0 = plan.remaining_m(0.0);
        assert!((d0 - 10.0 * 1.495_978_707e11).abs() / d0 < 1e-12);
        assert_eq!(plan.eta_s(0.0), plan.duration_s);
        assert_eq!(plan.remaining_m(plan.t_end_s()), 0.0);
        assert_eq!(plan.eta_s(plan.t_end_s()), 0.0);
    }

    #[test]
    fn target_validation_rejects_bad_inputs() {
        assert_eq!(
            Target::new(FrameId::SolarSystem, DVec3::new(f64::NAN, 0.0, 0.0)),
            Err(SelectError::NonFinitePosition)
        );
        let near = Target::new(FrameId::SolarSystem, DVec3::new(1e-12, 0.0, 0.0)).unwrap();
        assert!(matches!(
            plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &near, 0.0),
            Err(SelectError::TooClose { .. })
        ));
        let other = Target::new(FrameId::Planetocentric(EARTH), DVec3::X).unwrap();
        assert_eq!(
            plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &other, 0.0),
            Err(SelectError::CrossFrame {
                from: FrameId::SolarSystem,
                to: FrameId::Planetocentric(EARTH),
            })
        );
    }

    #[test]
    fn assists_follow_the_po_envelope() {
        // Rotation stabilization ON: attitude snaps to input…
        let mut ship = solar_ship();
        let want = DQuat::from_rotation_z(0.5);
        let input = ThrustInput {
            attitude: Some(want),
            ..ThrustInput::none()
        };
        step_free_flight(&mut ship, &CraftParams::default(), &input, 1.0, &|_| {
            DVec3::ZERO
        });
        assert_eq!(ship.chain.orientation(), want);
        // …OFF: body rates integrate instead.
        let manual = CraftParams {
            rotation_stabilization: false,
            ..CraftParams::default()
        };
        let mut ship2 = solar_ship();
        let spin = ThrustInput {
            attitude_rate: DVec3::Z,
            ..ThrustInput::none()
        };
        step_free_flight(&mut ship2, &manual, &spin, 1.0, &|_| DVec3::ZERO);
        let angle = 2.0 * ship2.chain.orientation().w.acos();
        assert!((angle - 1.0).abs() < 1e-6, "yawed {angle} rad in 1 s");
        // Translation damping OFF by default (coast untouched)…
        assert!(!CraftParams::default().translation_damping);
        // …ON: velocity decays exponentially.
        let damped = CraftParams {
            translation_damping: true,
            ..CraftParams::default()
        };
        let mut ship3 = solar_ship();
        let v0 = ship3.vel.length();
        step_free_flight(&mut ship3, &damped, &ThrustInput::none(), 2.0, &|_| {
            DVec3::ZERO
        });
        assert!((ship3.vel.length() - v0 * (-0.5 * 2.0f64).exp()).abs() / v0 < 1e-12);
    }

    #[test]
    fn snapshot_round_trips_and_midflight_resume_is_exact() {
        // DoD 3 (payload level): mid-transition snapshot → restore →
        // identical continuation.
        let target = Target::new(FrameId::SolarSystem, DVec3::new(10.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, DVec3::ZERO, &target, 0.0).unwrap();
        let clock = CompressionClock::new();
        let ship = solar_ship();
        let snap = ShipSnapshot::capture(&ship, Some(plan), &clock);
        let bytes = snap.clone().to_bytes();
        assert!(bytes.len() == 204, "with-plan length {}", bytes.len());
        assert_eq!(ShipSnapshot::from_bytes(&bytes), Ok(snap.clone()));
        let bare = ShipSnapshot::capture(&ship, None, &clock);
        assert_eq!(bare.clone().to_bytes().len(), 131);
        assert_eq!(ShipSnapshot::from_bytes(&bare.clone().to_bytes()), Ok(bare));
        // Corrupt inputs rejected.
        assert!(matches!(
            ShipSnapshot::from_bytes(&bytes[..130]),
            Err(SnapshotError::BadLength(130))
        ));
        let mut bad_frame = bytes.clone();
        bad_frame[1] = 9;
        assert!(matches!(
            ShipSnapshot::from_bytes(&bad_frame),
            Err(SnapshotError::BadFrame(9))
        ));
        // Resume: fresh exec from the restored plan ends identically.
        let t_half = plan.duration_s * 0.5;
        let restored_plan = snap.plan.unwrap();
        let mut a = FlyToExec::new(plan);
        let mut b = FlyToExec::new(restored_plan);
        a.commit().unwrap();
        b.commit().unwrap();
        assert_eq!(a.update(t_half), b.update(t_half));
        assert_eq!(a.update(plan.t_end_s()), b.update(restored_plan.t_end_s()));
    }
}
