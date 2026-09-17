//! `engine::time` — explicit time-compression state machine (spec §2,
//! ADR-015).
//!
//! Physics runs true real-time near bodies and compresses smoothly far
//! from them — as a state machine tied to frame occupancy, NEVER a
//! global multiplier (multipliers desynchronize SOI handoffs). Ratio is
//! display/persist data only; the sole advance path substeps fixed-size
//! Verlet physics.
//!
//! Rules:
//!
//! - Inside any blend band ⇒ ratio 1 (real-time THROUGH handoffs).
//! - Planetocentric / LocalEnu ⇒ ratio 1 (precision piloting).
//! - Elsewhere ratio ramps continuously from occupancy, slewed with a
//!   2 s time constant so physics dt never jumps — including across
//!   frame commits.
//!
//! ```
//! use game_engine::frames::FrameId;
//! use game_engine::time::{target_ratio, CompressionMode, Occupancy};
//!
//! // Deep frame: real-time regardless of fraction.
//! let deep = Occupancy { frame: FrameId::LocalEnu(game_engine::frames::BodyId::EARTH), in_blend_band: false, depth_fraction: 100.0 };
//! assert_eq!(target_ratio(deep), 1.0);
//! // Far solar-system cruise compresses.
//! let cruise = Occupancy { frame: FrameId::SolarSystem, in_blend_band: false, depth_fraction: 10.0 };
//! assert!(target_ratio(cruise) > 1.0);
//! assert_eq!(CompressionMode::of(1.0), CompressionMode::RealTime);
//! ```

use crate::frames::FrameId;
use crate::physics::{IntegratorState, verlet_step};
use glam::DVec3;

/// Slew time constant (real seconds): ratio chases target
/// exponentially, so physics dt has bounded derivative everywhere.
pub const SLEW_TAU_S: f64 = 2.0;
/// Ramp exponent: ratio = fraction³ above the significant radius.
pub const RAMP_EXPONENT: f64 = 3.0;

/// Frame occupancy input: what the compression rule decides from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occupancy {
    /// Active frame.
    pub frame: FrameId,
    /// True while any SOI blend weight is strictly inside (0, 1).
    /// Forces real-time (anti-desync rule).
    pub in_blend_band: bool,
    /// Distance-to-primary / significant radius (10⁻³ rule). ≤ 1 means
    /// inside gravitational significance. Callers that cannot compute
    /// it pass 1.0 (conservative: real-time).
    pub depth_fraction: f64,
}

/// Compression ceiling per frame (0 = never compress).
fn ceiling(frame: FrameId) -> f64 {
    match frame {
        FrameId::LocalEnu(_) | FrameId::Planetocentric(_) => 1.0,
        FrameId::SolarSystem => 1.0e4,
        FrameId::StellarNeighborhood => 1.0e6,
        FrameId::Galactocentric | FrameId::LocalGroup => 1.0e8,
        FrameId::Cosmological => 1.0e9,
    }
}

/// Target compression ratio for `occ`. Continuous in `depth_fraction`
/// (1.0 at fraction ≤ 1, ramping after); frame/blend inputs arrive via
/// explicit commits, never mid-tick surprises.
pub fn target_ratio(occ: Occupancy) -> f64 {
    if occ.in_blend_band {
        return 1.0;
    }
    let ceil = ceiling(occ.frame);
    if ceil <= 1.0 {
        return 1.0;
    }
    (occ.depth_fraction.max(1.0).powf(RAMP_EXPONENT)).min(ceil)
}

/// Named compression mode (HUD + persist semantics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionMode {
    /// Ratio exactly 1: true real-time physics.
    RealTime,
    /// Compressed: ratio > 1 advances sim time faster.
    Compressed,
}

impl CompressionMode {
    /// Mode for a ratio: exactly 1.0 ⇒ real-time, else compressed.
    pub fn of(ratio: f64) -> Self {
        if ratio <= 1.0 {
            CompressionMode::RealTime
        } else {
            CompressionMode::Compressed
        }
    }
}

/// Slewed compression clock: chases the occupancy target without jumps
/// and advances sim time through fixed-size physics substeps.
#[derive(Clone, Debug)]
pub struct CompressionClock {
    ratio: f64,
    sim_time_s: f64,
}

impl CompressionClock {
    /// Start at real-time, sim epoch zero.
    pub fn new() -> Self {
        Self {
            ratio: 1.0,
            sim_time_s: 0.0,
        }
    }

    /// Current ratio (display/persist only — never a physics input).
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Current mode.
    pub fn mode(&self) -> CompressionMode {
        CompressionMode::of(self.ratio)
    }

    /// Accumulated simulation time (s).
    pub fn sim_time_s(&self) -> f64 {
        self.sim_time_s
    }

    /// Slew toward `target` over `dt_real_s` (exponential approach,
    /// no overshoot) and return the new ratio.
    pub fn slew(&mut self, target: f64, dt_real_s: f64) -> f64 {
        debug_assert!(target >= 1.0 && target.is_finite());
        debug_assert!(dt_real_s >= 0.0 && dt_real_s.is_finite());
        let k = (dt_real_s / SLEW_TAU_S).min(1.0);
        self.ratio += (target - self.ratio) * k;
        // Snap when within 1 ulp-ish of target to avoid eternal chase.
        if (self.ratio - target).abs() < 1e-12 * target.max(1.0) {
            self.ratio = target;
        }
        self.ratio
    }

    /// Advance `state` by `dt_real_s` of wall time at the current ratio:
    /// `ceil(ratio·dt / max_physics_dt)` fixed-size Verlet substeps, sim
    /// time += ratio·dt exactly. Physics dt per substep ≤ max_dt ALWAYS.
    pub fn advance(
        &mut self,
        state: &mut IntegratorState,
        dt_real_s: f64,
        accel: &impl Fn(DVec3) -> DVec3,
        max_physics_dt: f64,
    ) -> u32 {
        debug_assert!(max_physics_dt > 0.0);
        let sim_dt = self.ratio * dt_real_s;
        let n = ((sim_dt / max_physics_dt).ceil() as u32).max(1);
        let h = sim_dt / n as f64;
        for _ in 0..n {
            verlet_step(state, h, accel);
        }
        self.sim_time_s += sim_dt;
        n
    }
}

impl Default for CompressionClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Frozen save payload for `autosave-persistence` (ADR-004): mode +
/// ratio + sim time in a fixed 24-byte layout (mode u8, 7 pad bytes,
/// ratio f64 LE, sim time f64 LE). Bit-exact round trip; unknown mode
/// discriminants and short/long buffers are rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressionSnapshot {
    /// Active mode.
    pub mode: CompressionMode,
    /// Active ratio.
    pub ratio: f64,
    /// Accumulated sim time (s).
    pub sim_time_s: f64,
}

impl CompressionSnapshot {
    /// Capture from a live clock.
    pub fn capture(clock: &CompressionClock) -> Self {
        Self {
            mode: clock.mode(),
            ratio: clock.ratio,
            sim_time_s: clock.sim_time_s,
        }
    }

    /// Fixed-layout encode (24 bytes).
    pub fn to_bytes(self) -> [u8; 24] {
        let mut out = [0u8; 24];
        out[0] = match self.mode {
            CompressionMode::RealTime => 0,
            CompressionMode::Compressed => 1,
        };
        out[8..16].copy_from_slice(&self.ratio.to_le_bytes());
        out[16..24].copy_from_slice(&self.sim_time_s.to_le_bytes());
        out
    }

    /// Decode, rejecting malformed input (never boot-loop on corrupt
    /// saves — persistence contract).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() != 24 {
            return Err(SnapshotError::BadLength(bytes.len()));
        }
        let mode = match bytes[0] {
            0 => CompressionMode::RealTime,
            1 => CompressionMode::Compressed,
            other => return Err(SnapshotError::BadMode(other)),
        };
        let mut ratio_b = [0u8; 8];
        let mut time_b = [0u8; 8];
        ratio_b.copy_from_slice(&bytes[8..16]);
        time_b.copy_from_slice(&bytes[16..24]);
        let ratio = f64::from_le_bytes(ratio_b);
        let sim_time_s = f64::from_le_bytes(time_b);
        if !ratio.is_finite() || ratio < 1.0 || !sim_time_s.is_finite() {
            return Err(SnapshotError::BadValue);
        }
        Ok(Self {
            mode,
            ratio,
            sim_time_s,
        })
    }
}

/// Snapshot decode failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    /// Buffer is not exactly 24 bytes.
    BadLength(usize),
    /// Unknown mode discriminant.
    BadMode(u8),
    /// Non-finite ratio, ratio < 1, or non-finite sim time.
    BadValue,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::BadLength(n) => write!(f, "snapshot must be 24 bytes, got {n}"),
            SnapshotError::BadMode(m) => write!(f, "unknown compression mode {m}"),
            SnapshotError::BadValue => write!(f, "non-finite ratio/time in snapshot"),
        }
    }
}

impl std::error::Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::BodyId;
    use crate::physics::orbital_energy;

    const EARTH: BodyId = BodyId::EARTH;

    fn occ(frame: FrameId, band: bool, frac: f64) -> Occupancy {
        Occupancy {
            frame,
            in_blend_band: band,
            depth_fraction: frac,
        }
    }

    #[test]
    fn occupancy_table_drives_mode() {
        // Deep frames: real-time at any fraction.
        assert_eq!(
            target_ratio(occ(FrameId::LocalEnu(EARTH), false, 100.0)),
            1.0
        );
        assert_eq!(
            target_ratio(occ(FrameId::Planetocentric(EARTH), false, 50.0)),
            1.0
        );
        // Inside significance: real-time everywhere.
        assert_eq!(target_ratio(occ(FrameId::SolarSystem, false, 1.0)), 1.0);
        assert_eq!(target_ratio(occ(FrameId::SolarSystem, false, 0.2)), 1.0);
        // Outside: ramps continuously, monotone, capped.
        let r2 = target_ratio(occ(FrameId::SolarSystem, false, 2.0));
        let r3 = target_ratio(occ(FrameId::SolarSystem, false, 3.0));
        assert_eq!(r2, 8.0); // 2³
        assert_eq!(r3, 27.0); // 3³
        assert!(r3 > r2);
        assert_eq!(target_ratio(occ(FrameId::SolarSystem, false, 1.0e9)), 1.0e4);
        assert_eq!(
            target_ratio(occ(FrameId::Cosmological, false, 1.0e9)),
            1.0e9
        );
        // Continuity at the ramp foot: 1±ε stays within units of 1.
        let eps = 1e-6;
        assert!((target_ratio(occ(FrameId::SolarSystem, false, 1.0 + eps)) - 1.0).abs() < 1e-4);
        // Modes follow.
        assert_eq!(CompressionMode::of(1.0), CompressionMode::RealTime);
        assert_eq!(CompressionMode::of(8.0), CompressionMode::Compressed);
    }

    #[test]
    fn blend_band_forces_realtime_everywhere() {
        // Desync guard (NFR/ADR-014): even mid-cruise occupancy with a
        // live blend weight compresses nothing.
        for frame in [
            FrameId::SolarSystem,
            FrameId::StellarNeighborhood,
            FrameId::Cosmological,
        ] {
            assert_eq!(target_ratio(occ(frame, true, 1.0e9)), 1.0);
        }
    }

    #[test]
    fn slew_converges_monotonically_without_overshoot() {
        let mut clock = CompressionClock::new();
        let mut prev = clock.ratio();
        for _ in 0..10_000 {
            let r = clock.slew(1000.0, 0.05);
            assert!(r >= prev && r <= 1000.0, "monotone chase, no overshoot");
            prev = r;
        }
        assert_eq!(clock.ratio(), 1000.0);
        // And back down.
        for _ in 0..10_000 {
            clock.slew(1.0, 0.05);
        }
        assert_eq!(clock.ratio(), 1.0);
        assert_eq!(clock.mode(), CompressionMode::RealTime);
    }

    #[test]
    fn advance_equals_manual_substepping() {
        // No-multiplier proof: advance() ≡ ceil-chopped fixed Verlet
        // steps with sim_time += ratio·dt exactly. A shortcut global
        // scaling could never match substep dynamics bit-for-bit.
        let accel = |p: DVec3| -p / p.length().powi(3);
        let mut a = IntegratorState {
            pos: DVec3::X,
            vel: DVec3::Y,
        };
        let mut b = a;
        let mut clock = CompressionClock::new();
        clock.slew(64.0, 100.0); // k = 1: snaps straight to 64
        let n = clock.advance(&mut a, 0.1, &accel, 0.05);
        assert_eq!(n, 128); // ceil(64·0.1/0.05)
        let h = 64.0 * 0.1 / 128.0;
        for _ in 0..128 {
            verlet_step(&mut b, h, &accel);
        }
        assert_eq!(a.pos, b.pos);
        assert_eq!(a.vel, b.vel);
        assert_eq!(clock.sim_time_s(), 6.4);
        // Physics dt per substep respected the cap.
        assert!(h <= 0.05);
    }

    #[test]
    fn sim_time_accumulation_is_deterministic() {
        // Pinned sequence: same occupancy history ⇒ same sim time.
        let run = || {
            let mut clock = CompressionClock::new();
            let mut t = 0.0;
            for (target, dt) in [(1.0, 1.0), (8.0, 1.0), (1000.0, 2.0), (1.0, 1.0)] {
                clock.slew(target, dt);
                // sim time advances at the CURRENT (slewed) ratio.
                let sim_dt = clock.ratio() * dt;
                t += sim_dt;
                // Mirror clock bookkeeping without stepping a body.
                clock.sim_time_s += sim_dt;
            }
            (clock.ratio(), t, clock.sim_time_s())
        };
        let (r1, t1, s1) = run();
        let (r2, t2, s2) = run();
        assert_eq!((r1, t1, s1), (r2, t2, s2));
        assert!(s1 > 0.0);
    }

    #[test]
    fn realtime_orbit_matches_analytic_period() {
        // PO tolerance table: period within 0.1% in real-time mode.
        // Circular orbit μ = r = 1: analytic T = 2π. Detect the first
        // y: −→+ x-axis crossing after t > 0 via Verlet at ratio 1.
        let accel = |p: DVec3| -p / p.length().powi(3);
        let mut s = IntegratorState {
            pos: DVec3::X,
            vel: DVec3::Y,
        };
        let mut clock = CompressionClock::new();
        assert_eq!(clock.mode(), CompressionMode::RealTime);
        let dt = 2.0 * std::f64::consts::PI / 2000.0;
        let mut t = 0.0;
        let mut prev_y = 0.0;
        let mut crossings = 0;
        let mut period = 0.0;
        for _ in 0..10_000 {
            clock.advance(&mut s, dt, &accel, dt); // ratio 1: 1 substep
            t += dt;
            if prev_y < 0.0 && s.pos.y >= 0.0 && t > 1.0 {
                crossings += 1;
                if crossings == 1 {
                    period = t;
                    break;
                }
            }
            prev_y = s.pos.y;
        }
        let expected = 2.0 * std::f64::consts::PI;
        assert!((period - expected).abs() / expected < 1e-3, "T = {period}");
    }

    #[test]
    fn slew_transition_keeps_energy_bounded() {
        // NFR (ADR-018): slewing 1 → 100 with substeps keeps the orbit
        // bound — adiabatic contract, documented limit (not exact
        // symplecticity, which variable-dt cannot provide).
        let accel = |p: DVec3| -p / p.length().powi(3);
        let mut s = IntegratorState {
            pos: DVec3::X,
            vel: DVec3::Y,
        };
        let mut clock = CompressionClock::new();
        let e0 = orbital_energy(s.pos, s.vel, 1.0);
        let mut max_drift: f64 = 0.0;
        for _ in 0..200 {
            clock.slew(100.0, 0.05);
            clock.advance(&mut s, 0.05, &accel, 0.05);
            max_drift = max_drift.max((orbital_energy(s.pos, s.vel, 1.0) - e0).abs() / e0.abs());
        }
        assert!(max_drift < 1e-2, "slew drift {max_drift}");
        assert!(s.pos.is_finite() && s.vel.is_finite());
    }

    #[test]
    fn snapshot_round_trips_and_rejects_corrupt() {
        let mut clock = CompressionClock::new();
        clock.slew(8.0, 100.0);
        clock.sim_time_s = 1234.5;
        let snap = CompressionSnapshot::capture(&clock);
        assert_eq!(snap.mode, CompressionMode::Compressed);
        let bytes = snap.to_bytes();
        assert_eq!(bytes.len(), 24);
        assert_eq!(CompressionSnapshot::from_bytes(&bytes), Ok(snap));
        // Corrupt inputs rejected, never boot-loop (persistence contract).
        assert!(matches!(
            CompressionSnapshot::from_bytes(&bytes[..23]),
            Err(SnapshotError::BadLength(23))
        ));
        let mut bad_mode = bytes;
        bad_mode[0] = 7;
        assert!(matches!(
            CompressionSnapshot::from_bytes(&bad_mode),
            Err(SnapshotError::BadMode(7))
        ));
        let mut bad_ratio = bytes;
        bad_ratio[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(matches!(
            CompressionSnapshot::from_bytes(&bad_ratio),
            Err(SnapshotError::BadValue)
        ));
        // Restore path: a fresh clock adopts the snapshot exactly.
        let mut restored = CompressionClock::new();
        restored.ratio = snap.ratio;
        restored.sim_time_s = snap.sim_time_s;
        assert_eq!(restored.mode(), CompressionMode::Compressed);
        assert_eq!(restored.sim_time_s(), 1234.5);
    }
}
