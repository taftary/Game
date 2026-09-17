//! Procedural-body orbit distributions (spec §5 last row).
//!
//! Exoplanet/asteroid orbital elements drawn from seed + Kepler-inspired
//! population priors: log-flat periods, Rayleigh eccentricities,
//! isotropic inclinations. Outputs are quantized (see
//! [`crate::core`]) so the committed snapshot below holds on every
//! platform despite 1-ulp `libm` drift. The domain-separated seed
//! stream (`hierarchical-seeding`, next feature) plugs into `rng`;
//! until then callers pass `SeededRng` directly.
//!
//! ```
//! use game_engine::core::SeededRng;
//! use game_engine::physics::{sample_orbit, OrbitPriors};
//!
//! let mut rng = SeededRng::stream(7, "test/orbits");
//! let orbit = sample_orbit(&mut rng, &OrbitPriors::default());
//! assert!(orbit.eccentricity < 1.0 && orbit.period_days > 0.0);
//! ```

use crate::core::{SeededRng, quantize_f64};

/// Population priors for one procedural body. Kepler-mission-inspired
/// defaults; tunable when catalog statistics land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitPriors {
    /// Period range in days (log-flat sampling).
    pub period_min_days: f64,
    /// Period range in days (log-flat sampling).
    pub period_max_days: f64,
    /// Rayleigh σ for eccentricity.
    pub ecc_sigma: f64,
}

impl Default for OrbitPriors {
    fn default() -> Self {
        Self {
            period_min_days: 1.0,
            period_max_days: 10_950.0, // ~30 yr
            ecc_sigma: 0.08,
        }
    }
}

/// One sampled procedural orbit (quantized, platform-stable).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledOrbit {
    /// Orbital period in days.
    pub period_days: f64,
    /// Eccentricity in [0, 1).
    pub eccentricity: f64,
    /// Inclination in rad [0, π].
    pub inclination_rad: f64,
    /// Orbital phase in rad [0, 2π).
    pub phase_rad: f64,
}

/// Sample one orbit from `priors` using `rng`.
pub fn sample_orbit(rng: &mut SeededRng, priors: &OrbitPriors) -> SampledOrbit {
    debug_assert!(priors.period_min_days > 0.0 && priors.period_min_days < priors.period_max_days);
    debug_assert!(priors.ecc_sigma > 0.0);
    let u_period = rng.unit_f64();
    let u_ecc = rng.unit_f64();
    let u_inc = rng.unit_f64();
    let u_phase = rng.unit_f64();
    // Log-flat period; Rayleigh eccentricity via inversion
    // (e = σ√(−2 ln(1−u))), clamped below 1; isotropic inclination.
    let period =
        priors.period_min_days * (priors.period_max_days / priors.period_min_days).powf(u_period);
    let ecc = (priors.ecc_sigma * (-2.0 * (1.0 - u_ecc).ln()).sqrt()).min(0.99);
    let inc = (1.0 - 2.0 * u_inc).acos();
    let phase = u_phase * 2.0 * std::f64::consts::PI;
    // Quantize: powf/ln/acos may drift 1 ulp across platforms.
    SampledOrbit {
        period_days: quantize_f64(period, 1e9) as f64 / 1e9,
        eccentricity: quantize_f64(ecc, 1e12) as f64 / 1e12,
        inclination_rad: quantize_f64(inc, 1e12) as f64 / 1e12,
        phase_rad: quantize_f64(phase, 1e12) as f64 / 1e12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_stay_in_physical_ranges() {
        let mut rng = SeededRng::stream(99, "test/orbits");
        for _ in 0..1000 {
            let o = sample_orbit(&mut rng, &OrbitPriors::default());
            assert!((1.0..=10_950.0).contains(&o.period_days));
            assert!((0.0..1.0).contains(&o.eccentricity));
            assert!((0.0..=std::f64::consts::PI).contains(&o.inclination_rad));
            assert!((0.0..2.0 * std::f64::consts::PI).contains(&o.phase_rad));
        }
    }

    #[test]
    fn same_seed_replays_identical_orbits() {
        // Determinism contract: content = pure function of (seed,
        // position). hierarchical-seeding later supplies the stream;
        // the sampler itself must already be replay-stable.
        let mut a = SeededRng::stream(1234, "test/orbits");
        let mut b = SeededRng::stream(1234, "test/orbits");
        for _ in 0..10 {
            assert_eq!(
                sample_orbit(&mut a, &OrbitPriors::default()),
                sample_orbit(&mut b, &OrbitPriors::default())
            );
        }
    }

    #[test]
    fn committed_snapshot_pins_the_distribution() {
        // Committed 2026-09-17 (x86-64): guards silent prior/algorithm
        // drift. Change deliberately or not at all.
        let mut rng = SeededRng::stream(7, "test/orbits");
        let o = sample_orbit(&mut rng, &OrbitPriors::default());
        assert_eq!(o.period_days, 1.053_443_506);
        assert_eq!(o.eccentricity, 0.067_218_563_095);
        assert_eq!(o.inclination_rad, 0.385_418_672_911);
        assert_eq!(o.phase_rad, 4.992_471_724_691);
    }
}
