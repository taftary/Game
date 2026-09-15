//! Player position and movement on the sphere surface.
//!
//! The player is a longitude/latitude anchor on a perfect sphere — no
//! heightfield or collision yet (M3). Movement input is expressed in the
//! local tangent plane (north/east), so one [`MoveInput`] mapping drives
//! keyboard, stick, or scripted demos identically.

use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

/// Latitude clamp margin at the poles (radians): the north/east tangent
/// frame degenerates exactly at the poles, so the player never reaches them.
pub const POLE_MARGIN: f32 = 1e-3;

/// Maximum player latitude in radians.
pub const MAX_LATITUDE: f32 = FRAC_PI_2 - POLE_MARGIN;

/// Movement intent in the player's tangent plane, each axis `-1.0..=1.0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveInput {
    /// North-positive axis.
    pub forward: f32,
    /// East-positive axis.
    pub right: f32,
}

impl MoveInput {
    /// Builds an input, clamping each axis into `-1.0..=1.0`
    /// (analog-stick friendly).
    ///
    /// ```
    /// use game::player::MoveInput;
    ///
    /// assert_eq!(MoveInput::new(0.0, 2.0).right, 1.0);
    /// ```
    pub fn new(forward: f32, right: f32) -> Self {
        Self {
            forward: forward.clamp(-1.0, 1.0),
            right: right.clamp(-1.0, 1.0),
        }
    }

    /// No movement.
    pub fn idle() -> Self {
        Self {
            forward: 0.0,
            right: 0.0,
        }
    }
}

/// Player state: a lon/lat anchor on the sphere plus surface velocity.
///
/// ```
/// use game::player::{MoveInput, Player};
///
/// let mut player = Player::new(0.0, 0.0, 100.0, 8.0);
/// player.update(MoveInput::new(0.0, 1.0), 0.05);
/// assert!(player.longitude() > 0.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    longitude: f32,
    latitude: f32,
    radius: f32,
    speed: f32,
    velocity: Vec3,
}

impl Player {
    /// Creates a player. `longitude` wraps into `[0, TAU)`; `latitude`
    /// clamps to `[-MAX_LATITUDE, MAX_LATITUDE]`.
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

    /// World-space velocity in meters per second, tangent to the surface.
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// World-space position on the sphere (`length == radius`).
    pub fn position(&self) -> Vec3 {
        let (lon_sin, lon_cos) = self.longitude.sin_cos();
        let (lat_sin, lat_cos) = self.latitude.sin_cos();
        Vec3::new(
            self.radius * lat_cos * lon_cos,
            self.radius * lat_sin,
            self.radius * lat_cos * lon_sin,
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
        Vec3::new(-lat_sin * lon_cos, lat_cos, -lat_sin * lon_sin)
    }

    /// Tangent right direction (east, unit length).
    pub fn east(&self) -> Vec3 {
        let (lon_sin, lon_cos) = self.longitude.sin_cos();
        Vec3::new(-lon_sin, 0.0, lon_cos)
    }

    /// Direction the player faces: velocity direction while moving,
    /// north when idle. Always tangent to the surface (unit length).
    pub fn facing(&self) -> Vec3 {
        if self.velocity.length_squared() > 1e-12 {
            self.velocity.normalize()
        } else {
            self.north()
        }
    }

    /// Advances the player by `dt` seconds. Diagonal input is normalized
    /// so it never exceeds full speed; the step is capped well below the
    /// radius so the sphere reprojection stays exact. Non-positive `dt`
    /// stops the player without moving.
    pub fn update(&mut self, input: MoveInput, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            self.velocity = Vec3::ZERO;
            return;
        }
        let mut wish = self.north() * input.forward + self.east() * input.right;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }
        let mut step = wish * self.speed * dt;
        let max_step = self.radius * 0.25;
        if step.length_squared() > max_step * max_step {
            step = step.normalize() * max_step;
        }
        let origin = self.position();
        self.velocity = step / dt;
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
        self.longitude = dir.z.atan2(dir.x).rem_euclid(TAU);
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
    fn walking_east_increases_longitude() {
        let mut player = player();
        player.update(MoveInput::new(0.0, 1.0), 0.05);
        assert!(player.longitude() > 0.0);
        assert!(player.latitude().abs() < 1e-6);
        assert!((player.position().length() - 100.0).abs() < 1e-4);
    }

    #[test]
    fn walking_north_increases_latitude() {
        let mut player = player();
        player.update(MoveInput::new(1.0, 0.0), 0.05);
        assert!(player.latitude() > 0.0);
        assert!((player.position().length() - 100.0).abs() < 1e-4);
    }

    #[test]
    fn diagonal_input_never_exceeds_full_speed() {
        let mut player = player();
        player.update(MoveInput::new(1.0, 1.0), 1.0);
        assert!(player.velocity().length() <= 8.0 + 1e-4);
    }

    #[test]
    fn idle_player_has_zero_velocity_and_faces_north() {
        let mut player = player();
        player.update(MoveInput::idle(), 0.05);
        assert_eq!(player.velocity(), Vec3::ZERO);
        assert!((player.facing() - player.north()).length() < 1e-6);
    }

    #[test]
    fn non_positive_dt_stops_without_moving() {
        let mut player = player();
        player.update(MoveInput::new(0.0, 1.0), 0.0);
        assert_eq!(player.longitude(), 0.0);
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
        player.update(MoveInput::new(0.0, 1.0), 1.0);
        assert!(player.longitude() < TAU);
        assert!(player.longitude() >= 0.0);
    }
}
