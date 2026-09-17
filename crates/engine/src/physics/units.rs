//! Per-frame unit systems (spec §5 non-dimensionalization) + exact local `G`.
//!
//! Raw SI (`G = 6.674×10⁻¹¹`, 10²⁶ m, 10⁴² kg) is poorly conditioned for
//! force math, so each active frame uses a well-conditioned local system.
//! `G` in local units is *derived* from SI constants — exactly — rather
//! than assumed: the spec's "normalize `G` to approximately 1" prose is
//! loose (the exact AU/day/solar-mass value is the Gaussian constant
//! squared, `k² = 2.95912208e-4`), so exactness wins here.
//!
//! ```
//! use game_engine::physics::{FrameUnits, M_SUN_KG};
//! use game_engine::frames::FrameId;
//!
//! // μ☉ recovered from the solar-system local G within constants precision.
//! let solar = FrameUnits::of(FrameId::SolarSystem);
//! let mu_si = solar.g_local() * solar.length_m.powi(3) / solar.time_s.powi(2)
//!     / solar.mass_kg
//!     * M_SUN_KG;
//! assert!((mu_si - 1.327_124_400_18e20).abs() / 1.327_124_400_18e20 < 1e-4);
//! ```

use crate::frames::{
    FrameId, METERS_PER_AU, METERS_PER_KM, METERS_PER_KPC, METERS_PER_MPC, METERS_PER_PC,
};

/// Newtonian `G` in SI (m³·kg⁻¹·s⁻², CODATA 2018).
pub const G_SI: f64 = 6.674_30e-11;
/// Solar mass in kg, derived as μ☉/`G_SI` (IAU μ☉ = 1.32712440018e20
/// m³/s²) so the pair is mutually consistent — a rounded M☉ with CODATA
/// G silently shifts every solar-system period at 1e-4 level.
/// Evaluates to ≈ 1.9884099e30.
pub const M_SUN_KG: f64 = 1.327_124_400_18e20 / G_SI;
/// Earth mass in kg, derived as μ⊕/`G_SI` (μ⊕ = 3.986004418e14 m³/s²)
/// for the same consistency. Evaluates to ≈ 5.972168e24.
pub const M_EARTH_KG: f64 = 3.986_004_418e14 / G_SI;
/// Day in seconds.
pub const DAY_S: f64 = 86_400.0;
/// Julian year in seconds.
pub const YEAR_S: f64 = 365.25 * DAY_S;
/// Kiloyear in seconds.
pub const KYR_S: f64 = 1_000.0 * YEAR_S;
/// Megayear in seconds.
pub const MYR_S: f64 = 1_000_000.0 * YEAR_S;
/// Gigayear in seconds.
pub const GYR_S: f64 = 1_000_000_000.0 * YEAR_S;

/// Length / time / mass scales of one frame's local unit system.
/// Lengths reuse [`FrameId::meters_per_unit`]; time and mass scales are
/// chosen per frame for well-conditioned dynamics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameUnits {
    /// Meters per local length unit.
    pub length_m: f64,
    /// Seconds per local time unit.
    pub time_s: f64,
    /// Kilograms per local mass unit.
    pub mass_kg: f64,
}

impl FrameUnits {
    /// Unit system for `frame`.
    pub fn of(frame: FrameId) -> Self {
        match frame {
            FrameId::Cosmological | FrameId::LocalGroup => Self {
                length_m: METERS_PER_MPC,
                time_s: GYR_S,
                mass_kg: 1.0e12 * M_SUN_KG,
            },
            FrameId::Galactocentric => Self {
                length_m: METERS_PER_KPC,
                time_s: MYR_S,
                mass_kg: 1.0e10 * M_SUN_KG,
            },
            FrameId::StellarNeighborhood => Self {
                length_m: METERS_PER_PC,
                time_s: KYR_S,
                mass_kg: M_SUN_KG,
            },
            FrameId::SolarSystem => Self {
                length_m: METERS_PER_AU,
                time_s: DAY_S,
                mass_kg: M_SUN_KG,
            },
            FrameId::Planetocentric(_) => Self {
                length_m: METERS_PER_KM,
                time_s: 1.0,
                mass_kg: M_EARTH_KG,
            },
            FrameId::LocalEnu(_) => Self {
                length_m: 1.0,
                time_s: 1.0,
                mass_kg: 1.0,
            },
        }
    }

    /// Gravitational constant in local units
    /// (length³·mass⁻¹·time⁻²), derived exactly from [`G_SI`].
    pub fn g_local(&self) -> f64 {
        G_SI * self.mass_kg * self.time_s.powi(2) / self.length_m.powi(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_g_is_gaussian_k_squared() {
        // k = 0.01720209895 exactly (Gaussian constant); G = k² in
        // AU³/(M☉·day²). Cross-checks the derivation chain, not just the
        // formula: any constant or exponent slip breaks this. Relative
        // bound (not exact): μ☉ itself carries ~1e-10 definition rounding.
        let g = FrameUnits::of(FrameId::SolarSystem).g_local();
        assert!(
            (g - 2.959_122_082_86e-4).abs() / 2.959_122_082_86e-4 < 1e-9,
            "g_local = {g}"
        );
    }

    #[test]
    fn planetary_g_recovers_earth_mu() {
        // μ⊕ = 3.986004418e5 km³/s² per Earth mass (standard gravitational
        // parameter, independently known).
        let g = FrameUnits::of(FrameId::Planetocentric(crate::frames::BodyId::EARTH)).g_local();
        assert!(
            (g - 3.986_004_418e5).abs() / 3.986_004_418e5 < 1e-9,
            "g_local = {g}"
        );
    }

    #[test]
    fn local_enu_is_plain_si() {
        let u = FrameUnits::of(FrameId::LocalEnu(crate::frames::BodyId::EARTH));
        assert_eq!(u.g_local(), G_SI);
    }

    #[test]
    fn every_frame_has_finite_positive_g() {
        // Cosmic frames have no canonical reference value; finiteness +
        // positivity is the contract (no zero/negative time or mass).
        let frames = [
            FrameId::Cosmological,
            FrameId::Galactocentric,
            FrameId::LocalGroup,
            FrameId::StellarNeighborhood,
            FrameId::SolarSystem,
        ];
        for frame in frames {
            let g = FrameUnits::of(frame).g_local();
            assert!(g.is_finite() && g > 0.0, "{frame:?}: g_local = {g}");
        }
    }
}
