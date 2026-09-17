//! Symplectic integrator (velocity Verlet / leapfrog, ADR-018) + Euler
//! baseline for the stability comparison.
//!
//! Any numerically integrated regime uses Verlet: naive Euler visibly
//! drifts or decays orbits over long sessions. Generic over an
//! acceleration closure so every scale plugs its own model in.
//!
//! ```
//! use game_engine::physics::{orbital_energy, verlet_step, IntegratorState};
//! use glam::DVec3;
//!
//! // Circular orbit, nondimensional units (μ = 1, r = 1, v = 1).
//! let mut s = IntegratorState { pos: DVec3::X, vel: DVec3::Y };
//! let accel = |p: DVec3| -p / p.length().powi(3);
//! let e0 = orbital_energy(s.pos, s.vel, 1.0);
//! for _ in 0..628 {
//!     verlet_step(&mut s, 0.01, &accel);
//! }
//! let drift = (orbital_energy(s.pos, s.vel, 1.0) - e0).abs() / e0.abs();
//! assert!(drift < 1e-4);
//! ```

use glam::DVec3;

/// Position + velocity state for one integrated body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    /// Position (caller units).
    pub pos: DVec3,
    /// Velocity (caller units per caller time).
    pub vel: DVec3,
}

/// Specific orbital energy `v²/2 − μ/r` (caller units).
pub fn orbital_energy(pos: DVec3, vel: DVec3, mu: f64) -> f64 {
    0.5 * vel.length_squared() - mu / pos.length().max(1e-300)
}

/// One velocity-Verlet step (kick-drift-kick): symplectic, bounded
/// energy oscillation, no secular drift.
pub fn verlet_step(state: &mut State, dt: f64, accel: &impl Fn(DVec3) -> DVec3) {
    let a0 = accel(state.pos);
    state.vel += a0 * (0.5 * dt);
    state.pos += state.vel * dt;
    let a1 = accel(state.pos);
    state.vel += a1 * (0.5 * dt);
}

/// One explicit-Euler step. Baseline only: exhibits secular energy drift
/// (see the stability test) and must never drive gameplay integration.
pub fn euler_step(state: &mut State, dt: f64, accel: &impl Fn(DVec3) -> DVec3) {
    state.vel += accel(state.pos) * dt;
    state.pos += state.vel * dt;
}

#[cfg(test)]
mod tests {
    use super::*;

    const MU: f64 = 1.0;
    const ORBITS: usize = 1000;
    const STEPS_PER_ORBIT: usize = 200;

    fn circular() -> State {
        State {
            pos: DVec3::X,
            vel: DVec3::Y,
        }
    }

    fn gravity(p: DVec3) -> DVec3 {
        -p / p.length().powi(3)
    }

    #[test]
    fn verlet_holds_energy_over_a_thousand_orbits() {
        // PO tolerance table: relative energy drift < 1e-4 over 1,000
        // orbits. 200 steps/orbit × 1000 orbits of pure arithmetic.
        let dt = 2.0 * std::f64::consts::PI / STEPS_PER_ORBIT as f64;
        let mut s = circular();
        let e0 = orbital_energy(s.pos, s.vel, MU);
        let mut max_drift: f64 = 0.0;
        for _ in 0..ORBITS * STEPS_PER_ORBIT {
            verlet_step(&mut s, dt, &gravity);
            max_drift = max_drift.max((orbital_energy(s.pos, s.vel, MU) - e0).abs() / e0.abs());
        }
        assert!(max_drift < 1e-4, "Verlet drift {max_drift}");
        // …and the orbit is still circular, not just energetic.
        assert!(
            (s.pos.length() - 1.0).abs() < 1e-2,
            "radius {}",
            s.pos.length()
        );
    }

    #[test]
    fn euler_visibly_diverges_on_the_same_orbit() {
        // Same setup, Euler baseline: must diverge by ≥100× Verlet's
        // worst drift (the "naive Euler drifts" clause of ADR-018). The
        // comparison is direct — no absolute Euler threshold assumed.
        let dt = 2.0 * std::f64::consts::PI / STEPS_PER_ORBIT as f64;
        let mut v = circular();
        let e0 = orbital_energy(v.pos, v.vel, MU);
        let mut verlet_max: f64 = 0.0;
        for _ in 0..ORBITS * STEPS_PER_ORBIT {
            verlet_step(&mut v, dt, &gravity);
            verlet_max = verlet_max.max((orbital_energy(v.pos, v.vel, MU) - e0).abs() / e0.abs());
        }
        let mut s = circular();
        for _ in 0..ORBITS * STEPS_PER_ORBIT {
            euler_step(&mut s, dt, &gravity);
        }
        let euler_drift = (orbital_energy(s.pos, s.vel, MU) - e0).abs() / e0.abs();
        assert!(
            euler_drift > 100.0 * verlet_max.max(1e-12),
            "Euler {euler_drift} vs Verlet {verlet_max}"
        );
    }
}
