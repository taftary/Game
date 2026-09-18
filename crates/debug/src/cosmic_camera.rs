//! Space camera + cosmic marker math (WS3).
//!
//! [`CosmicCamera`] is the player's own camera in the Cosmic Scale:
//! Chase (default; behind/above the marker, aligned to the ship),
//! Orbit (free look around the marker), FirstPerson (eye at the ship,
//! marker hidden by construction). Window- and GPU-free: the binary
//! turns cursor events into [`CosmicCamera::rotate`]/[`zoom`] calls and
//! pushes [`view_proj`] like the map cameras.
//!
//! Precision: the anchor is the ship's f64 Mpc position; eye/target are
//! built in f64 and recentered ([`frames::recenter`], ADR-013) so the
//! marker and the web share one camera-relative f32 frame. Projection is
//! the pinned un-flipped `directx::perspective` with MapOrbitCamera-style
//! derived near/far (fixed 0.05/1000 is invalid at Mpc).

use game_engine::frames::recenter;
use game_engine::render::FOV_Y;
use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{DQuat, DVec3, Mat4, Vec3};
use std::f32::consts::FRAC_PI_2;

/// Camera modes, `P` cycles Chase → Orbit → FirstPerson.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CosmicCameraMode {
    /// Behind/above the marker, aligned to the ship nose.
    Chase,
    /// Free orbit around the marker.
    Orbit,
    /// Eye at the ship, looking along the nose (marker hidden).
    FirstPerson,
}

impl CosmicCameraMode {
    /// Next mode in the `P` cycle.
    pub fn cycle(self) -> Self {
        match self {
            Self::Chase => Self::Orbit,
            Self::Orbit => Self::FirstPerson,
            Self::FirstPerson => Self::Chase,
        }
    }

    /// Short label for the HUD/mode pill (mirrors `camera_mode_label`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Chase => "chase",
            Self::Orbit => "orbit",
            Self::FirstPerson => "first",
        }
    }
}

/// Pitch clamp (same guard as `MapOrbitCamera::MAP_MAX_PITCH`).
pub const COSMIC_MAX_PITCH: f32 = FRAC_PI_2 - 0.02;
/// Drag sensitivity (rad/px), matching `OrbitCamera::rotate`.
pub const COSMIC_ROTATE_SENSITIVITY: f32 = 0.01;
/// Default chase eye offset behind/above the marker, Mpc.
pub const DEFAULT_CHASE_DISTANCE_MPC: f32 = 8.0;
/// Default orbit eye distance from the marker, Mpc.
pub const DEFAULT_ORBIT_DISTANCE_MPC: f32 = 30.0;
/// Chase eye height as a fraction of chase distance.
pub const CHASE_HEIGHT_FRACTION: f32 = 0.35;

/// Rebase trigger: the ship sailing farther than this (Mpc) from the
/// render origin rebuilds the buffers at the ship (WS4 tick).
pub const REBASE_DISTANCE_MPC: f64 = 50.0;

/// Player camera in the Cosmic Scale. Distances in Mpc (f32 is plenty:
/// every matrix is camera-relative; f64 values never enter a matrix
/// directly).
///
/// Two references: the **anchor** (live ship position — eye/target math)
/// and the **render origin** (upload origin `O` of the point/line
/// buffers). Both enter matrices only through [`recenter`]: buffers hold
/// `f32(P − O)`, the view holds `look_at(eye − O, target − O)`, so scene
/// and camera share one coherent frame. When the ship sails farther than
/// [`REBASE_DISTANCE_MPC`] from `O`, the shell rebuilds the buffers at
/// the ship and calls [`set_render_origin`](Self::set_render_origin) —
/// nearby precision stays sub-AU instead of degrading to the 15 kpc f32
/// quantum at the box rim.
#[derive(Clone, Debug, PartialEq)]
pub struct CosmicCamera {
    mode: CosmicCameraMode,
    /// Ship position in f64 Mpc (eye/target math reference).
    anchor: DVec3,
    /// Buffer upload origin in f64 Mpc (matrix recenter reference).
    origin: DVec3,
    /// Ship nose in frame axes (unit or zero).
    forward: DVec3,
    /// Chase eye offset behind the marker, Mpc.
    chase_distance: f32,
    /// Orbit eye distance, Mpc.
    orbit_distance: f32,
    /// Free-look yaw/pitch (Orbit mode only).
    yaw: f32,
    pitch: f32,
    /// Scene half-extent in Mpc (descriptor radius): drives near/far
    /// only, never framing.
    scene_radius: f32,
}

impl CosmicCamera {
    /// New camera in Chase mode; distances clamped to sane cosmic bands.
    pub fn new(scene_radius_mpc: f32) -> Self {
        Self {
            mode: CosmicCameraMode::Chase,
            anchor: DVec3::ZERO,
            origin: DVec3::ZERO,
            forward: DVec3::X,
            chase_distance: DEFAULT_CHASE_DISTANCE_MPC.clamp(0.5, 100.0),
            orbit_distance: DEFAULT_ORBIT_DISTANCE_MPC.clamp(1.0, 500.0),
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.6,
            scene_radius: scene_radius_mpc.max(f32::EPSILON),
        }
    }

    /// Current mode.
    pub fn mode(&self) -> CosmicCameraMode {
        self.mode
    }

    /// `P` cycle.
    pub fn cycle(&mut self) {
        self.mode = self.mode.cycle();
    }

    /// Track the ship (called every tick from player state). The render
    /// origin is untouched — it moves only on buffer rebuilds.
    pub fn track(&mut self, anchor_mpc: DVec3, forward: DVec3) {
        self.anchor = anchor_mpc;
        self.forward = forward;
    }

    /// Pin the matrix recenter reference to a fresh buffer upload
    /// origin (rebase path).
    pub fn set_render_origin(&mut self, origin_mpc: DVec3) {
        self.origin = origin_mpc;
    }

    /// Current render origin (buffer upload reference).
    pub fn render_origin(&self) -> DVec3 {
        self.origin
    }

    /// Drag-rotate: Orbit mode only (Chase/FirstPerson follow the ship —
    /// the mouse steers the ship there, wired in WS4).
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        if self.mode != CosmicCameraMode::Orbit {
            return;
        }
        self.yaw += dx * COSMIC_ROTATE_SENSITIVITY;
        self.pitch = (self.pitch - dy * COSMIC_ROTATE_SENSITIVITY)
            .clamp(-COSMIC_MAX_PITCH, COSMIC_MAX_PITCH);
    }

    /// Scroll-zoom: Chase adjusts chase distance, Orbit the orbit
    /// distance, FirstPerson ignores (eye is at the ship).
    pub fn zoom(&mut self, factor: f32) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        match self.mode {
            CosmicCameraMode::Chase => {
                self.chase_distance = (self.chase_distance * factor).clamp(0.5, 100.0);
            }
            CosmicCameraMode::Orbit => {
                self.orbit_distance = (self.orbit_distance * factor).clamp(1.0, 500.0);
            }
            CosmicCameraMode::FirstPerson => {}
        }
    }

    /// Eye position in f64 Mpc (world truth; recentered before upload).
    pub fn eye_world(&self) -> DVec3 {
        let forward = self.forward_or_x();
        match self.mode {
            CosmicCameraMode::Chase => {
                // Behind the marker (against the nose) and above it.
                self.anchor - forward * f64::from(self.chase_distance)
                    + DVec3::Y * f64::from(self.chase_distance * CHASE_HEIGHT_FRACTION)
            }
            CosmicCameraMode::Orbit => {
                let (sy, cy) = f64::from(self.yaw).sin_cos();
                let (sp, cp) = f64::from(self.pitch).sin_cos();
                self.anchor + DVec3::new(cp * cy, sp, cp * sy) * f64::from(self.orbit_distance)
            }
            CosmicCameraMode::FirstPerson => self.anchor,
        }
    }

    /// Look target in f64 Mpc: the marker in Chase/Orbit, ahead of the
    /// nose in FirstPerson.
    fn target_world(&self) -> DVec3 {
        match self.mode {
            CosmicCameraMode::Chase | CosmicCameraMode::Orbit => self.anchor,
            CosmicCameraMode::FirstPerson => self.anchor + self.forward_or_x(),
        }
    }

    /// Eye-to-anchor distance driving the near/far policy (the framing
    /// scale in every mode).
    fn framing_distance(&self) -> f32 {
        match self.mode {
            CosmicCameraMode::Chase => self.chase_distance,
            CosmicCameraMode::Orbit => self.orbit_distance,
            CosmicCameraMode::FirstPerson => self.chase_distance,
        }
    }

    /// View matrix in the camera-relative f32 frame (eye/target
    /// recentered on the render origin — ADR-013; coherent with
    /// buffers uploaded relative to the same origin).
    pub fn view_matrix(&self) -> Mat4 {
        let eye = recenter(self.eye_world(), self.origin);
        let target = recenter(self.target_world(), self.origin);
        look_at_mat4(eye, target, self.up_for(eye, target))
    }

    /// Perspective projection (un-flipped `directx`, never Vulkan):
    /// near/far derive from framing distance + scene radius (the
    /// MapOrbitCamera policy — only clipping is affected; the web
    /// pipelines run with no depth state).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let dist = self.framing_distance();
        let near = (dist * 0.01).max(self.scene_radius * 1e-4);
        let far = dist + 4.0 * self.scene_radius;
        perspective(FOV_Y, aspect, near, far.max(near * 2.0))
    }

    /// Combined matrix the renderer pushes (`projection * view`).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// Pixels per Mpc at the anchor depth: the point-shader scale for
    /// world-sized sprites (mirrors `MapOrbitCamera::px_scale`).
    pub fn px_scale(&self, vp_h: f32) -> f32 {
        vp_h.max(1.0) / (2.0 * (FOV_Y * 0.5).tan())
    }

    /// Current chase distance (post-clamp).
    pub fn chase_distance(&self) -> f32 {
        self.chase_distance
    }

    /// Current orbit distance (post-clamp).
    pub fn orbit_distance(&self) -> f32 {
        self.orbit_distance
    }

    fn forward_or_x(&self) -> DVec3 {
        if self.forward.length_squared() > 1e-24 {
            self.forward.normalize()
        } else {
            DVec3::X
        }
    }

    /// Up vector with parallel fallback (looking straight up/down):
    /// +Y unless the view direction is within ~0.1% of ±Y, then +Z.
    fn up_for(&self, eye: Vec3, target: Vec3) -> Vec3 {
        let view = (target - eye).normalize_or_zero();
        if view.y.abs() > 0.999_999 {
            Vec3::Z
        } else {
            Vec3::Y
        }
    }
}

/// Cosmic marker tip: from the camera-relative ship position along the
/// camera-relative nose at an eye-distance-proportional length (the
/// `sphere_tip_world` rule — readable once the UI clamps it to pixels).
/// Pure math so the demo draw and its regression tests share it.
pub fn cosmic_tip_world(pos_rel: Vec3, facing_rel: Vec3, eye_dist: f32) -> Vec3 {
    pos_rel + facing_rel.normalize_or_zero() * eye_dist.max(1e-6) * 0.08
}

/// Facing direction as a rotation (nose = body +X): shared with the
/// player spawn orientation for tests.
pub fn facing_quat_for(dir: DVec3) -> DQuat {
    if dir.dot(DVec3::X) < -0.999_999 {
        DQuat::from_rotation_y(std::f64::consts::PI)
    } else {
        DQuat::from_rotation_arc(DVec3::X, dir.normalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    fn camera() -> CosmicCamera {
        let mut cam = CosmicCamera::new(250.0);
        cam.track(DVec3::new(200.0, -40.0, 80.0), DVec3::X);
        // Mirrors WS4 usage: buffers upload at the tracked position.
        cam.set_render_origin(DVec3::new(200.0, -40.0, 80.0));
        cam
    }

    #[test]
    fn mode_cycle_covers_all_three() {
        let mut cam = CosmicCamera::new(250.0);
        assert_eq!(cam.mode(), CosmicCameraMode::Chase);
        cam.cycle();
        assert_eq!(cam.mode(), CosmicCameraMode::Orbit);
        cam.cycle();
        assert_eq!(cam.mode(), CosmicCameraMode::FirstPerson);
        cam.cycle();
        assert_eq!(cam.mode(), CosmicCameraMode::Chase);
        assert_eq!(CosmicCameraMode::Chase.label(), "chase");
        assert_eq!(CosmicCameraMode::Orbit.label(), "orbit");
        assert_eq!(CosmicCameraMode::FirstPerson.label(), "first");
    }

    #[test]
    fn projection_uses_unflipped_perspective() {
        use glam::camera::rh::proj::{directx, vulkan};
        let cam = camera();
        let aspect = 16.0 / 9.0;
        let near = (8.0f32 * 0.01).max(250.0 * 1e-4);
        let far = 8.0 + 4.0 * 250.0;
        assert_eq!(
            cam.projection_matrix(aspect),
            directx::perspective(FOV_Y, aspect, near, far)
        );
        assert_ne!(
            cam.projection_matrix(aspect),
            vulkan::perspective(FOV_Y, aspect, near, far)
        );
    }

    #[test]
    fn chase_centers_the_anchor_at_any_offset() {
        // Precision contract (DoD 7): the ship sits at NDC center
        // whether the anchor is at the origin or 250 Mpc out — the
        // camera-relative frame absorbs the offset. A stale render
        // origin (pre-rebase) still centers the anchor: look_at
        // centers its target by construction.
        for anchor in [
            DVec3::ZERO,
            DVec3::new(100.0, -30.0, 45.0),
            DVec3::new(250.0, 250.0, -250.0),
        ] {
            let mut cam = CosmicCamera::new(250.0);
            cam.track(anchor, DVec3::X);
            cam.set_render_origin(anchor);
            let clip = cam.view_proj(800.0 / 600.0) * Vec4::new(0.0, 0.0, 0.0, 1.0);
            assert!(clip.w > 0.0, "anchor behind camera at {anchor}");
            let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
            assert!(
                ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5,
                "anchor off-center at {anchor}: {ndc}"
            );
        }
        // Stale origin 40 Mpc away (pre-rebase worst case): the anchor
        // still projects to center (project its origin-relative frame
        // position, mirroring the WS4 marker path).
        let mut stale = CosmicCamera::new(250.0);
        stale.track(DVec3::new(200.0, 0.0, 0.0), DVec3::X);
        stale.set_render_origin(DVec3::new(160.0, 0.0, 0.0));
        let anchor_rel = recenter(DVec3::new(200.0, 0.0, 0.0), DVec3::new(160.0, 0.0, 0.0));
        let clip = stale.view_proj(800.0 / 600.0) * Vec4::from((anchor_rel, 1.0));
        let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
        assert!(
            ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5,
            "stale origin decentered the anchor: {ndc}"
        );
    }

    #[test]
    fn first_person_eye_sits_on_the_anchor() {
        // The marker-hide mechanism (WS4 pins world_to_pixels ⇒ None):
        // eye == anchor puts the ship position exactly in the eye
        // plane, so clip.w == 0 there by construction.
        let mut cam = camera();
        cam.cycle();
        cam.cycle();
        assert_eq!(cam.mode(), CosmicCameraMode::FirstPerson);
        assert_eq!(cam.eye_world(), cam.anchor);
        let clip = cam.view_proj(800.0 / 600.0) * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!(
            clip.w.abs() < 1e-6,
            "ship must sit in the eye plane, w = {}",
            clip.w
        );
    }

    #[test]
    fn orbit_ignores_nothing_and_chase_ignores_rotate() {
        let mut cam = camera();
        cam.cycle(); // Orbit
        let (y0, p0) = (cam.yaw, cam.pitch);
        cam.rotate(10.0, -10.0);
        assert!((cam.yaw - (y0 + 0.1)).abs() < 1e-6);
        assert!((cam.pitch - (p0 + 0.1)).abs() < 1e-6);
        cam.zoom(0.5);
        assert!((cam.orbit_distance - 15.0).abs() < 1e-5);
        cam.cycle();
        cam.cycle(); // back to Chase
        let (y1, p1) = (cam.yaw, cam.pitch);
        cam.rotate(10.0, -10.0);
        assert_eq!((cam.yaw, cam.pitch), (y1, p1), "chase ignores rotate");
    }

    #[test]
    fn tip_scales_with_eye_distance() {
        let pos = Vec3::ZERO;
        let tip = cosmic_tip_world(pos, Vec3::X, 100.0);
        assert_eq!(tip, Vec3::new(8.0, 0.0, 0.0));
        let near = cosmic_tip_world(pos, Vec3::X, 1.0);
        assert_eq!(near, Vec3::new(0.08, 0.0, 0.0));
    }

    #[test]
    fn px_scale_matches_map_camera_policy() {
        let cam = camera();
        let expected = 600.0 / (2.0 * (FOV_Y * 0.5).tan());
        assert!((cam.px_scale(600.0) - expected).abs() < 1e-6);
    }

    #[test]
    fn facing_quat_points_the_nose() {
        let q = facing_quat_for(DVec3::new(0.0, 0.0, 5.0));
        let nose = q * DVec3::X;
        assert!((nose - DVec3::Z).length() < 1e-12);
        // Antiparallel guard: −X maps through +Y half-turn, stays unit.
        let flip = facing_quat_for(DVec3::NEG_X);
        assert!((flip * DVec3::X - DVec3::NEG_X).length() < 1e-9);
        assert!((flip.length_squared() - 1.0).abs() < 1e-12);
    }
}
