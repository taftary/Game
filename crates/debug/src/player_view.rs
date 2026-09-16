//! Player overlay state for the sphere viewer: a keyboard-driven
//! player, the three player-relative cameras, and tick-based chunk
//! streaming — pure math over the `game` modules, no window, no GPU, so
//! every behavior stays unit-testable and the binary only projects
//! markers and routes input.

use game::camera::{CameraMode, PlayerCamera};
use game::player::{MoveInput, Player};
use game::streaming::{ChunkStreamer, StreamDelta};
use glam::{Mat4, Vec3};

/// Ticks an unseen chunk survives before eviction (same default as the
/// headless `game` demo).
pub const PLAYER_GRACE_TICKS: u64 = 10;
/// Default walk speed: half a radius per second at full tilt (a full
/// equatorial lap takes ~12.6 s — brisk but inspectable).
pub const SPEED_RADIUS_RATIO: f32 = 0.5;

/// Held movement keys. North/south drive thrust (forward/backward
/// along the heading), west/east rotate the heading (left/right);
/// opposing pairs cancel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveKeys {
    pub north: bool,
    pub south: bool,
    pub west: bool,
    pub east: bool,
}

impl MoveKeys {
    /// Fold held keys into a [`MoveInput`]: north/south → thrust
    /// `±1.0`, west/east → turn `∓/±1.0` (each axis `-1.0..=1.0`).
    pub fn input(self) -> MoveInput {
        let axis = |pos: bool, neg: bool| f32::from(pos) - f32::from(neg);
        MoveInput::new(axis(self.north, self.south), axis(self.east, self.west))
    }
}

/// Owned player overlay: position, cameras, streaming, input flags, tick.
/// The viewer owns one; `regenerate()` resets it to the new radius.
#[derive(Clone, Debug)]
pub struct PlayerViewState {
    player: Player,
    camera: PlayerCamera,
    streamer: ChunkStreamer,
    keys: MoveKeys,
    tick: u64,
    /// Whether the sphere viewport renders through the player camera
    /// (`U` toggles; drag/wheel route to it while set).
    pub active: bool,
}

impl PlayerViewState {
    /// Inactive player at lon/lat origin for a planet of `radius`.
    ///
    /// # Panics
    ///
    /// Panics if `radius` is not positive and finite.
    pub fn new(radius: f32) -> Self {
        Self {
            player: Player::new(0.0, 0.0, radius, radius * SPEED_RADIUS_RATIO),
            camera: PlayerCamera::new(radius),
            streamer: ChunkStreamer::new(PLAYER_GRACE_TICKS),
            keys: MoveKeys::default(),
            tick: 0,
            active: false,
        }
    }

    /// Reset position, cameras, streaming and tick to a fresh planet of
    /// `radius`, keeping the `active` flag (regenerating the mesh must
    /// not kick the user out of player mode).
    ///
    /// # Panics
    ///
    /// Panics if `radius` is not positive and finite.
    pub fn reset(&mut self, radius: f32) {
        let active = self.active;
        *self = Self::new(radius);
        self.active = active;
    }

    /// Replace the held-key flags (the binary tracks press/release).
    pub fn set_keys(&mut self, keys: MoveKeys) {
        self.keys = keys;
    }

    /// Currently held keys.
    pub fn keys(&self) -> MoveKeys {
        self.keys
    }

    /// Advance one frame: walk the player by `dt`, then stream `desired`
    /// (the viewer's visible chunk ids) with the grace delay. Returns
    /// what changed so the binary can log it.
    pub fn update(&mut self, dt: f32, desired: &[u32]) -> StreamDelta {
        self.tick += 1;
        self.player.update(self.keys.input(), dt);
        self.streamer.update(desired, self.tick)
    }

    /// Toggle player mode (`U`).
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    /// Cycle Follow → FirstPerson → ThirdPerson → Global (`P`).
    pub fn cycle_camera(&mut self) {
        self.camera.cycle_mode();
    }

    /// Active camera mode.
    pub fn mode(&self) -> CameraMode {
        self.camera.mode()
    }

    /// Drag-rotate the player camera (binary routes viewport drags here
    /// while active; first/third person ignore rotation).
    pub fn rotate_camera(&mut self, dx: f32, dy: f32) {
        self.camera.rotate(dx, dy);
    }

    /// Scroll-zoom the player camera (binary routes wheel here while
    /// active; first/third person ignore zoom).
    pub fn zoom_camera(&mut self, delta: f32) {
        self.camera.zoom(delta);
    }

    /// World-space player position (length == radius).
    pub fn position(&self) -> Vec3 {
        self.player.position()
    }

    /// Direction the player faces (stored heading in the tangent frame,
    /// unit length) — the debug marker arrow source.
    pub fn facing(&self) -> Vec3 {
        self.player.facing()
    }

    /// Player camera position for the current mode.
    pub fn eye(&self) -> Vec3 {
        self.camera.eye(&self.player)
    }

    /// View matrix through the player camera.
    pub fn view_matrix(&self) -> Mat4 {
        self.camera.view_matrix(&self.player)
    }

    /// Shared-FOV projection (same as the orbit camera).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        self.camera.projection_matrix(aspect)
    }

    /// Longitude/latitude in degrees for the panel readout.
    pub fn lon_lat_deg(&self) -> (f32, f32) {
        (
            self.player.longitude().to_degrees(),
            self.player.latitude().to_degrees(),
        )
    }

    /// Compass bearing in degrees for the panel readout.
    pub fn heading_deg(&self) -> f32 {
        self.player.heading().to_degrees()
    }

    /// Currently loaded chunk count.
    pub fn loaded_count(&self) -> usize {
        self.streamer.loaded_count()
    }

    /// Currently loaded chunk ids, sorted ascending.
    pub fn loaded(&self) -> Vec<u32> {
        self.streamer.loaded()
    }

    /// Frame tick (monotonic within a run; drives eviction).
    pub fn tick(&self) -> u64 {
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_state() -> PlayerViewState {
        let mut state = PlayerViewState::new(1.0);
        state.toggle();
        state
    }

    #[test]
    fn keys_fold_to_heading_input() {
        assert_eq!(MoveKeys::default().input(), MoveInput::idle());
        let keys = MoveKeys {
            north: true,
            east: true,
            ..MoveKeys::default()
        };
        // Thrust forward + turn right.
        assert_eq!(keys.input(), MoveInput::new(1.0, 1.0));
        // Opposing pairs cancel.
        let stuck = MoveKeys {
            north: true,
            south: true,
            west: true,
            east: true,
        };
        assert_eq!(stuck.input(), MoveInput::idle());
    }

    #[test]
    fn turn_key_rotates_without_walking() {
        let mut state = active_state();
        let before = state.position();
        state.set_keys(MoveKeys {
            east: true,
            ..MoveKeys::default()
        });
        let delta = state.update(0.05, &[1, 2, 3]);
        assert_eq!(delta.loaded, vec![1, 2, 3]);
        assert!((state.position() - before).length() < 1e-6);
        assert!(
            state.heading_deg() > 0.0,
            "east key must turn right, got {}",
            state.heading_deg()
        );
        assert_eq!(state.tick(), 1);
        assert_eq!(state.loaded_count(), 3);
    }

    #[test]
    fn thrust_key_walks_along_heading() {
        let mut state = active_state();
        state.set_keys(MoveKeys {
            north: true,
            ..MoveKeys::default()
        });
        state.update(0.05, &[1, 2, 3]);
        let (lon, lat) = state.lon_lat_deg();
        assert!(lat > 0.0, "north thrust must raise latitude, got {lat}");
        assert!(lon.abs() < 1e-4, "{lon}");
    }

    #[test]
    fn trail_evicts_after_grace() {
        let mut state = active_state();
        state.update(0.05, &[1, 2]);
        let mut last = StreamDelta::default();
        for _ in 0..=PLAYER_GRACE_TICKS {
            last = state.update(0.05, &[2]);
        }
        assert_eq!(last.unloaded, vec![1]);
        assert!(!state.loaded().contains(&1));
    }

    #[test]
    fn toggle_and_cycle() {
        let mut state = PlayerViewState::new(1.0);
        assert!(!state.active);
        state.toggle();
        assert!(state.active);
        assert_eq!(state.mode(), CameraMode::Follow);
        state.cycle_camera();
        assert_eq!(state.mode(), CameraMode::FirstPerson);
        state.cycle_camera();
        assert_eq!(state.mode(), CameraMode::ThirdPerson);
        state.toggle();
        assert!(!state.active);
    }

    #[test]
    fn reset_keeps_active_and_clears() {
        let mut state = active_state();
        state.set_keys(MoveKeys {
            north: true,
            ..MoveKeys::default()
        });
        state.update(0.05, &[7]);
        assert!(state.tick() > 0 && state.loaded_count() > 0);
        state.reset(2.0);
        assert!(state.active, "regen must not kick out of player mode");
        assert_eq!(state.tick(), 0);
        assert_eq!(state.loaded_count(), 0);
        assert_eq!(state.keys(), MoveKeys::default());
        let (lon, lat) = state.lon_lat_deg();
        assert_eq!((lon, lat), (0.0, 0.0));
        assert!((state.position().length() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn eye_tracks_mode() {
        let mut state = active_state();
        let follow_eye = state.eye();
        assert!((follow_eye - state.position()).length() > 0.0);
        state.cycle_camera();
        let fp_eye = state.eye();
        assert!(fp_eye.length() > state.position().length() - 1e-6);
    }

    #[test]
    fn updates_are_deterministic() {
        let run = || {
            let mut state = active_state();
            state.set_keys(MoveKeys {
                north: true,
                east: true,
                ..MoveKeys::default()
            });
            let mut log = Vec::new();
            for tick in 0..8 {
                let desired: Vec<u32> = (tick..tick + 3).collect();
                log.push(state.update(0.05, &desired));
            }
            log
        };
        assert_eq!(run(), run());
    }
}
