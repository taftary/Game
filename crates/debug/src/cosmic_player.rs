//! Cosmic-scale player: a live [`ShipState`] in `FrameId::Cosmological`.
//!
//! This is the WS2 foundation the demo (WS4), inspector (WS5), and fly-to
//! (WS6) build on. The player spawns inside a filament near the home node
//! (CSP-008), flies a scale-relative cruise law (update 2026-09-18-2027),
//! and slews time compression up to the Cosmological ceiling of 10⁹.
//!
//! Why cruise, not thrust: the spec §10 physical 30 m/s² envelope is
//! unusable at Mpc — positions are f64 Mpc at ~30 Mpc magnitude (ulp ≈
//! 7×10⁻¹⁵ Mpc ≈ 200,000 km), so per-frame ½·a·dt² ≈ 10⁻²² Mpc sits
//! thousands of times below the ulp and `pos + δ == pos` bit-exactly. A
//! ship under raw thrust could never move near a node, and the demo HUD
//! never even showed speed. Cruise replaces it with real-time
//! screen-speed authority: full input crosses the local scale length in
//! `t_cross_s` seconds, eased over `tau_s` (momentum feel, coast on
//! release — translation damping stays OFF per spec §10).
//!
//! Velocity-unit convention (pinned — a latent hand-back mismatch lived
//! here): [`ShipState::vel`] at Cosmological is **frame units per sim
//! second** (Mpc/sim-s), the same unit [`FlyToExec`] samples when it
//! drives or hands back. A cancelled fly-to therefore continues seamlessly
//! under cruise; integration `pos += v · sim_dt` is exact for the
//! piecewise-constant eased step and Mpc-scale per frame at any depth.
//!
//! Sim-time authority stays the [`CompressionClock`]: every frame pushes
//! wall time through `advance` on a throwaway integrator (one discarded
//! Verlet — the physics comes from the cruise step or the easing), so
//! `sim_time_s` advances by exactly `ratio × dt_real` in both modes.
//!
//! Window- and GPU-free: pure state + tests. Rendering (WS3/WS4) reads
//! this state through accessors.

use super::cosmic_camera::facing_quat_for;
use game_engine::flight::{FlyToExec, ShipMode, ShipState, mode_of};
use game_engine::frames::{FrameChain, FrameId};
use game_engine::physics::IntegratorState;
use game_engine::time::{CompressionClock, Occupancy, target_ratio};
use game_engine::universe::WebDescriptor;
use glam::{DQuat, DVec3};

/// Nominal spawn offset along the departure link, Mpc.
pub const SPAWN_OFFSET_MPC: f64 = 30.0;

/// Cruise tuning: full input crosses the local scale length in this many
/// real seconds (player-adjustable — see `CosmicDemoState::cruise_speed_by`
/// in the sibling demo module).
pub const CRUISE_T_CROSS_S: f64 = 20.0;

/// Cruise tuning: velocity-easing time constant in real seconds (the
/// momentum feel — release coasts, opposite input brakes).
pub const CRUISE_TAU_S: f64 = 0.75;

/// Cruise tuning: the scale length never drops below this multiple of the
/// nearest virial radius — precision speeds at a node, never frozen.
pub const CRUISE_SCALE_FLOOR_RVIR: f64 = 1.0;

/// Cruise-speed adjust: `Shift`+wheel notch factor on `t_cross_s`
/// (factor > 1 slows the crossing pace down, < 1 speeds it up).
pub const CRUISE_SPEED_NOTCH: f64 = 1.5;

/// Cruise-speed range for `t_cross_s`, real seconds.
pub const CRUISE_T_CROSS_MIN_S: f64 = 2.0;
/// Cruise-speed range for `t_cross_s`, real seconds.
pub const CRUISE_T_CROSS_MAX_S: f64 = 600.0;

/// Reference dry mass in kg (spec §10 envelope value): carried on the
/// ship for persistence parity. Thrust itself is not integrated at this
/// frame (see the module docs), so no craft envelope is stored.
pub const SHIP_DRY_MASS_KG: f64 = 5_000.0;

/// Cruise tuning (runtime params, not constants).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CruiseParams {
    /// Real seconds to cross the local scale length at full input.
    pub t_cross_s: f64,
    /// Velocity-easing time constant, real seconds.
    pub tau_s: f64,
}

impl Default for CruiseParams {
    fn default() -> Self {
        Self {
            t_cross_s: CRUISE_T_CROSS_S,
            tau_s: CRUISE_TAU_S,
        }
    }
}

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
/// cruise tuning + event queue. Field-visible for the WS4/WS6 shell wiring
/// (debug crate is developer-only; independence of fields keeps tick code
/// legible).
pub struct CosmicPlayerState {
    /// Ship in `FrameId::Cosmological` (f64 Mpc frame-chain + momentum).
    pub ship: ShipState,
    /// Slewed compression clock (10⁹ ceiling at this frame).
    pub clock: CompressionClock,
    /// Armed/committed fly-to executor, if any (WS6 engages).
    pub exec: Option<FlyToExec>,
    /// Node index the executor is flying to, if any.
    pub target_node: Option<u32>,
    /// Cruise tuning (update 2026-09-18-2027: `t_cross_s` is
    /// player-adjustable; the demo shell mutates it directly).
    pub cruise: CruiseParams,
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
                mass_kg: SHIP_DRY_MASS_KG,
                fuel: f64::INFINITY,
            },
            clock: CompressionClock::new(),
            exec: None,
            target_node: None,
            cruise: CruiseParams::default(),
            events: Vec::new(),
        }
    }

    /// Advance one real frame: slew compression toward the occupancy
    /// target, push wall time through the clock, then move. A committed
    /// executor drives position from the easing (and disarms at arrival
    /// with a completion event); otherwise the cruise law runs: full
    /// input crosses `scale_mpc` in `t_cross_s` real seconds, velocity
    /// eased over `tau_s` (momentum feel), zero input coasts at the
    /// current velocity. `ship.vel` is Mpc per **sim-second** everywhere
    /// (the [`FlyToExec`] unit), so integration is `pos += v · sim_dt`.
    ///
    /// `body_dir` is the cruise direction in ship body axes (zero =
    /// coast); `scale_mpc` is the local scale length from the demo's
    /// nearest-node scan; `depth_fraction` is distance-to-nearest-
    /// significant-body over its significance radius (callers that cannot
    /// compute it pass 1.0 = real-time; deep-space cruise passes large
    /// values).
    pub fn step_cruise(
        &mut self,
        body_dir: DVec3,
        scale_mpc: f64,
        dt_real_s: f64,
        depth_fraction: f64,
    ) {
        debug_assert!(dt_real_s >= 0.0 && dt_real_s.is_finite());
        debug_assert!(body_dir.is_finite());
        debug_assert!(scale_mpc >= 0.0 && scale_mpc.is_finite());
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
        // Free cruise only when no committed executor drove this frame.
        let driving = self
            .exec
            .as_ref()
            .is_some_and(|e| mode_of(Some(e)) == ShipMode::FlyTo);
        if !driving {
            let ratio = self.clock.ratio().max(1.0);
            // Full input crosses the scale length in t_cross_s real
            // seconds; the eased velocity is sim-anchored (per sim-second,
            // the FlyToExec unit) so executor hand-back continues cleanly.
            let full_sim = (scale_mpc / self.cruise.t_cross_s.max(f64::MIN_POSITIVE)) / ratio;
            let target_vel = if body_dir.length_squared() > 1e-24 {
                let world_dir = (self.ship.chain.orientation() * body_dir.normalize()).normalize();
                world_dir * full_sim.max(0.0)
            } else {
                // Release coasts: the target is the current velocity, so
                // the easing holds it bit-exact (damping stays OFF).
                self.ship.vel
            };
            let k = (1.0 - (-dt_real_s / self.cruise.tau_s.max(f64::MIN_POSITIVE)).exp())
                .clamp(0.0, 1.0);
            self.ship.vel += (target_vel - self.ship.vel) * k;
            let pos = self.ship.chain.position() + self.ship.vel * sim_dt_si;
            self.ship.chain.set_position(pos);
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

    /// Current cruise speed in Mpc per **real** second (HUD display:
    /// sim-anchored velocity through the live clock ratio).
    pub fn real_speed_mpc_s(&self) -> f64 {
        self.ship.vel.length() * self.clock.ratio()
    }
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
    fn coast_preserves_velocity_and_replays_bit_exact() {
        // Damping stays OFF (spec §10): zero input holds the velocity
        // bit-exact (the easing target is the current velocity itself),
        // and identical runs replay identically.
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        player.ship.vel = DVec3::new(0.5, -0.25, 0.1);
        let v0 = player.ship.vel;
        let p0 = player.position_mpc();
        for _ in 0..100 {
            player.step_cruise(DVec3::ZERO, 20.0, 1.0 / 60.0, 1.0);
        }
        assert_eq!(player.ship.vel, v0, "cosmic coast must not pace");
        // Coast still travels — Mpc-scale in seconds (the visible-motion
        // contract the old thrust law could never meet).
        let moved = (player.position_mpc() - p0).length();
        assert!(moved > 0.5, "coast must travel, moved {moved} Mpc");
        let mut replay = CosmicPlayerState::spawn(&web);
        replay.ship.vel = v0;
        for _ in 0..100 {
            replay.step_cruise(DVec3::ZERO, 20.0, 1.0 / 60.0, 1.0);
        }
        assert_eq!(player.ship, replay.ship, "fixed-step replay diverges");
    }

    #[test]
    fn cruise_from_spawn_moves_mpc_scale_in_seconds() {
        // The headline regression (update 2026-09-18-2027): at depth 1
        // the clock sits at ratio 1, so full input asks scale/t_cross =
        // 20/20 = 1 Mpc per real second, eased over 0.75 s. Five seconds
        // must move Mpc-scale along the input direction and converge on
        // the target speed — the old 30 m/s² law moved ~10⁻¹⁵ Mpc in the
        // same span (below the f64 ulp: the ship could never move).
        let web = nominal_web();
        let mut player = CosmicPlayerState::spawn(&web);
        player.ship.chain.set_orientation(DQuat::IDENTITY);
        let p0 = player.position_mpc();
        for _ in 0..300 {
            player.step_cruise(DVec3::X, 20.0, 1.0 / 60.0, 1.0);
        }
        let leg = player.position_mpc() - p0;
        assert!(
            leg.x > 1.0,
            "5 s of cruise must move Mpc-scale, moved {leg}"
        );
        let speed = player.ship.vel.length();
        assert!(
            (speed - 1.0).abs() < 0.01,
            "cruise must converge on 1 Mpc/s, got {speed}"
        );
        assert!(
            player.facing().x > 0.999,
            "cruise must keep flying the input direction"
        );
    }

    #[test]
    fn cruise_target_speed_scales_with_scale_length() {
        // Depth-scaling contract: same input, double the scale length ⇒
        // double the converged speed (both at ratio 1).
        let web = nominal_web();
        let mut near = CosmicPlayerState::spawn(&web);
        let mut far = CosmicPlayerState::spawn(&web);
        for _ in 0..300 {
            near.step_cruise(DVec3::X, 20.0, 1.0 / 60.0, 1.0);
            far.step_cruise(DVec3::X, 40.0, 1.0 / 60.0, 1.0);
        }
        let ratio = far.ship.vel.length() / near.ship.vel.length();
        assert!(
            (ratio - 2.0).abs() < 0.01,
            "speed must double with scale, ratio {ratio}"
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
    fn committed_executor_drives_to_rest_at_target() {
        // The WS6 branch contract, exercised directly: a committed
        // executor overrides cruise, samples the easing at sim time, and
        // disarms at arrival with a completion event.
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
        player.step_cruise(DVec3::ZERO, 1.0, 1.0, 1.0e6);
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
    fn cancel_mid_leg_hands_back_the_eased_state_unit_consistent() {
        // The velocity-unit pin: FlyToExec samples Mpc/sim-s and cruise
        // integrates Mpc/sim-s, so a mid-leg cancel must hand back exactly
        // the eased state (the old Gyr reading broke this by ×3.156e16).
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
        // 0.1 sim-s into the leg (ratio 1, far from arrival).
        player.step_cruise(DVec3::ZERO, 1.0, 0.1, 1.0);
        assert!(player.cancel_fly_to());
        let t = player.clock.sim_time_s();
        assert_eq!(
            player.ship.vel,
            plan.velocity_at(t),
            "hand-back velocity must be the eased state"
        );
        assert_eq!(
            player.position_mpc(),
            plan.position_at(t),
            "hand-back position must be the eased state"
        );
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
