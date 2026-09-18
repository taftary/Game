//! Cosmic-scale player: a live [`ShipState`] in `FrameId::Cosmological`.
//!
//! This is the WS2 foundation the demo (WS4), inspector (WS5), and fly-to
//! (WS6) build on. The player spawns inside a filament near the home node
//! (CSP-008), integrates physical free flight through the shipped
//! [`step_free_flight`] (CSP-006/007), and slews time compression up to
//! the Cosmological ceiling of 10⁹.
//!
//! Time-unit convention (read carefully — the `step_free_flight` doc
//! string says "real seconds", which holds only for SI-time frames):
//! `thrust_accel_frame` converts with `time_s² / length_m`, so the step
//! `dt` must be expressed in the active frame's time unit — **Gyr here**,
//! i.e. `dt_gyr = ratio × dt_real_s / FrameUnits::of(Cosmological).time_s`.
//! The CSP-007 analytic test pins this: full throttle for a known sim
//! span must match 30 m/s² exactly. (Noted for ADR-023 as a
//! spec-clarification; the function itself is untouched.)
//!
//! Single-step exactness: with zero live gravity (spec §5) and
//! piecewise-constant thrust input, velocity-Verlet under constant
//! acceleration is exact for any step size, so one
//! [`step_free_flight`] call per frame with the full sim dt is both
//! exact and cheap — no 16M-substep blowup at 10⁹ compression. Pinned by
//! `single_step_matches_fine_substeps`. Substepping activates only with
//! live gravity (future frames), never here.
//!
//! Sim-time authority stays the [`CompressionClock`]: every frame pushes
//! wall time through `advance` on a throwaway integrator (one discarded
//! Verlet — the physics comes from the flight step or the easing), so
//! `sim_time_s` advances by exactly `ratio × dt_real` in both modes.
//!
//! Window- and GPU-free: pure state + tests. Rendering (WS3/WS4) reads
//! this state through accessors.

use super::cosmic_camera::facing_quat_for;
use game_engine::flight::{
    CraftParams, FlyToExec, ShipMode, ShipState, ThrustInput, mode_of, step_free_flight,
};
use game_engine::frames::{FrameChain, FrameId};
use game_engine::physics::{FrameUnits, IntegratorState};
use game_engine::time::{CompressionClock, Occupancy, target_ratio};
use game_engine::universe::WebDescriptor;
use glam::{DQuat, DVec3};

/// Nominal spawn offset along the departure link, Mpc.
pub const SPAWN_OFFSET_MPC: f64 = 30.0;

/// Player-facing flight events (WS6 feeds the transition pill + console
/// from [`CosmicPlayerState::drain_events`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CosmicEvent {
    /// A node was click-selected as a fly-to target.
    TargetSelected {
        /// Selected node index.
        node: u32,
    },
    /// Fly-to engaged on the selected node.
    FlyToStarted {
        /// Target node index.
        node: u32,
    },
    /// Fly-to cancelled (by input or by re-engage).
    FlyToCancelled {
        /// Target node index.
        node: u32,
    },
    /// Fly-to arrived (eased to rest at the node center).
    FlyToCompleted {
        /// Target node index.
        node: u32,
    },
}

/// Zero live gravity at cosmic scale (spec §5): [`StaticDensityField`] is
/// static by design — no integration, no dynamics API. The closure below
/// is the enforced zero, and `zero_gravity_is_the_static_field` pins the
/// contract instead of assuming it.
fn zero_gravity(_position: DVec3) -> DVec3 {
    DVec3::ZERO
}

/// Live cosmic player: ship + compression clock + fly-to executor slot +
/// event queue. Field-visible for the WS4/WS6 shell wiring (debug crate
/// is developer-only; independence of fields keeps tick code legible).
pub struct CosmicPlayerState {
    /// Ship in `FrameId::Cosmological` (f64 Mpc frame-chain + momentum).
    pub ship: ShipState,
    /// Slewed compression clock (10⁹ ceiling at this frame).
    pub clock: CompressionClock,
    /// Armed/committed fly-to executor, if any (WS6 engages).
    pub exec: Option<FlyToExec>,
    /// Node index the executor is flying to, if any.
    pub target_node: Option<u32>,
    /// Craft envelope (spec §10 PO values via [`CraftParams::default`]).
    pub params: CraftParams,
    /// Pending flight events for the pill/console feed.
    pub events: Vec<CosmicEvent>,
}

impl CosmicPlayerState {
    /// Spawn inside a filament: home node → strongest departure link →
    /// [`SPAWN_OFFSET_MPC`] out along it (clamped to the middle 80% of
    /// short links), nose facing the far node. Falls back to +30 Mpc on
    /// +X from home when the web has no links (degenerate params —
    /// deterministic either way).
    pub fn spawn(web: &WebDescriptor) -> Self {
        let home_pos = DVec3::from(web.home().position_mpc);
        let (pos, orientation) = match web.strongest_link_from(web.home_node) {
            Some(link) => {
                let far_idx = if link.a == web.home_node {
                    link.b
                } else {
                    link.a
                };
                let far = DVec3::from(web.nodes[far_idx as usize].position_mpc);
                let leg = far - home_pos;
                let len = leg.length().max(f64::MIN_POSITIVE);
                let dir = leg / len;
                let d = SPAWN_OFFSET_MPC.clamp(0.1 * len, 0.9 * len);
                (home_pos + dir * d, facing_quat_for(dir))
            }
            None => (home_pos + DVec3::X * SPAWN_OFFSET_MPC, DQuat::IDENTITY),
        };
        Self {
            ship: ShipState {
                // Cosmological is the frame-chain root (depth 0): no
                // parent links.
                chain: FrameChain::new(FrameId::Cosmological, pos, orientation, Vec::new()),
                vel: DVec3::ZERO,
                mass_kg: params_mass(),
                fuel: f64::INFINITY,
            },
            clock: CompressionClock::new(),
            exec: None,
            target_node: None,
            params: CraftParams::default(),
            events: Vec::new(),
        }
    }

    /// Advance one real frame: slew compression toward the occupancy
    /// target, push wall time through the clock, then integrate. A
    /// committed executor drives position from the easing (and disarms
    /// at arrival with a completion event); otherwise one exact
    /// free-flight step runs.
    ///
    /// `depth_fraction` is distance-to-nearest-significant-body over its
    /// significance radius (callers that cannot compute it pass 1.0 =
    /// real-time; deep-space cruise passes large values).
    pub fn step_free(&mut self, input: &ThrustInput, dt_real_s: f64, depth_fraction: f64) {
        debug_assert!(dt_real_s >= 0.0 && dt_real_s.is_finite());
        let target = target_ratio(Occupancy {
            frame: FrameId::Cosmological,
            in_blend_band: false,
            depth_fraction,
        });
        self.clock.slew(target, dt_real_s);
        // Sim-time authority: one discarded Verlet carries exactly
        // ratio × dt_real into sim_time_s (physics comes next).
        let sim_dt_si = self.clock.ratio() * dt_real_s;
        let mut discarded = IntegratorState {
            pos: self.ship.chain.position(),
            vel: self.ship.vel,
        };
        self.clock.advance(
            &mut discarded,
            dt_real_s,
            &zero_gravity,
            sim_dt_si.max(f64::MIN_POSITIVE),
        );
        // A committed executor overrides free flight (WS6 engages it).
        let mut disarm = false;
        if let Some(exec) = &mut self.exec
            && mode_of(Some(&*exec)) == ShipMode::FlyTo
        {
            let t = self.clock.sim_time_s();
            let (pos, vel) = exec.update(t).expect("committed executor samples cleanly");
            self.ship.chain.set_position(pos);
            self.ship.vel = vel;
            if exec.is_complete(t) {
                let (end_pos, end_vel) =
                    exec.cancel(t).expect("committed executor cancels cleanly");
                self.ship.chain.set_position(end_pos);
                self.ship.vel = end_vel;
                disarm = true;
            }
        }
        if disarm {
            let node = self.target_node;
            self.exec = None;
            self.target_node = None;
            if let Some(node) = node {
                self.events.push(CosmicEvent::FlyToCompleted { node });
            }
            return;
        }
        // Free flight only when no committed executor drove this frame.
        let driving = self
            .exec
            .as_ref()
            .is_some_and(|e| mode_of(Some(e)) == ShipMode::FlyTo);
        if !driving {
            let frame_time_s = FrameUnits::of(FrameId::Cosmological).time_s;
            step_free_flight(
                &mut self.ship,
                &self.params,
                input,
                sim_dt_si / frame_time_s,
                &zero_gravity,
            );
        }
    }

    /// Drain pending flight events (pill/console feed consumes).
    pub fn drain_events(&mut self) -> Vec<CosmicEvent> {
        std::mem::take(&mut self.events)
    }

    /// Cancel a committed fly-to at the current sim time, resuming free
    /// flight with the eased state (continuous by construction). Pushes
    /// `FlyToCancelled` and returns true; no committed executor ⇒ false
    /// (no event, no-op for `E` toggling and thrust-cancel alike).
    pub fn cancel_fly_to(&mut self) -> bool {
        let committed = self
            .exec
            .as_ref()
            .is_some_and(|e| mode_of(Some(e)) == ShipMode::FlyTo);
        if !committed {
            return false;
        }
        let t = self.clock.sim_time_s();
        if let Some(exec) = self.exec.as_mut()
            && let Ok((pos, vel)) = exec.cancel(t)
        {
            self.ship.chain.set_position(pos);
            self.ship.vel = vel;
        }
        self.exec = None;
        if let Some(node) = self.target_node.take() {
            self.events.push(CosmicEvent::FlyToCancelled { node });
        }
        true
    }

    /// Active-frame position in Mpc (f64 flight truth; rendering
    /// recenter()s this into camera-relative f32).
    pub fn position_mpc(&self) -> DVec3 {
        self.ship.chain.position()
    }

    /// Ship nose direction in frame axes (body +X through orientation).
    pub fn facing(&self) -> DVec3 {
        self.ship.chain.orientation() * DVec3::X
    }
}

/// Reference dry mass: the spec §10 envelope value (single-sourced here
/// so spawn and CraftParams cannot drift apart).
fn params_mass() -> f64 {
    CraftParams::default().dry_mass_kg
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::flight::{Target, plan_fly_to};
    use game_engine::physics::StaticDensityField;
    use game_engine::time::CompressionMode;
    use game_engine::universe::{CosmicWebParams, generate_cosmic_web};

    fn nominal_web() -> WebDescriptor {
        generate_cosmic_web(1234, &CosmicWebParams::nominal())
    }

    #[test]
    fn spawn_sits_in_a_filament_facing_a_node() {
        let web = nominal_web();
        let player = CosmicPlayerState::spawn(&web);
        assert_eq!(player.ship.chain.active(), FrameId::Cosmological);
        assert_eq!(player.ship.vel, DVec3::ZERO);
        assert_eq!(player.ship.mass_kg, 5_000.0);
        assert_eq!(player.ship.fuel, f64::INFINITY);
        // Inside the descriptor sphere, off the home node, on the way
        // to a real far node.
        let pos = player.position_mpc();
        let home = DVec3::from(web.home().position_mpc);
        let d_home = (pos - home).length();
        assert!(
            (5.0..100.0).contains(&d_home),
            "spawn {d_home} Mpc from home"
        );
        let link = web
            .strongest_link_from(web.home_node)
            .expect("nominal home has a departure link");
        let far_idx = if link.a == web.home_node {
            link.b
        } else {
            link.a
        };
        let far = DVec3::from(web.nodes[far_idx as usize].position_mpc);
        let to_far = (far - pos).normalize();
        assert!(
            player.facing().dot(to_far) > 0.999,
            "nose off the far node: {}",
            player.facing().dot(to_far)
        );
    }

    #[test]
    fn spawn_without_links_falls_back_deterministically() {
        let mut web = nominal_web();
        web.links.clear();
        let a = CosmicPlayerState::spawn(&web);
        let b = CosmicPlayerState::spawn(&web);
        assert_eq!(a.ship.chain.position(), b.ship.chain.position());
        let home = DVec3::from(web.home().position_mpc);
        assert_eq!(a.position_mpc(), home + DVec3::X * SPAWN_OFFSET_MPC);
        assert_eq!(a.ship.chain.orientation(), DQuat::IDENTITY);
    }

    #[test]
    fn coast_keeps_momentum_bit_exact_in_cosmological() {
        // The flight precedent, lifted to the cosmic frame (DoD 5):
        // unthrusted coast changes nothing about velocity, and identical
        // runs replay identically.
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        player.ship.vel = DVec3::new(1.0e-6, 0.0, 0.0);
        let v0 = player.ship.vel;
        for _ in 0..100 {
            player.step_free(&ThrustInput::none(), 1.0 / 60.0, 10.0);
        }
        assert_eq!(player.ship.vel, v0, "cosmic coast must not pace");
        let mut replay = CosmicPlayerState::spawn(&web);
        replay.ship.vel = v0;
        for _ in 0..100 {
            replay.step_free(&ThrustInput::none(), 1.0 / 60.0, 10.0);
        }
        assert_eq!(player.ship, replay.ship, "fixed-step replay diverges");
    }

    #[test]
    fn thrust_matches_the_frame_unit_convention() {
        // Full throttle along body +X with identity orientation for a
        // known sim span must match 30 m/s² through the frame conversion
        // a_frame = a_si × T²/L with dt in frame time units (Gyr here).
        // This pins the WS2 time-unit reading of step_free_flight.
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        player.ship.chain.set_orientation(DQuat::IDENTITY);
        player.clock.slew(1_000.0, 1_000.0);
        assert_eq!(player.clock.ratio(), 1_000.0);
        let input = ThrustInput {
            throttle: 1.0,
            body_axis: DVec3::X,
            attitude: None,
            attitude_rate: DVec3::ZERO,
        };
        // Depth fraction 10 ⇒ target ratio exactly 1,000 (10³), so the
        // pre-slewed ratio holds through the step.
        player.step_free(&input, 60.0, 10.0);
        let units = FrameUnits::of(FrameId::Cosmological);
        let a_frame = 30.0 * units.time_s.powi(2) / units.length_m;
        let dt_frame = 1_000.0 * 60.0 / units.time_s;
        let expected = DVec3::X * a_frame * dt_frame;
        let got = player.ship.vel;
        assert!(
            (got - expected).length() / expected.length() < 1e-9,
            "dv mismatch: got {got} want {expected}"
        );
    }

    #[test]
    fn compression_ceiling_is_1e9_at_cosmological() {
        // DoD 5: far-from-everything cruise compresses to the
        // Cosmological ceiling and reports Compressed mode.
        let occ = Occupancy {
            frame: FrameId::Cosmological,
            in_blend_band: false,
            depth_fraction: 1.0e6,
        };
        assert_eq!(target_ratio(occ), 1.0e9);
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        player.clock.slew(target_ratio(occ), 1_000.0);
        assert_eq!(player.clock.ratio(), 1.0e9);
        assert_eq!(player.clock.mode(), CompressionMode::Compressed);
    }

    #[test]
    fn single_step_matches_fine_substeps() {
        // Justification for one step_free_flight per frame at any
        // compression: constant acceleration + zero gravity integrates
        // exactly at any step size (relative tolerance, since summation
        // order differs).
        let web = nominal_web();
        let input = ThrustInput {
            throttle: 0.7,
            body_axis: DVec3::new(0.0, 1.0, 0.0),
            attitude: None,
            attitude_rate: DVec3::ZERO,
        };
        let mut single = CosmicPlayerState::spawn(&web);
        single.clock.slew(1.0e6, 1_000.0);
        // Depth fraction 100 ⇒ target ratio exactly 10⁶, so the ratio
        // holds across single and substepped runs alike.
        single.step_free(&input, 1.0, 100.0);
        let mut fine = CosmicPlayerState::spawn(&web);
        fine.clock.slew(1.0e6, 1_000.0);
        for _ in 0..100 {
            fine.step_free(&input, 0.01, 100.0);
        }
        let dp = (single.position_mpc() - fine.position_mpc()).length();
        let scale = single.position_mpc().length().max(1.0);
        assert!(dp / scale < 1e-9, "single vs substep drift {dp}");
        let dv = (single.ship.vel - fine.ship.vel).length();
        let vscale = single.ship.vel.length().max(1.0e-30);
        assert!(dv / vscale < 1e-9, "velocity drift {dv}");
    }

    #[test]
    fn committed_executor_drives_to_rest_at_target() {
        // The WS6 branch contract, exercised directly: a committed
        // executor overrides free flight, samples the easing at sim
        // time, and disarms at arrival with a completion event.
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        let home = DVec3::from(web.home().position_mpc);
        let target = Target::new(FrameId::Cosmological, home).expect("finite");
        let plan = plan_fly_to(FrameId::Cosmological, player.position_mpc(), &target, 0.0)
            .expect("home is outside the arrival sphere");
        let mut exec = FlyToExec::new(plan);
        exec.commit().expect("commits once");
        player.exec = Some(exec);
        player.target_node = Some(web.home_node);
        // One real second at full compression covers the ~210 s leg.
        player.step_free(&ThrustInput::none(), 1.0, 1.0e6);
        assert!(player.exec.is_none(), "executor must disarm at arrival");
        assert!(player.target_node.is_none());
        // Arrival is the plan end (same easing path ⇒ bit-identical)
        // and sits at the node center.
        assert_eq!(player.position_mpc(), plan.position_at(plan.t_end_s()));
        assert!(
            (player.position_mpc() - home).length() < 1e-6,
            "arrival off the node center"
        );
        assert_eq!(player.ship.vel, DVec3::ZERO, "arrival is at rest");
        assert_eq!(
            player.drain_events(),
            vec![CosmicEvent::FlyToCompleted {
                node: web.home_node
            }]
        );
        assert!(player.drain_events().is_empty());
    }

    #[test]
    fn cancel_without_executor_is_a_quiet_no() {
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        assert!(!player.cancel_fly_to());
        assert!(player.drain_events().is_empty());
    }

    #[test]
    fn cancel_hands_back_the_eased_state() {
        use game_engine::flight::{Target, plan_fly_to};

        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        let home = DVec3::from(web.home().position_mpc);
        let target = Target::new(FrameId::Cosmological, home).expect("finite");
        let plan = plan_fly_to(FrameId::Cosmological, player.position_mpc(), &target, 0.0)
            .expect("leg plannable");
        let mut exec = FlyToExec::new(plan);
        exec.commit().expect("commits once");
        player.exec = Some(exec);
        player.target_node = Some(web.home_node);
        // Cancel mid-leg (sim time still 0 → eased start state).
        assert!(player.cancel_fly_to());
        assert!(player.exec.is_none());
        assert!(player.target_node.is_none());
        assert_eq!(player.position_mpc(), plan.position_at(0.0));
        assert_eq!(
            player.drain_events(),
            vec![CosmicEvent::FlyToCancelled {
                node: web.home_node
            }]
        );
    }

    #[test]
    fn zero_gravity_is_the_static_field() {
        // Spec §5: nothing moves in real time at cosmic scale — the step
        // closure is the enforced zero, and the field type pins it.
        assert!(StaticDensityField::is_static());
        assert_eq!(zero_gravity(DVec3::new(1.0, 2.0, 3.0)), DVec3::ZERO);
    }
}
