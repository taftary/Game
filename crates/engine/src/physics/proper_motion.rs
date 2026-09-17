//! Linear Gaia proper-motion extrapolation (spec §8, ADR-020).
//!
//! Real nearby-star positions + proper motions stream in with
//! `star-catalog-streaming` (v0.2.0); this module owns the math both
//! sides share: linear extrapolation from the Gaia DR3 epoch 2016.0,
//! valid ±1000 years, with an explicit out-of-window flag (never fail
//! silently — the flag feeds HUD/debug display).
//!
//! ```
//! use game_engine::physics::{extrapolate, ProperMotion};
//!
//! // 1 mas/yr for 1000 yr = 1 arcsec = 4.848e-6 rad.
//! let pm = ProperMotion { ra_rad: 0.0, dec_rad: 0.0, pm_ra_mas_yr: 1.0, pm_dec_mas_yr: 0.0 };
//! let (ra, _, validity) = extrapolate(pm, 1000.0);
//! assert!((ra - 4.848_136_811e-6).abs() / 4.848_136_811e-6 < 0.01);
//! assert!(validity.in_window());
//! ```

use crate::physics::ephemeris::Validity;

/// Gaia DR3 reference epoch (years).
pub const GAIA_EPOCH_YR: f64 = 2016.0;
/// Linear-extrapolation validity half-window (years, ADR-020).
pub const PROPER_MOTION_SPAN_YR: f64 = 1000.0;
/// Milliarcseconds per radian (arcsec/rad × 1000).
pub const MAS_PER_RAD: f64 = 206_264_806.247_096_36;

/// Cataloged star astrometry: epoch position + proper motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProperMotion {
    /// Right ascension at epoch (rad).
    pub ra_rad: f64,
    /// Declination at epoch (rad).
    pub dec_rad: f64,
    /// Proper motion in RA (mas/yr, catalog convention: includes cos δ).
    pub pm_ra_mas_yr: f64,
    /// Proper motion in dec (mas/yr).
    pub pm_dec_mas_yr: f64,
}

/// Linearly extrapolate to `years_since_epoch` after 2016.0.
/// Returns `(ra_rad, dec_rad, validity)`.
pub fn extrapolate(pm: ProperMotion, years_since_epoch: f64) -> (f64, f64, Validity) {
    let validity = if years_since_epoch.abs() <= PROPER_MOTION_SPAN_YR {
        Validity::InWindow
    } else {
        Validity::OutOfWindow
    };
    (
        pm.ra_rad + pm.pm_ra_mas_yr / MAS_PER_RAD * years_since_epoch,
        pm.dec_rad + pm.pm_dec_mas_yr / MAS_PER_RAD * years_since_epoch,
        validity,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Barnard's-star-class fast mover (mas/yr): exercises large
    // displacements without asserting any catalog truth.
    const FAST: ProperMotion = ProperMotion {
        ra_rad: 1.0,
        dec_rad: 0.5,
        pm_ra_mas_yr: -800.0,
        pm_dec_mas_yr: 10_300.0,
    };

    #[test]
    fn displacement_matches_rate_times_time() {
        // PO tolerance table: proper-motion displacement within 1%.
        let (ra, dec, v) = extrapolate(FAST, 100.0);
        let dra = (ra - FAST.ra_rad) * MAS_PER_RAD;
        let ddec = (dec - FAST.dec_rad) * MAS_PER_RAD;
        let got = (dra * dra + ddec * ddec).sqrt();
        let expected = ((800.0f64.powi(2) + 10_300.0f64.powi(2)).sqrt()) * 100.0;
        assert!((got - expected).abs() / expected < 0.01, "got {got} mas");
        assert!(v.in_window());
        // Absolute anchor: 1 mas/yr × 1000 yr = 1 arcsec = 4.848136811e-6
        // rad (catches unit slips like mas vs µas).
        let unit = ProperMotion {
            ra_rad: 0.0,
            dec_rad: 0.0,
            pm_ra_mas_yr: 1.0,
            pm_dec_mas_yr: 0.0,
        };
        let (ra1, _, _) = extrapolate(unit, 1000.0);
        assert!((ra1 - 4.848_136_811e-6).abs() / 4.848_136_811e-6 < 0.01);
    }

    #[test]
    fn window_edges_flag_correctly() {
        let (_, _, inside) = extrapolate(FAST, PROPER_MOTION_SPAN_YR);
        assert!(inside.in_window());
        let (_, _, outside) = extrapolate(FAST, PROPER_MOTION_SPAN_YR + 1.0);
        assert_eq!(outside, Validity::OutOfWindow);
        let (_, _, past) = extrapolate(FAST, -PROPER_MOTION_SPAN_YR - 1.0);
        assert_eq!(past, Validity::OutOfWindow);
    }
}
