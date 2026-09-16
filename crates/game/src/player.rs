//! Player position and movement on the sphere surface.
//!
//! The player is a headed walker on a perfect sphere — no heightfield
//! or collision yet (M3). Movement input is expressed relative to the
//! stored heading: thrust walks forward/backward along it, turn rotates
//! it, so one [`MoveInput`] mapping drives keyboard, stick, or scripted
//! demos identically. Holding thrust without turning traces a great
//! circle: the heading is parallel-transported into the new tangent
//! frame after every step.

use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Latitude clamp margin at the poles (radians): the north/east tangent
/// frame degenerates exactly at the poles, so the player never reaches them.
pub const POLE_MARGIN: f32 = 1e-3;

/// Maximum player latitude in radians.
pub const MAX_LATITUDE: f32 = FRAC_PI_2 - POLE_MARGIN;

/// Heading turn rate at full deflection, radians per second.
pub const TURN_RATE: f32 = PI;

/// Movement intent relative to the player's heading, each axis
/// `-1.0..=1.0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveInput {
    /// Thrust axis: forward-positive along the heading.
    pub forward: f32,
    /// Turn axis: right-positive rotation of the heading.
    pub turn: f32,
}

impl MoveInput {
    /// Builds an input, clamping each axis into `-1.0..=1.0`
    /// (analog-stick friendly).
    ///
    /// ```
    /// use game::player::MoveInput;
    ///
    /// assert_eq!(MoveInput::new(0.0, 2.0).turn, 1.0);
    /// ```
    pub fn new(forward: f32, turn: f32) -> Self {
        Self {
            forward: forward.clamp(-1.0, 1.0),
            turn: turn.clamp(-1.0, 1.0),
        }
    }

    /// No movement.
    pub fn idle() -> Self {
        Self {
            forward: 0.0,
            turn: 0.0,
        }
    }
}

/// Player state: a lon/lat anchor on the sphere with a stored heading
/// (compass bearing) plus surface velocity.
///
/// ```
/// use game::player::{MoveInput, Player};
///
/// let mut player = Player::new(0.0, 0.0, 100.0, 8.0);
/// player.update(MoveInput::new(1.0, 0.0), 0.05);
/// assert!(player.latitude() > 0.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    longitude: f32,
    latitude: f32,
    radius: f32,
    speed: f32,
    heading: f32,
    velocity: Vec3,
}

impl Player {
    /// Creates a player. `longitude` wraps into `[0, TAU)`; `latitude`
    /// clamps to `[-MAX_LATITUDE, MAX_LATITUDE]`. The heading starts at
    /// 0 (facing north).
    ///
    /// # Panics
    ///
    /// Panics if `radius` is not positive and finite, or if `speed` is
    /// negative or not finite.
    pub fn new(longitude: f32, latitude: f32, radius: f32, speed: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "player radius must be positive and finite, got {radius}"
        );
        assert!(
            speed.is_finite() && speed >= 0.0,
            "player speed must be non-negative and finite, got {speed}"
        );
        Self {
            longitude: longitude.rem_euclid(TAU),
            latitude: latitude.clamp(-MAX_LATITUDE, MAX_LATITUDE),
            radius,
            speed,
            heading: 0.0,
            velocity: Vec3::ZERO,
        }
    }

    /// Longitude in radians, `[0, TAU)`.
    pub fn longitude(&self) -> f32 {
        self.longitude
    }

    /// Latitude in radians, `[-MAX_LATITUDE, MAX_LATITUDE]`.
    pub fn latitude(&self) -> f32 {
        self.latitude
    }

    /// Sphere radius in meters.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Full-deflection speed in meters per second.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Compass bearing in radians, `[0, TAU)`: 0 faces north, positive
    /// rotates toward east. Persists while idle; the only direction
    /// state (see [`Player::facing`]).
    pub fn heading(&self) -> f32 {
        self.heading
    }

    /// World-space velocity in meters per second, tangent to the surface.
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// World-space position on the sphere (`length == radius`). The
    /// longitude sign makes `(north, east, up)` a proper ENU frame
    /// (`east × north == up`), so positive headings are true compass
    /// bearings (clockwise from north) and turn input steers toward
    /// the avatar's own right.
    pub fn position(&self) -> Vec3 {
        let (lon_sin, lon_cos) = self.longitude.sin_cos();
        let (lat_sin, lat_cos) = self.latitude.sin_cos();
        Vec3::new(
            self.radius * lat_cos * lon_cos,
            self.radius * lat_sin,
            -self.radius * lat_cos * lon_sin,
        )
    }

    /// Outward surface normal at the player position (unit length).
    pub fn normal(&self) -> Vec3 {
        self.position() / self.radius
    }

    /// Tangent forward direction (north, unit length).
    pub fn north(&self) -> Vec3 {
        let (lon_sin, lon_cos) = self.longitude.sin_cos();
        let (lat_sin, lat_cos) = self.latitude.sin_cos();
        Vec3::new(-lat_sin * lon_cos, lat_cos, lat_sin * lon_sin)
    }

    /// Tangent right direction (east, unit length).
    pub fn east(&self) -> Vec3 {
        let (lon_sin, lon_cos) = self.longitude.sin_cos();
        Vec3::new(-lon_sin, 0.0, -lon_cos)
    }

    /// Direction the player faces: the stored heading expressed in the
    /// local north/east frame. Persists while idle. Always tangent to
    /// the surface (unit length).
    pub fn facing(&self) -> Vec3 {
        self.north() * self.heading.cos() + self.east() * self.heading.sin()
    }

    /// Advances the player by `dt` seconds. Turn rotates the heading
    /// first; thrust then steps along it, and the heading is
    /// parallel-transported into the new tangent frame so a straight
    /// walk traces a great circle. The step is capped well below the
    /// radius so the sphere reprojection stays exact. Non-positive `dt`
    /// stops the player without moving or turning.
    pub fn update(&mut self, input: MoveInput, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            self.velocity = Vec3::ZERO;
            return;
        }
        self.heading = (self.heading + input.turn * TURN_RATE * dt).rem_euclid(TAU);
        let facing = self.facing();
        let mut step = facing * input.forward * self.speed * dt;
        let max_step = self.radius * 0.25;
        if step.length_squared() > max_step * max_step {
            step = step.normalize() * max_step;
        }
        let origin = self.position();
        self.velocity = step / dt;
        if step == Vec3::ZERO {
            // Pure turn: the heading above is the whole update.
            return;
        }
        let dir = (origin + step).normalize_or_zero();
        if dir == Vec3::ZERO {
            self.velocity = Vec3::ZERO;
            return;
        }
        self.latitude = dir
            .y
            .clamp(-1.0, 1.0)
            .asin()
            .clamp(-MAX_LATITUDE, MAX_LATITUDE);
        self.longitude = (-dir.z).atan2(dir.x).rem_euclid(TAU);
        // Carry the pre-move facing into the new tangent frame: project
        // out the new normal and re-express the heading, so thrust
        // without turn keeps walking "straight ahead" on the sphere.
        let carried = facing - dir * facing.dot(dir);
        if carried.length_squared() > 1e-12 {
            let carried = carried.normalize();
            self.heading = carried
                .dot(self.east())
                .atan2(carried.dot(self.north()))
                .rem_euclid(TAU);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Player {
        Player::new(0.0, 0.0, 100.0, 8.0)
    }

    #[test]
    fn position_sits_on_sphere() {
        let player = Player::new(1.2, 0.4, 100.0, 8.0);
        assert!((player.position().length() - 100.0).abs() < 1e-4);
        assert!((player.normal().length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn thrust_north_increases_latitude() {
        let mut player = player();
        // Fresh players face north (heading 0).
        assert_eq!(player.heading(), 0.0);
        player.update(MoveInput::new(1.0, 0.0), 0.05);
        assert!(player.latitude() > 0.0);
        assert!(player.longitude().abs() < 1e-6);
        assert!((player.position().length() - 100.0).abs() < 1e-4);
    }

    #[test]
    fn turning_east_then_walking_raises_longitude() {
        let mut player = player();
        // Full right turn for half a second faces east (heading PI/2).
        player.update(MoveInput::new(0.0, 1.0), 0.5);
        assert!((player.heading() - FRAC_PI_2).abs() < 1e-4);
        player.update(MoveInput::new(1.0, 0.0), 0.05);
        assert!(player.longitude() > 0.0);
        assert!(player.latitude().abs() < 1e-6);
        assert!((player.position().length() - 100.0).abs() < 1e-4);
    }

    #[test]
    fn tangent_frame_is_east_north_up() {
        let player = player();
        // Proper ENU frame (right-handed like Earth): east × north == up.
        let frame = player.east().cross(player.north()) - player.normal();
        assert!(frame.length() < 1e-6);
    }

    #[test]
    fn turn_right_rotates_toward_own_right() {
        let mut player = player();
        let f0 = player.facing();
        // Own right side: forward × up.
        let right0 = f0.cross(player.normal());
        player.update(MoveInput::new(0.0, 1.0), 0.05);
        let f1 = player.facing();
        assert!(
            (f1 - f0).dot(right0) > 0.0,
            "D must turn toward the avatar's own right"
        );
    }

    #[test]
    fn turning_in_place_does_not_move() {
        let mut player = player();
        let before = player.position();
        player.update(MoveInput::new(0.0, 1.0), 0.25);
        assert!((player.position() - before).length() < 1e-6);
        assert_eq!(player.velocity(), Vec3::ZERO);
        assert!((player.heading() - PI / 4.0).abs() < 1e-5);
    }

    #[test]
    fn turn_plus_thrust_never_exceeds_full_speed() {
        let mut player = player();
        player.update(MoveInput::new(1.0, 1.0), 1.0);
        assert!(player.velocity().length() <= 8.0 + 1e-4);
    }

    #[test]
    fn idle_player_keeps_heading() {
        let mut player = player();
        player.update(MoveInput::new(0.0, 1.0), 0.5);
        let east = player.east();
        player.update(MoveInput::idle(), 0.05);
        assert_eq!(player.velocity(), Vec3::ZERO);
        assert!((player.facing() - east).length() < 1e-6);
    }

    #[test]
    fn straight_walk_traces_a_great_circle() {
        let mut player = player();
        // Face a non-cardinal bearing, then walk straight.
        player.update(MoveInput::new(0.0, 1.0), 0.6 / PI);
        let start_heading = player.heading();
        assert!((start_heading - 0.6).abs() < 1e-4);
        let plane = player
            .position()
            .normalize()
            .cross(player.facing())
            .normalize();
        for _ in 0..200 {
            player.update(MoveInput::new(1.0, 0.0), 0.05);
            // Still on the initial great-circle plane...
            assert!(
                player.position().dot(plane).abs() < 1e-2,
                "left the great circle: {}",
                player.position().dot(plane)
            );
            // ...and still on the sphere.
            assert!((player.position().length() - 100.0).abs() < 1e-3);
        }
        // Parallel transport drifted the compass bearing (not a rhumb line).
        assert!((player.heading() - start_heading).abs() > 1e-3);
    }

    #[test]
    fn non_positive_dt_stops_without_moving_or_turning() {
        let mut player = player();
        player.update(MoveInput::new(1.0, 1.0), 0.0);
        assert_eq!(player.longitude(), 0.0);
        assert_eq!(player.heading(), 0.0);
        assert_eq!(player.velocity(), Vec3::ZERO);
    }

    #[test]
    fn latitude_clamps_before_poles() {
        let mut player = Player::new(0.0, MAX_LATITUDE, 100.0, 8.0);
        player.update(MoveInput::new(1.0, 0.0), 10.0);
        assert!(player.latitude() <= MAX_LATITUDE);
        assert!(player.position().is_finite());
    }

    #[test]
    fn longitude_wraps_around() {
        let mut player = Player::new(TAU - 0.001, 0.0, 100.0, 8.0);
        player.update(MoveInput::new(0.0, 1.0), 0.5);
        player.update(MoveInput::new(1.0, 0.0), 1.0);
        assert!(player.longitude() < TAU);
        assert!(player.longitude() >= 0.0);
    }
}
