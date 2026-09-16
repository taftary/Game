//! Switchable cameras: a free global orbit plus three player-relative modes.
//!
//! Views are derived from the [`Player`](crate::player::Player) every frame,
//! so all modes track movement for free and the flat map can recenter on the
//! player with the same [`visible_hemisphere`](game_engine::render::visible_hemisphere)
//! rule the debug viewer uses today.

use game_engine::render::{MAX_PITCH, OrbitCamera};
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::player::Player;

/// Active camera viewpoint. One key cycles through all four in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMode {
    /// Orbit locked onto the player; drag rotates, wheel zooms.
    #[default]
    Follow,
    /// At the player's eyes, looking along the facing direction.
    FirstPerson,
    /// Behind and above the player, tracking position and facing.
    ThirdPerson,
    /// Free global orbit around the planet center, independent of the player.
    Global,
}

impl CameraMode {
    /// Next mode in the switch cycle.
    ///
    /// ```
    /// use game::camera::CameraMode;
    ///
    /// assert_eq!(CameraMode::Follow.cycle(), CameraMode::FirstPerson);
    /// assert_eq!(CameraMode::Global.cycle(), CameraMode::Follow);
    /// ```
    pub fn cycle(self) -> Self {
        match self {
            Self::Follow => Self::FirstPerson,
            Self::FirstPerson => Self::ThirdPerson,
            Self::ThirdPerson => Self::Global,
            Self::Global => Self::Follow,
        }
    }
}

/// All camera state. Distances scale with the planet radius at construction.
#[derive(Clone, Debug)]
pub struct PlayerCamera {
    mode: CameraMode,
    radius: f32,
    global: OrbitCamera,
    follow_yaw: f32,
    follow_pitch: f32,
    follow_distance: f32,
    eye_height: f32,
    chase_distance: f32,
    chase_height: f32,
}

impl PlayerCamera {
    /// Creates cameras for a planet of `radius` meters, starting in
    /// [`CameraMode::Follow`].
    ///
    /// # Panics
    ///
    /// Panics if `radius` is not positive and finite.
    pub fn new(radius: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "camera radius must be positive and finite, got {radius}"
        );
        Self {
            mode: CameraMode::Follow,
            radius,
            global: OrbitCamera::framing_planet(radius),
            // South of a north-facing player, looking north: thrust
            // walks up-screen, east lies right-screen (map-like view).
            // (Negative pitch: the world-space offset sits on the
            // south side; yaw 0 keeps it outside the planet.)
            follow_yaw: 0.0,
            follow_pitch: -0.5,
            follow_distance: radius * 0.8,
            eye_height: radius * 0.01,
            chase_distance: radius * 0.2,
            chase_height: radius * 0.1,
        }
    }

    /// Active mode.
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    /// Switches to `mode` instantly.
    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
    }

    /// Advances to the next mode in the cycle.
    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.cycle();
    }

    /// Free global orbit (drag-rotate, scroll-zoom).
    pub fn global_mut(&mut self) -> &mut OrbitCamera {
        &mut self.global
    }

    /// Current follow-orbit distance in meters.
    pub fn follow_distance(&self) -> f32 {
        self.follow_distance
    }

    /// Drag-rotate: orbits the global camera, or the follow orbit around
    /// the player. First/third person ignore rotation for now (v1 minimum).
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        match self.mode {
            CameraMode::Global => self.global.rotate(dx, dy),
            CameraMode::Follow => {
                self.follow_yaw += dx * 0.01;
                self.follow_pitch = (self.follow_pitch - dy * 0.01).clamp(-MAX_PITCH, MAX_PITCH);
            }
            CameraMode::FirstPerson | CameraMode::ThirdPerson => {}
        }
    }

    /// Scroll-zoom: zooms the global camera, or the follow distance
    /// (clamped to `[2% R, 2 R]`). First/third person ignore zoom for now.
    pub fn zoom(&mut self, delta: f32) {
        match self.mode {
            CameraMode::Global => self.global.zoom(delta),
            CameraMode::Follow => {
                self.follow_distance = (self.follow_distance * (1.0 - 0.1 * delta))
                    .clamp(self.radius * 0.02, self.radius * 2.0);
            }
            CameraMode::FirstPerson | CameraMode::ThirdPerson => {}
        }
    }

    /// Camera position in world space for the given player.
    pub fn eye(&self, player: &Player) -> Vec3 {
        let position = player.position();
        let normal = player.normal();
        match self.mode {
            CameraMode::Global => self.global.eye(),
            CameraMode::Follow => {
                let (sy, cy) = self.follow_yaw.sin_cos();
                let (sp, cp) = self.follow_pitch.sin_cos();
                position + Vec3::new(cp * cy, sp, cp * sy) * self.follow_distance
            }
            CameraMode::FirstPerson => position + normal * self.eye_height,
            CameraMode::ThirdPerson => {
                position - player.facing() * self.chase_distance + normal * self.chase_height
            }
        }
    }

    /// View matrix for the given player (right-handed, same convention as
    /// [`OrbitCamera`]).
    pub fn view_matrix(&self, player: &Player) -> Mat4 {
        let position = player.position();
        let normal = player.normal();
        match self.mode {
            CameraMode::Global => self.global.view_matrix(),
            CameraMode::Follow => look_at(self.eye(player), position, normal),
            CameraMode::FirstPerson => {
                let eye = self.eye(player);
                look_at(eye, eye + player.facing(), normal)
            }
            CameraMode::ThirdPerson => look_at(
                self.eye(player),
                position + normal * self.eye_height,
                normal,
            ),
        }
    }

    /// Perspective projection (Vulkan NDC). All modes share the orbit
    /// camera's field of view in v1.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        self.global.projection_matrix(aspect)
    }
}

/// `look_at` with a fallback up vector when the view direction runs
/// parallel to the preferred up (e.g. follow camera directly overhead).
fn look_at(eye: Vec3, center: Vec3, preferred_up: Vec3) -> Mat4 {
    let forward = center - eye;
    let up = if forward
        .normalize_or_zero()
        .cross(preferred_up)
        .length_squared()
        < 1e-10
    {
        Vec3::Y
    } else {
        preferred_up
    };
    look_at_mat4(eye, center, up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::MoveInput;

    fn setup() -> (PlayerCamera, Player) {
        (PlayerCamera::new(100.0), Player::new(0.0, 0.0, 100.0, 8.0))
    }

    #[test]
    fn follow_eye_sits_at_configured_distance() {
        let (camera, player) = setup();
        let offset = camera.eye(&player) - player.position();
        assert!((offset.length() - camera.follow_distance()).abs() < 1e-4);
    }

    #[test]
    fn follow_view_looks_at_player() {
        let (camera, player) = setup();
        let mapped = camera.view_matrix(&player) * player.position().extend(1.0);
        assert!(mapped.x.abs() < 1e-4 && mapped.y.abs() < 1e-4);
        assert!(mapped.z < 0.0);
    }

    #[test]
    fn first_person_eye_is_above_surface() {
        let (mut camera, player) = setup();
        camera.set_mode(CameraMode::FirstPerson);
        let eye = camera.eye(&player);
        assert!(eye.length() > player.radius());
    }

    #[test]
    fn third_person_eye_is_behind_and_above() {
        let (mut camera, mut player) = setup();
        camera.set_mode(CameraMode::ThirdPerson);
        player.update(MoveInput::new(0.0, 1.0), 0.05);
        let offset = camera.eye(&player) - player.position();
        assert!(offset.dot(player.facing()) < 0.0);
        assert!(offset.dot(player.normal()) > 0.0);
    }

    #[test]
    fn global_mode_ignores_player() {
        let (mut camera, player) = setup();
        camera.set_mode(CameraMode::Global);
        let before = camera.eye(&player);
        let mut moved = player;
        moved.update(MoveInput::new(1.0, 1.0), 1.0);
        assert!((camera.eye(&moved) - before).length() < 1e-6);
    }

    #[test]
    fn cycle_visits_every_mode() {
        let (mut camera, _) = setup();
        let mut seen = vec![camera.mode()];
        for _ in 0..3 {
            camera.cycle_mode();
            seen.push(camera.mode());
        }
        assert_eq!(
            seen,
            vec![
                CameraMode::Follow,
                CameraMode::FirstPerson,
                CameraMode::ThirdPerson,
                CameraMode::Global,
            ]
        );
    }

    #[test]
    fn follow_zoom_clamps_to_configured_range() {
        let (mut camera, _) = setup();
        camera.zoom(1_000.0);
        assert!((camera.follow_distance() - 2.0).abs() < 1e-4);
        camera.zoom(-1_000.0);
        assert!((camera.follow_distance() - 200.0).abs() < 1e-4);
    }
}
