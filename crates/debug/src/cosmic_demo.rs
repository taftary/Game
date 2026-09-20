//! Game Demo cosmic state (WS4): the generated web, the live player, its
//! camera, the shipping HUD, and the frame's input intent.
//!
//! Window- and GPU-free: the binary turns keys/cursor into
//! [`CosmicDemoState::cruise_input`]/[`steer`](CosmicDemoState::steer)
//! intent, ticks [`tick`](CosmicDemoState::tick), and uploads buffers on
//! [`needs_rebase`](CosmicDemoState::needs_rebase). Everything here is
//! unit-testable.

use super::cosmic_camera::CosmicCamera;
use super::cosmic_player::{
    CRUISE_SCALE_FLOOR_RVIR, CRUISE_T_CROSS_MAX_S, CRUISE_T_CROSS_MIN_S, CosmicEvent,
    CosmicPlayerState,
};
use super::cosmic_web::COSMIC_PICK_RADIUS_PX;
use super::picking::project_to_screen;
use super::ui::Rect;
use game::hud::Hud;
use game_engine::flight::{FlyToExec, Target, plan_fly_to};
use game_engine::frames::FrameId;
use game_engine::universe::{CosmicWebParams, WebDescriptor, generate_cosmic_web};
use glam::{DQuat, DVec3};

/// Mouse-steer sensitivity (rad/px): gentler than the map orbit rate —
/// this turns the ship, not a camera.
pub const COSMIC_STEER_SENSITIVITY: f64 = 0.005;

/// Outcome of the fly-to toggle (drives notices + hint; a rejected
/// leg is a notice, never an error surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngageOutcome {
    /// No node selected — hint the player to click first.
    NoTarget,
    /// Fly-to committed on the selected node.
    Engaged,
    /// Planning rejected the leg (inside the arrival sphere, …).
    Failed,
}

/// Held thrust keys (WASD + arrows feed the same four flags).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldThrust {
    /// +X body (nose): `W` / `Up`.
    pub fwd: bool,
    /// −X body: `S` / `Down`.
    pub back: bool,
    /// −Z body (port): `A` / `Left`.
    pub left: bool,
    /// +Z body (starboard): `D` / `Right`.
    pub right: bool,
}

impl HeldThrust {
    /// Any thrust key held.
    pub fn any(self) -> bool {
        self.fwd || self.back || self.left || self.right
    }

    /// Clear all flags (key release / focus loss — keys never stick).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The demo tab's cosmic scene: one generated web + the player flying in
/// it + the player's camera + the live HUD view-model + input intent.
pub struct CosmicDemoState {
    /// Master seed driving stage 0 (shared with the galaxy/system maps:
    /// one seed regenerates the whole universe).
    pub seed: u64,
    /// Generation parameters (nominal; a future Settings tab may tune).
    pub params: CosmicWebParams,
    /// The generated web both surfaces render.
    pub web: WebDescriptor,
    /// Live player (ship + clock + fly-to slot + events).
    pub player: CosmicPlayerState,
    /// The player's own camera (tracks the ship every tick).
    pub camera: CosmicCamera,
    /// Shipping HUD view-model (SOI hysteresis lives here).
    pub hud: Hud,
    /// Held thrust keys for the next tick.
    pub held: HeldThrust,
    /// Buffer upload origin (rebase reference for camera + buffers).
    pub upload_origin: DVec3,
    /// Manual depth-fraction override (`None` = automatic occupancy).
    /// Test/dev plumbing (no UI in v0.3.2): pins the compressed regime
    /// without teleporting the ship — Mpc units cannot resolve
    /// real-time thrust inside a test-length run.
    pub depth_fraction_override: Option<f64>,
}

impl CosmicDemoState {
    /// Generate the web and spawn the player (boot path).
    pub fn new(seed: u64) -> Self {
        let params = CosmicWebParams::nominal();
        let web = generate_cosmic_web(seed, &params);
        let player = CosmicPlayerState::spawn(&web);
        let mut camera = CosmicCamera::new(params.descriptor_radius_mpc as f32);
        camera.track(player.position_mpc(), player.facing());
        let upload_origin = player.position_mpc();
        camera.set_render_origin(upload_origin);
        Self {
            seed,
            params,
            web,
            player,
            camera,
            hud: Hud::new(),
            held: HeldThrust::default(),
            upload_origin,
            depth_fraction_override: None,
        }
    }

    /// Regenerate everything from a new seed (`R` in the demo): the web,
    /// the player, the camera framing, and the HUD hysteresis.
    pub fn reseed(&mut self, seed: u64) {
        *self = Self::new(seed);
    }

    /// Steer the ship nose: drag-right turns right (world-Y yaw), drag
    /// down pitches down about the ship's starboard axis (FPS-style,
    /// non-inverted). Space has no up-reference to protect, so pitch
    /// accumulates freely.
    pub fn steer(&mut self, dx: f32, dy: f32) {
        let yaw = DQuat::from_rotation_y(-f64::from(dx) * COSMIC_STEER_SENSITIVITY);
        let nose = self.player.ship.chain.orientation() * DVec3::X;
        let up = DVec3::Y;
        let mut right = nose.cross(up);
        if right.length_squared() < 1e-12 {
            right = DVec3::Z;
        }
        let pitch =
            DQuat::from_axis_angle(right.normalize(), -f64::from(dy) * COSMIC_STEER_SENSITIVITY);
        let orientation = (pitch * yaw * self.player.ship.chain.orientation()).normalize();
        self.player.ship.chain.set_orientation(orientation);
    }

    /// Cruise intent for the next tick: held keys on body axes (zero =
    /// coast). The player turns this into a depth-scaled target velocity
    /// (orientation hold on the steered attitude — rotation stabilization
    /// is ON per the craft envelope).
    pub fn cruise_input(&self) -> (DVec3, bool) {
        let dir = DVec3::X * (f64::from(self.held.fwd as u8) - f64::from(self.held.back as u8))
            + DVec3::Z * (f64::from(self.held.right as u8) - f64::from(self.held.left as u8));
        (dir, self.held.any())
    }

    /// Nearest-node scan shared by the compression occupancy and the
    /// cruise scale: minimum depth fraction, closest distance, and that
    /// node's virial radius. O(nodes) per call — ~6k distance checks,
    /// µs-scale.
    fn scan_nodes(&self) -> Option<(f64, f64, f64)> {
        let pos = self.player.position_mpc();
        // (min depth fraction, nearest distance Mpc, its virial radius).
        let mut acc: Option<(f64, f64, f64)> = None;
        for node in &self.web.nodes {
            let dx = node.position_mpc[0] - pos.x;
            let dy = node.position_mpc[1] - pos.y;
            let dz = node.position_mpc[2] - pos.z;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            let r_vir = node.virial_radius_mpc.max(1e-6);
            let frac = dist / (10.0 * r_vir);
            acc = Some(match acc {
                Some((best_frac, best_dist, best_rvir)) => (
                    frac.min(best_frac),
                    // Lowest node index wins exact ties (the map-tab rule).
                    if dist < best_dist { dist } else { best_dist },
                    if dist < best_dist { r_vir } else { best_rvir },
                ),
                None => (frac, dist, r_vir),
            });
        }
        acc
    }

    /// Depth fraction for the compression occupancy: distance to the
    /// nearest node over ten of its virial radii (≤ 1 near a node ⇒
    /// real-time maneuvering; thousands deep in the void ⇒ full
    /// compression).
    pub fn depth_fraction(&self) -> f64 {
        self.scan_nodes().map(|(frac, _, _)| frac).unwrap_or(1.0)
    }

    /// Cruise scale length in Mpc: nearest-node distance with an
    /// r_vir-scaled floor (precision speeds at a node, never frozen).
    pub fn scale_length_mpc(&self) -> f64 {
        match self.scan_nodes() {
            Some((_, dist, r_vir)) => dist.max(CRUISE_SCALE_FLOOR_RVIR * r_vir),
            None => 1.0,
        }
    }

    /// Adjust the cruise pace: `Shift`+wheel scales `t_cross_s` by
    /// `factor` (a wheel notch passes ×/÷ the cruise-speed notch),
    /// clamped to the cruise-speed range. Returns the new crossing time
    /// for the HUD.
    pub fn cruise_speed_by(&mut self, factor: f64) -> f64 {
        debug_assert!(factor > 0.0 && factor.is_finite());
        let t = (self.player.cruise.t_cross_s * factor)
            .clamp(CRUISE_T_CROSS_MIN_S, CRUISE_T_CROSS_MAX_S);
        self.player.cruise.t_cross_s = t;
        t
    }

    /// One real frame: cruise from held input, then track the camera on
    /// the ship. Returns true when the ship outran the upload origin and
    /// the shell must rebuild the buffers (rebase path). Any held cruise
    /// input cancels a committed fly-to first (continuous hand-back —
    /// the freed step cruises the same tick).
    pub fn tick(&mut self, dt_real_s: f64) -> bool {
        let (dir, any) = self.cruise_input();
        if any {
            self.player.cancel_fly_to();
        }
        let scale = self.scale_length_mpc();
        let depth = self
            .depth_fraction_override
            .unwrap_or_else(|| self.depth_fraction());
        self.player.step_cruise(dir, scale, dt_real_s, depth);
        self.camera
            .track(self.player.position_mpc(), self.player.facing());
        self.needs_rebase()
    }

    /// Click-select a fly-to target: project every node through the
    /// player camera, keep the nearest projected point within
    /// [`COSMIC_PICK_RADIUS_PX`]. A hit sets the target and queues
    /// `TargetSelected`; a miss clears the target (no event). Lowest
    /// node index wins exact ties (the map-tab rule).
    pub fn select_node_at(&mut self, cursor: (f32, f32), vp: Rect) -> Option<u32> {
        let view_proj = self.camera.view_proj(vp.w / vp.h);
        let origin = self.upload_origin;
        let mut best: Option<(u32, f32)> = None;
        for node in &self.web.nodes {
            let world = glam::Vec3::new(
                (node.position_mpc[0] - origin.x) as f32,
                (node.position_mpc[1] - origin.y) as f32,
                (node.position_mpc[2] - origin.z) as f32,
            );
            if let Some((sx, sy)) = project_to_screen(world, view_proj, vp) {
                let d = (sx - cursor.0).hypot(sy - cursor.1);
                if d <= COSMIC_PICK_RADIUS_PX
                    && best.is_none_or(|(bi, bd)| d < bd || (d == bd && node.node_index < bi))
                {
                    best = Some((node.node_index, d));
                }
            }
        }
        match best.map(|(i, _)| i) {
            Some(i) => {
                self.player.target_node = Some(i);
                self.player
                    .events
                    .push(CosmicEvent::TargetSelected { node: i });
                Some(i)
            }
            None => {
                self.player.target_node = None;
                None
            }
        }
    }

    /// Engage fly-to on the selected node (`E`): plan from the ship at
    /// the current sim time, commit, and queue `FlyToStarted`. No
    /// target ⇒ [`EngageOutcome::NoTarget`]; a rejected leg ⇒
    /// [`EngageOutcome::Failed`] (notice, never a panic — e.g. the
    /// ship already sits inside the arrival sphere).
    pub fn engage_fly_to(&mut self) -> EngageOutcome {
        let Some(node) = self.player.target_node else {
            return EngageOutcome::NoTarget;
        };
        let Some(descriptor) = self.web.nodes.get(node as usize) else {
            return EngageOutcome::NoTarget;
        };
        let Ok(target) = Target::new(
            FrameId::Cosmological,
            glam::DVec3::from(descriptor.position_mpc),
        ) else {
            return EngageOutcome::Failed;
        };
        match plan_fly_to(
            FrameId::Cosmological,
            self.player.position_mpc(),
            &target,
            self.player.clock.sim_time_s(),
        ) {
            Ok(plan) => {
                let mut exec = FlyToExec::new(plan);
                if exec.commit().is_err() {
                    return EngageOutcome::Failed;
                }
                self.player.exec = Some(exec);
                self.player.events.push(CosmicEvent::FlyToStarted { node });
                EngageOutcome::Engaged
            }
            Err(_) => EngageOutcome::Failed,
        }
    }

    /// True when the ship sailed farther than
    /// [`REBASE_DISTANCE_MPC`](super::cosmic_camera::REBASE_DISTANCE_MPC)
    /// from the upload origin.
    pub fn needs_rebase(&self) -> bool {
        (self.player.position_mpc() - self.upload_origin).length()
            > super::cosmic_camera::REBASE_DISTANCE_MPC
    }

    /// Rebuild reference after a buffer re-upload at the ship: origin
    /// follows the ship (buffers are uploaded relative to it).
    pub fn rebased(&mut self) {
        self.upload_origin = self.player.position_mpc();
        self.camera.set_render_origin(self.upload_origin);
    }

    /// HUD target label: the selected node's content ID, if any (WS6
    /// selects; the demo shows the line meanwhile).
    pub fn target_label(&self) -> Option<String> {
        self.player
            .target_node
            .map(|node| self.web.node_content_id(node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> CosmicDemoState {
        CosmicDemoState::new(1234)
    }

    #[test]
    fn demo_boots_with_player_camera_and_hud_aligned() {
        let demo = demo();
        assert_eq!(demo.seed, 1234);
        assert!(!demo.web.nodes.is_empty());
        // Camera tracks the ship from the first frame, origin pinned at
        // the upload reference.
        assert_eq!(demo.camera.render_origin(), demo.upload_origin);
        assert_eq!(demo.upload_origin, demo.player.position_mpc());
        assert!(!demo.held.any());
        assert!(demo.player.events.is_empty());
    }

    #[test]
    fn reseed_regenerates_everything() {
        let mut demo = demo();
        let before = demo.player.position_mpc();
        demo.reseed(999);
        assert_eq!(demo.seed, 999);
        assert_eq!(demo.camera.render_origin(), demo.player.position_mpc());
        assert_ne!(demo.player.position_mpc(), before);
    }

    #[test]
    fn steer_turns_the_nose_with_the_drag() {
        let mut demo = demo();
        // Face +X first for a deterministic expectation.
        demo.player.ship.chain.set_orientation(DQuat::IDENTITY);
        demo.steer(100.0, 0.0);
        let nose = demo.player.facing();
        // Drag right → nose toward +Z (starboard).
        assert!(nose.z > 0.01, "drag right must turn right: {nose}");
        assert!((nose.length() - 1.0).abs() < 1e-12);
        demo.player.ship.chain.set_orientation(DQuat::IDENTITY);
        demo.steer(0.0, 100.0);
        let nose = demo.player.facing();
        // Drag down → nose toward −Y.
        assert!(nose.y < -0.01, "drag down must pitch down: {nose}");
    }

    #[test]
    fn cruise_intent_maps_held_keys_to_body_axes() {
        let mut demo = demo();
        let (idle_dir, idle_any) = demo.cruise_input();
        assert!(!idle_any);
        assert_eq!(idle_dir, DVec3::ZERO);
        demo.held.fwd = true;
        demo.held.right = true;
        let (dir, any) = demo.cruise_input();
        assert!(any);
        assert_eq!(dir, DVec3::new(1.0, 0.0, 1.0));
        demo.held.clear();
        assert!(!demo.held.any());
    }

    #[test]
    fn scale_length_is_floored_near_nodes() {
        use crate::cosmic_player::{CRUISE_SPEED_NOTCH, CRUISE_T_CROSS_S};

        let mut demo = demo();
        let spawn_scale = demo.scale_length_mpc();
        assert!(
            spawn_scale.is_finite() && spawn_scale > 0.0,
            "spawn scale {spawn_scale}"
        );
        // Parked on node 0, the scale is exactly the r_vir floor, never
        // zero (cruise stays alive at a fly-to arrival point).
        let node = &demo.web.nodes[0];
        demo.player
            .ship
            .chain
            .set_position(DVec3::from(node.position_mpc));
        let floor = CRUISE_SCALE_FLOOR_RVIR * node.virial_radius_mpc.max(1e-6);
        assert_eq!(demo.scale_length_mpc(), floor);
        // Speed adjust starts at the nominal pace, notches both ways,
        // and clamps at the range ends.
        assert_eq!(demo.player.cruise.t_cross_s, CRUISE_T_CROSS_S);
        assert!(demo.cruise_speed_by(1.0 / CRUISE_SPEED_NOTCH) < CRUISE_T_CROSS_S);
        assert_eq!(demo.cruise_speed_by(1e-9), CRUISE_T_CROSS_MIN_S);
        assert_eq!(demo.cruise_speed_by(1e9), CRUISE_T_CROSS_MAX_S);
    }

    #[test]
    fn depth_fraction_is_finite_and_positive() {
        // Mechanism check (compression magnitudes are pinned in
        // cosmic_player): nearest-node distance over ten virial radii.
        let demo = demo();
        let frac = demo.depth_fraction();
        assert!(frac.is_finite() && frac > 0.0, "fraction {frac}");
    }

    #[test]
    fn select_engage_cancel_and_cruise_cancel_flow() {
        use super::CosmicEvent;

        let mut demo = demo();
        let vp = Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        };
        let origin = demo.upload_origin;
        let view_proj = demo.camera.view_proj(800.0 / 600.0);
        // Click exactly on the first in-viewport node projection (the
        // demo camera faces the web, so thousands qualify).
        let mut hit = None;
        for node in &demo.web.nodes {
            let world = glam::Vec3::new(
                (node.position_mpc[0] - origin.x) as f32,
                (node.position_mpc[1] - origin.y) as f32,
                (node.position_mpc[2] - origin.z) as f32,
            );
            if let Some((sx, sy)) = project_to_screen(world, view_proj, vp)
                && (0.0..=800.0).contains(&sx)
                && (0.0..=600.0).contains(&sy)
            {
                hit = Some((node.node_index, (sx, sy)));
                break;
            }
        }
        let (idx, cursor) = hit.expect("some node must be clickable");
        let picked = demo.select_node_at(cursor, vp);
        assert_eq!(picked, Some(idx));
        assert_eq!(demo.player.target_node, Some(idx));
        // Engage commits the executor.
        assert_eq!(demo.engage_fly_to(), EngageOutcome::Engaged);
        assert!(demo.player.exec.is_some());
        // A miss clears the target (infinite cursor: no projection can
        // land within 8 px — deterministic miss by construction).
        assert_eq!(
            demo.select_node_at((f32::INFINITY, f32::INFINITY), vp),
            None
        );
        assert_eq!(demo.player.target_node, None);
        // Re-select and engage, then cruise-cancel mid-leg.
        assert_eq!(demo.select_node_at(cursor, vp), Some(idx));
        assert_eq!(demo.engage_fly_to(), EngageOutcome::Engaged);
        demo.held.fwd = true;
        demo.tick(1.0 / 60.0);
        assert!(demo.player.exec.is_none(), "held cruise must cancel fly-to");
        let events = demo.player.drain_events();
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                CosmicEvent::TargetSelected { .. } => "selected",
                CosmicEvent::FlyToStarted { .. } => "started",
                CosmicEvent::FlyToCancelled { .. } => "cancelled",
                CosmicEvent::FlyToCompleted { .. } => "completed",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["selected", "started", "selected", "started", "cancelled"],
            "event sequence wrong: {kinds:?}"
        );
    }

    #[test]
    fn engage_without_target_hints() {
        let mut demo = demo();
        assert_eq!(demo.player.target_node, None);
        assert_eq!(demo.engage_fly_to(), EngageOutcome::NoTarget);
        assert!(demo.player.exec.is_none());
        assert!(demo.player.drain_events().is_empty());
    }

    #[test]
    fn engage_on_top_of_target_fails_cleanly() {
        let mut demo = demo();
        // Park the ship exactly on node 0, select it, engage: the leg
        // is inside the arrival sphere — a notice, never a panic.
        let node_pos = glam::DVec3::from(demo.web.nodes[0].position_mpc);
        demo.player.ship.chain.set_position(node_pos);
        demo.player.target_node = Some(0);
        assert_eq!(demo.engage_fly_to(), EngageOutcome::Failed);
        assert!(demo.player.exec.is_none());
    }

    #[test]
    fn tick_cruises_visibly_and_tracks_camera() {
        // The playtest regression (update 2026-09-18-2027): five seconds
        // of W at spawn must move Mpc-scale with no test override — the
        // old thrust law needed `depth_fraction_override` to move at all
        // and still moved sub-ulp. The camera stays glued throughout.
        let mut demo = demo();
        demo.held.fwd = true;
        let p0 = demo.player.position_mpc();
        let mut rebased = false;
        for _ in 0..300 {
            rebased |= demo.tick(1.0 / 60.0);
        }
        let moved = (demo.player.position_mpc() - p0).length();
        assert!(
            moved > 1.0,
            "5 s of W must move Mpc-scale, moved {moved} Mpc"
        );
        assert!(!rebased, "five seconds must not outrun the origin");
        // Camera still glued to the ship: the live anchor (in the
        // origin-relative frame, mirroring the marker path) projects
        // to center.
        let anchor_rel =
            game_engine::frames::recenter(demo.player.position_mpc(), demo.camera.render_origin());
        let clip = demo.camera.view_proj(800.0 / 600.0) * glam::Vec4::from((anchor_rel, 1.0));
        let ndc = glam::Vec3::new(clip.x, clip.y, clip.z) / clip.w;
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5);
    }
}
