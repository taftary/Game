//! Shared 3D orbit camera for the universe maps (universe-maps-3d).
//!
//! Pure math over `glam` — no window, no GPU handle — so the windowed
//! binary turns cursor events into orbit/pan/zoom state through these
//! functions and every behavior stays unit-testable. Unit-free: the
//! galaxy view instantiates it in compressed ly, the system view in AU.
//!
//! World embedding (both maps): `world = (east, up, −north)` — the
//! engine ENU/player frame (y-up, north = −z). Chosen so a camera
//! south of its target and above the plane shows north up and east
//! right, i.e. the classic top-down map read, from a natural 3D view.
//! The default / top-down yaw ([`DEFAULT_YAW`]) is the south approach
//! for exactly this reason.
//!
//! Projection is the app's pinned un-flipped perspective
//! ([`FOV_Y`], glam `directx::perspective` — RH, Z ∈ [0, 1], no Y-flip,
//! the same convention `OrbitCamera` proves); points and lines carry no
//! faces, so no front-face/cull state is involved.

use game_engine::render::FOV_Y;
use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

/// Pitch clamp: the camera never reaches the exact poles, where the
/// up-vector degenerates (same guard as `OrbitCamera::MAX_PITCH`,
/// slightly nearer the pole so the top-down snap reads flat).
pub const MAP_MAX_PITCH: f32 = FRAC_PI_2 - 0.02;

/// Default / top-down yaw: camera south of the target (eye at +z in
/// the `(east, up, −north)` embedding), so screen-right is east and
/// screen-up is north.
pub const DEFAULT_YAW: f32 = FRAC_PI_2;

/// Default opening tilt (radians from the plane): a 3D read that keeps
/// the map conventions (north up-ish, east right).
pub const DEFAULT_PITCH: f32 = 50.0 * PI / 180.0;

/// Drag sensitivity (rad/px), matching `OrbitCamera::rotate`.
pub const ROTATE_SENSITIVITY: f32 = 0.01;

/// 3D orbit camera around a map-space target. `distance` is the
/// eye-to-target range in map units, clamped to
/// `[min_distance, max_distance]`; `pitch` clamps to
/// `±MAP_MAX_PITCH`.
#[derive(Clone, Debug, PartialEq)]
pub struct MapOrbitCamera {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
    min_distance: f32,
    max_distance: f32,
    /// Scene half-extent in map units (disk radius + margin): drives
    /// the perspective near/far planes only, never the framing.
    scene_radius: f32,
    /// Remembered (yaw, pitch) for the top-down toggle.
    saved_tilt: (f32, f32),
    /// Per-instance vertical FOV, radians (`cosmic-depth-window`:
    /// slab mode narrows the inspector to 20° for the near-
    /// orthographic target framing). Defaults to [`FOV_Y`]; every
    /// derived quantity (`projection_matrix`, `px_scale`,
    /// `world_per_pixel`, `project_to_screen`) reads it, so picking
    /// and drawing always share one matrix.
    fov_y: f32,
}

impl MapOrbitCamera {
    /// Creates an orbit camera. `distance` clamps into
    /// `[min_distance, max_distance]`; `pitch` into
    /// `±MAP_MAX_PITCH`.
    pub fn new(
        target: Vec3,
        distance: f32,
        yaw: f32,
        pitch: f32,
        min_distance: f32,
        max_distance: f32,
        scene_radius: f32,
    ) -> Self {
        let mut camera = Self {
            target,
            distance,
            yaw,
            pitch,
            min_distance,
            max_distance,
            scene_radius: scene_radius.max(f32::EPSILON),
            saved_tilt: (yaw, pitch),
            fov_y: FOV_Y,
        };
        camera.distance = camera
            .distance
            .clamp(min_distance, max_distance.max(min_distance));
        camera.pitch = camera.pitch.clamp(-MAP_MAX_PITCH, MAP_MAX_PITCH);
        camera.saved_tilt = (camera.yaw, camera.pitch);
        camera
    }

    /// Distance whose vertical half-extent is `h` map units under
    /// [`FOV_Y`]: the bridge from the old ortho half-extents to orbit
    /// distances, so per-map clamps mirror the 2D ranges.
    pub fn distance_for_half_height(h: f32) -> f32 {
        h / (FOV_Y * 0.5).tan()
    }

    /// Drag-rotate: `dx`/`dy` are window pixels (y down-positive).
    /// Dragging up pitches the camera up; dragging right yaws with the
    /// drag (same convention as `OrbitCamera::rotate`).
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * ROTATE_SENSITIVITY;
        self.pitch = (self.pitch - dy * ROTATE_SENSITIVITY).clamp(-MAP_MAX_PITCH, MAP_MAX_PITCH);
    }

    /// View-plane pan: `dx`/`dy` are window pixels (y down-positive),
    /// `vp_h` the viewport height in pixels. Content follows the
    /// cursor — the picked map point stays under the cursor.
    pub fn pan_screen(&mut self, dx: f32, dy: f32, vp_h: f32) {
        let wpp = self.world_per_pixel(vp_h);
        let forward = (self.target - self.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        // Screen +x is world `right` (target moves against the drag);
        // screen +y-down is world −`up`.
        self.target = self.target - right * (dx * wpp) + up * (dy * wpp);
        if !self.target.is_finite() {
            self.target = Vec3::ZERO;
        }
    }

    /// Scroll-zoom: `factor < 1` moves closer (multiplicative, like the
    /// old log zoom); clamped to `[min_distance, max_distance]`.
    pub fn zoom_by(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = (self.distance * factor).clamp(self.min_distance, self.max_distance);
        }
    }

    /// Jump the distance (focus jumps, reframe); clamped to
    /// `[min_distance, max_distance]`.
    pub fn set_distance(&mut self, distance: f32) {
        if distance.is_finite() && distance > 0.0 {
            self.distance = distance.clamp(self.min_distance, self.max_distance);
        }
    }

    /// Toggle the top-down snap: first call stores the current tilt
    /// and moves to the south-approach near-pole view (the classic 2D
    /// framing: north up, east right); second call restores the tilt.
    pub fn toggle_top_down(&mut self) {
        if (self.pitch - MAP_MAX_PITCH).abs() < 1e-4 {
            (self.yaw, self.pitch) = self.saved_tilt;
        } else {
            self.saved_tilt = (self.yaw, self.pitch);
            self.yaw = DEFAULT_YAW;
            self.pitch = MAP_MAX_PITCH;
        }
    }

    /// Camera position in map space.
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * cy, sp, cp * sy) * self.distance
    }

    /// View matrix (right-handed, camera looks at `target`, up +Y).
    pub fn view_matrix(&self) -> Mat4 {
        look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// Perspective projection in framebuffer NDC (Z in `[0, 1]`, NDC
    /// +1 = top — the un-flipped `directx` constructor, never glam's
    /// Y-flipped `vulkan::perspective`). Near/far derive from the
    /// current distance + scene radius: only clipping is affected (the
    /// map pipelines run with no depth state).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let near = (self.distance * 0.01).max(self.scene_radius * 1e-4);
        let far = self.distance + 4.0 * self.scene_radius;
        perspective(self.fov_y, aspect, near, far.max(near * 2.0))
    }

    /// Combined matrix the renderer pushes (`projection * view`).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// Map units per screen pixel at the target depth (pan scale).
    pub fn world_per_pixel(&self, vp_h: f32) -> f32 {
        2.0 * self.distance * (self.fov_y * 0.5).tan() / vp_h.max(1.0)
    }

    /// Pixels per map unit at the target depth: the point-shader
    /// scale for world-sized sprites (`px = world_size * px_scale /
    /// clip.w`).
    pub fn px_scale(&self, vp_h: f32) -> f32 {
        vp_h.max(1.0) / (2.0 * (self.fov_y * 0.5).tan())
    }

    /// Instance vertical FOV, radians (default [`FOV_Y`]).
    pub fn fov_y(&self) -> f32 {
        self.fov_y
    }

    /// Narrow (or widen) the FOV to `fov_deg` while keeping the framed
    /// width constant: distance rescales by `tan(old/2)/tan(new/2)`
    /// (clamped to the zoom band). Toggling back restores both.
    pub fn set_fov_keep_framing(&mut self, fov_deg: f32) {
        if !fov_deg.is_finite() {
            return;
        }
        let new_fov = (fov_deg.to_radians()).clamp(1.0_f32.to_radians(), 89.0_f32.to_radians());
        let scale = (self.fov_y * 0.5).tan() / (new_fov * 0.5).tan();
        if scale.is_finite() && scale > 0.0 {
            self.fov_y = new_fov;
            self.distance = (self.distance * scale).clamp(self.min_distance, self.max_distance);
        }
    }

    /// Current target.
    pub fn target(&self) -> Vec3 {
        self.target
    }

    /// Move the target (focus jumps, reframe).
    pub fn set_target(&mut self, target: Vec3) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// Current distance (post-clamp).
    pub fn distance(&self) -> f32 {
        self.distance
    }

    /// Closest zoom for this camera (post-clamp).
    pub fn min_distance(&self) -> f32 {
        self.min_distance
    }

    /// Current yaw.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Current pitch.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// Widest zoom for this camera (post-clamp).
    pub fn max_distance(&self) -> f32 {
        self.max_distance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec3, Vec4};

    fn camera() -> MapOrbitCamera {
        MapOrbitCamera::new(
            Vec3::ZERO,
            100.0,
            DEFAULT_YAW,
            DEFAULT_PITCH,
            10.0,
            200.0,
            120.0,
        )
    }

    #[test]
    fn constructor_clamps_distance_and_pitch() {
        let near = MapOrbitCamera::new(Vec3::ZERO, 1.0, 0.0, 0.0, 10.0, 200.0, 120.0);
        assert_eq!(near.distance(), 10.0);
        let far = MapOrbitCamera::new(Vec3::ZERO, 1e9, 0.0, 0.0, 10.0, 200.0, 120.0);
        assert_eq!(far.distance(), 200.0);
        let pole = MapOrbitCamera::new(Vec3::ZERO, 100.0, 0.0, 10.0, 10.0, 200.0, 120.0);
        assert_eq!(pole.pitch(), MAP_MAX_PITCH);
    }

    #[test]
    fn rotate_matches_orbit_camera_convention() {
        // Drag up (negative window dy) pitches up, drag down pitches
        // down; dragging right yaws with the drag.
        let mut cam = camera();
        let (y0, p0) = (cam.yaw(), cam.pitch());
        cam.rotate(10.0, -10.0);
        assert!((cam.yaw() - (y0 + 0.1)).abs() < 1e-6);
        assert!((cam.pitch() - (p0 + 0.1)).abs() < 1e-6);
        cam.rotate(0.0, 100_000.0);
        assert_eq!(cam.pitch(), -MAP_MAX_PITCH);
        cam.rotate(0.0, -100_000.0);
        assert_eq!(cam.pitch(), MAP_MAX_PITCH);
    }

    #[test]
    fn zoom_is_multiplicative_and_clamped() {
        let mut cam = camera();
        cam.zoom_by(0.5);
        assert!((cam.distance() - 50.0).abs() < 1e-5);
        cam.zoom_by(0.0);
        assert!(
            (cam.distance() - 50.0).abs() < 1e-5,
            "non-positive factor is a no-op"
        );
        cam.zoom_by(1e9);
        assert_eq!(cam.distance(), 200.0);
        cam.zoom_by(1e-9);
        assert_eq!(cam.distance(), 10.0);
    }

    #[test]
    fn pan_moves_target_in_the_view_plane() {
        let mut cam = camera();
        // Pure horizontal drag moves the target along world ±right
        // only (no vertical drift).
        cam.pan_screen(100.0, 0.0, 600.0);
        let moved = cam.target() - Vec3::ZERO;
        assert!(
            moved.y.abs() < 1e-5,
            "horizontal pan must not drift vertically"
        );
        assert!(moved.length() > 0.0);
        // Content follows the cursor: dragging by (+dx, +dy) moves the
        // fixed world point under the cursor by the same screen delta.
        let mut cam = camera();
        let vp = (0.0, 0.0, 800.0, 600.0);
        let aspect = vp.2 / vp.3;
        let project = |cam: &MapOrbitCamera, world: Vec3| {
            let clip = cam.view_proj(aspect) * world.extend(1.0);
            let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
            (
                vp.0 + (ndc.x + 1.0) * 0.5 * vp.2,
                vp.1 + (1.0 - ndc.y) * 0.5 * vp.3,
            )
        };
        let before = project(&cam, Vec3::ZERO);
        cam.pan_screen(40.0, -25.0, vp.3);
        let after = project(&cam, Vec3::ZERO);
        assert!(
            (after.0 - (before.0 + 40.0)).abs() < 1.0,
            "{after:?} vs {before:?}"
        );
        assert!(
            (after.1 - (before.1 - 25.0)).abs() < 1.0,
            "{after:?} vs {before:?}"
        );
    }

    #[test]
    fn set_distance_jumps_with_clamp() {
        let mut cam = camera();
        cam.set_distance(50.0);
        assert!((cam.distance() - 50.0).abs() < 1e-6);
        cam.set_distance(1e9);
        assert_eq!(cam.distance(), 200.0);
        cam.set_distance(-5.0);
        assert!(
            (cam.distance() - 200.0).abs() < 1e-6,
            "non-positive is a no-op"
        );
    }

    #[test]
    fn top_down_toggle_snaps_and_restores() {
        let mut cam = camera();
        let (yaw, pitch) = (cam.yaw(), cam.pitch());
        cam.toggle_top_down();
        assert_eq!(cam.yaw(), DEFAULT_YAW);
        assert_eq!(cam.pitch(), MAP_MAX_PITCH);
        cam.toggle_top_down();
        assert!((cam.yaw() - yaw).abs() < 1e-6);
        assert!((cam.pitch() - pitch).abs() < 1e-6);
    }

    #[test]
    fn projection_uses_unflipped_perspective() {
        use glam::camera::rh::proj::{directx, vulkan};
        let cam = camera();
        let aspect = 16.0 / 9.0;
        let projection = cam.projection_matrix(aspect);
        let near = (100.0f32 * 0.01).max(120.0 * 1e-4);
        let far = 100.0 + 4.0 * 120.0;
        // Framebuffer-true NDC (Z in [0, 1], no Y-flip).
        assert_eq!(projection, directx::perspective(FOV_Y, aspect, near, far));
        // And differs from glam's Y-flipped Vulkan constructor.
        assert_ne!(projection, vulkan::perspective(FOV_Y, aspect, near, far));
    }

    #[test]
    fn target_projects_to_viewport_center() {
        let cam = camera();
        let clip = cam.view_proj(800.0 / 600.0) * Vec4::new(0.0, 0.0, 0.0, 1.0);
        let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5);
        assert!((0.0..=1.0).contains(&ndc.z));
    }

    #[test]
    fn default_framing_keeps_map_conventions() {
        // South approach + above the plane: a point NORTH of target
        // (−z world) projects UP-screen; a point EAST (+x world)
        // projects RIGHT-screen. North = −z is the world embedding the
        // views upload (map z flips sign).
        let cam = camera();
        let view_proj = cam.view_proj(800.0 / 600.0);
        let screen = |world: Vec3| {
            let clip = view_proj * world.extend(1.0);
            let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
            ((ndc.x + 1.0) * 400.0, (1.0 - ndc.y) * 300.0)
        };
        let (cx, cy) = screen(Vec3::ZERO);
        let (ex, ey) = screen(Vec3::new(10.0, 0.0, 0.0));
        assert!(ex > cx && (ey - cy).abs() < 1.0, "east must be right");
        let (nx, ny) = screen(Vec3::new(0.0, 0.0, -10.0));
        assert!(ny < cy && (nx - cx).abs() < 1.0, "north must be up");
        // Top-down snap preserves the conventions exactly.
        let mut top = camera();
        top.toggle_top_down();
        let view_proj = top.view_proj(800.0 / 600.0);
        let screen = |world: Vec3| {
            let clip = view_proj * world.extend(1.0);
            let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
            ((ndc.x + 1.0) * 400.0, (1.0 - ndc.y) * 300.0)
        };
        let (cx, cy) = screen(Vec3::ZERO);
        assert!((cx - 400.0).abs() < 1.0 && (cy - 300.0).abs() < 1.0);
        let (ex, ey) = screen(Vec3::new(10.0, 0.0, 0.0));
        assert!(
            ex > cx && (ey - cy).abs() < 1.0,
            "east must be right top-down"
        );
        let (nx, ny) = screen(Vec3::new(0.0, 0.0, -10.0));
        assert!(
            ny < cy && (nx - cx).abs() < 1.0,
            "north must be up top-down"
        );
    }

    #[test]
    fn eye_sits_at_framing_distance_from_target() {
        let target = Vec3::new(1.0, 2.0, 3.0);
        let cam = MapOrbitCamera::new(target, 100.0, 0.3, 0.2, 10.0, 200.0, 120.0);
        assert!((cam.eye() - target).length() - 100.0 < 1e-4);
    }

    #[test]
    fn fov_defaults_to_shared_value() {
        assert_eq!(camera().fov_y(), FOV_Y);
    }

    #[test]
    fn set_fov_keep_framing_preserves_width() {
        // Generous zoom band so the ×3.27 distance rescale never clamps.
        let mut cam = MapOrbitCamera::new(Vec3::ZERO, 100.0, 0.3, 0.2, 10.0, 2000.0, 120.0);
        let width = |cam: &MapOrbitCamera| 2.0 * cam.distance() * (cam.fov_y() * 0.5).tan();
        let before = width(&cam);
        cam.set_fov_keep_framing(20.0);
        assert!((cam.fov_y() - 20.0_f32.to_radians()).abs() < 1e-6);
        assert!(
            (width(&cam) - before).abs() / before < 0.01,
            "framed width drifted: {} vs {before}",
            width(&cam)
        );
        // Toggling back restores both.
        cam.set_fov_keep_framing(60.0);
        assert!((cam.fov_y() - FOV_Y).abs() < 1e-6);
        assert!((width(&cam) - before).abs() / before < 0.01);
        assert!((cam.distance() - 100.0).abs() / 100.0 < 0.01);
        // Degenerate input is a no-op.
        cam.set_fov_keep_framing(f32::NAN);
        assert!((cam.fov_y() - FOV_Y).abs() < 1e-6);
    }

    #[test]
    fn projection_stays_unflipped_at_any_fov() {
        use glam::camera::rh::proj::directx;
        let mut cam = camera();
        for fov in [60.0, 20.0, 25.0] {
            cam.set_fov_keep_framing(fov);
            let aspect = 16.0 / 9.0;
            let near = (cam.distance() * 0.01).max(120.0 * 1e-4);
            let far = cam.distance() + 4.0 * 120.0;
            assert_eq!(
                cam.projection_matrix(aspect),
                directx::perspective(cam.fov_y(), aspect, near, far),
                "projection must stay directx at {fov}°"
            );
        }
    }
}
