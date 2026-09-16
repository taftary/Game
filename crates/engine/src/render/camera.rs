//! Orbit camera: drag to rotate, scroll to zoom.
//!
//! Pure math over `glam` — no window, no GPU handle — so the M1 smoke,
//! the `debug-sphere-viewer` draft, and headless tests all share it.
//! World units are meters; the camera is decoupled from game logic
//! (VR future-proofing per `docs/game/scope.md`).

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

/// Clamp on orbit pitch: the camera never reaches the exact poles, where
/// yaw becomes degenerate.
pub const MAX_PITCH: f32 = 1.45;
/// Vertical field of view in radians.
pub const FOV_Y: f32 = 60.0_f32.to_radians();

/// Orbit camera around a target point (the planet center in M1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    target: Vec3,
    /// Distance from target in meters, clamped to `[min_distance,
    /// max_distance]`.
    distance: f32,
    /// Horizontal angle in radians.
    yaw: f32,
    /// Vertical angle in radians, clamped to `[-MAX_PITCH, MAX_PITCH]`.
    pitch: f32,
    min_distance: f32,
    max_distance: f32,
}

impl OrbitCamera {
    /// Creates an orbit camera. `distance` is clamped into
    /// `[min_distance, max_distance]`; `pitch` into `[-MAX_PITCH,
    /// MAX_PITCH]`.
    pub fn new(
        target: Vec3,
        distance: f32,
        yaw: f32,
        pitch: f32,
        min_distance: f32,
        max_distance: f32,
    ) -> Self {
        let mut camera = Self {
            target,
            distance,
            yaw,
            pitch,
            min_distance,
            max_distance,
        };
        camera.distance = camera.distance.clamp(min_distance, max_distance);
        camera.pitch = camera.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        camera
    }

    /// Default framing for a planet of `radius`: distance 3.2×R, clamped
    /// to the smoke range [1.6R, 8R].
    pub fn framing_planet(radius: f32) -> Self {
        Self::new(
            Vec3::ZERO,
            3.2 * radius,
            0.0,
            0.35,
            1.6 * radius,
            8.0 * radius,
        )
    }

    /// Drag-rotate: `dx`/`dy` are window pixels (y down-positive);
    /// 0.01 rad/px. Dragging up pitches the camera up toward the
    /// sphere's top (FPS-style, non-inverted); dragging right yaws
    /// with the drag.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.01;
        self.pitch = (self.pitch - dy * 0.01).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Scroll-zoom: positive `delta` moves closer (multiplicative).
    pub fn zoom(&mut self, delta: f32) {
        self.distance =
            (self.distance * (1.0 - 0.1 * delta)).clamp(self.min_distance, self.max_distance);
    }

    /// Camera position in world space.
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * cy, sp, cp * sy) * self.distance
    }

    /// View matrix (right-handed, camera looks at `target`).
    pub fn view_matrix(&self) -> Mat4 {
        look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// Perspective projection in framebuffer NDC (Z in `[0, 1]`, NDC
    /// +1 = top row — the app's proven convention: the UI ortho and
    /// the flat-map MVP both treat pixel row 0 as NDC +1, and picking
    /// unprojects the same way). This is the RH/ZO matrix **without**
    /// glam's Y-flip: `vulkan::perspective` (`directx` is the same
    /// convention) baked a Y-flip into the projection, which mirrored
    /// every 3D view vertically on screen (player-camera flip,
    /// 2026-09-16). The mesh's CCW-outward fans then classify as CCW
    /// front faces directly (`FrontFace::CounterClockwise`).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        perspective(FOV_Y, aspect, 0.05, 1000.0)
    }

    /// Current distance (post-clamp).
    pub fn distance(&self) -> f32 {
        self.distance
    }

    /// Current yaw.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Current pitch.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> OrbitCamera {
        OrbitCamera::framing_planet(1.0)
    }

    #[test]
    fn eye_sits_at_framing_distance_from_target() {
        let camera = camera();
        let offset = camera.eye() - Vec3::ZERO;
        assert!((offset.length() - 3.2).abs() < 1e-5);
    }

    #[test]
    fn zoom_clamps_to_configured_range() {
        let mut camera = camera();
        camera.zoom(1_000.0);
        assert_eq!(camera.distance(), 1.6);
        camera.zoom(-1_000.0);
        assert_eq!(camera.distance(), 8.0);
    }

    #[test]
    fn pitch_clamps_away_from_poles() {
        let mut camera = camera();
        // Drag up (negative window dy) pitches up, drag down pitches down.
        camera.rotate(0.0, -10_000.0);
        assert_eq!(camera.pitch(), MAX_PITCH);
        camera.rotate(0.0, 20_000.0);
        assert_eq!(camera.pitch(), -MAX_PITCH);
    }

    #[test]
    fn view_matrix_looks_at_target() {
        let camera = camera();
        let view = camera.view_matrix();
        // The target (origin) maps to -distance on view Z.
        let mapped = view * camera.target.extend(1.0);
        assert!((mapped.z + camera.distance()).abs() < 1e-4);
        assert!(mapped.x.abs() < 1e-5 && mapped.y.abs() < 1e-5);
    }

    #[test]
    fn projection_uses_vulkan_ndc() {
        use glam::camera::rh::proj::{directx, vulkan};
        let camera = camera();
        let aspect = 16.0 / 9.0;
        let projection = camera.projection_matrix(aspect);
        // Framebuffer-true NDC (Z in [0, 1], no Y-flip): matches the
        // DirectX/WebGPU constructor exactly.
        assert_eq!(
            projection,
            directx::perspective(FOV_Y, aspect, 0.05, 1000.0)
        );
        // And differs from glam's Y-flipped Vulkan constructor.
        assert_ne!(projection, vulkan::perspective(FOV_Y, aspect, 0.05, 1000.0));
    }
}
