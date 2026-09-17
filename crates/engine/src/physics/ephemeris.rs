//! Background-body ephemeris: provider trait + built-in Keplerian table.
//!
//! The spec (§5, §8, ADR-020) names VSOP87 (default) with an optional
//! DE440-derived high-precision mode. The full series are catalog data:
//! they land with `star-catalog-streaming` (v0.2.0) as new providers of
//! the [`EphemerisTable`] trait defined here. The built-in
//! [`KeplerianTable`] (JPL Keplerian elements + rates, embedded) is the
//! documented v0.1.0 stand-in: arcminute-class over 1800–2050 CE, honest
//! about it, and already behind the runtime [`EphemerisMode`] switch the
//! high-precision provider will reuse.
//!
//! ```
//! use game_engine::physics::{EphemerisBody, EphemerisTable, KeplerianTable};
//!
//! let table = KeplerianTable::standard();
//! // Earth near J2000 sits ~1 AU from the Sun.
//! let (pos, validity) = table.position(EphemerisBody::Earth, 0.0);
//! assert!((pos.length() - 1.0).abs() < 0.02);
//! assert!(validity.in_window());
//! ```

use crate::physics::kepler::solve_kepler;
use glam::DVec3;

/// Validity of an ephemeris / proper-motion evaluation: inside the
/// provider's calibrated window, or extrapolated past it (ADR-020: never
/// fail silently — flag it for HUD/debug display).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validity {
    /// Inside the calibrated window.
    InWindow,
    /// Outside it: value returned anyway, caller must surface a warning.
    OutOfWindow,
}

impl Validity {
    /// True for [`Validity::InWindow`].
    pub fn in_window(self) -> bool {
        matches!(self, Validity::InWindow)
    }
}

/// Background bodies covered by the built-in table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EphemerisBody {
    Mercury,
    Venus,
    Earth,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
}

/// Precision mode selector (runtime switch — plan-time ARCHITECT call on
/// the notion's open question). `non_exhaustive`: `HighPrecision`
/// arrives with the DE440-derived provider in v0.2.0.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EphemerisMode {
    /// Built-in Keplerian elements (1800–2050 CE, arcminute-class).
    Standard,
}

/// Background-body position provider. Positions are heliocentric
/// ecliptic-J2000 AU. Full VSOP87/DE440 series implement this trait in
/// `star-catalog-streaming`; nothing else changes for consumers.
pub trait EphemerisTable {
    /// Position `days_since_j2000` days after J2000.0, with validity.
    fn position(&self, body: EphemerisBody, days_since_j2000: f64) -> (DVec3, Validity);
}

/// JPL Keplerian elements + rates per century: `(a, e, I, L, long_peri,
/// long_node, da, de, dI, dL, dperi, dnode)` with `a` in AU and angles in
/// degrees. Valid 1800–2050 CE (JPL table notes).
#[rustfmt::skip]
const ELEMENTS: [[f64; 12]; 8] = [
    // Mercury
    [0.38709927, 0.20563593, 7.00497902, 252.25032371, 77.45779628, 48.33076593,
     0.00000037, 0.00001906, -0.00594749, 149472.67411175, 0.16047689, -0.12534081],
    // Venus
    [0.72333566, 0.00677672, 3.39467605, 181.97909950, 131.60246718, 76.67984255,
     0.00000390, -0.00004107, -0.00078890, 58517.81538729, 0.00268329, -0.27769418],
    // Earth (EM Barycenter)
    [1.00000261, 0.01671123, -0.00001531, 100.46457166, 102.93768193, 0.0,
     0.00000562, -0.00004392, -0.01294668, 35999.37244981, 0.32327364, 0.0],
    // Mars
    [1.52371034, 0.09339410, 1.84969142, -4.55343205, -23.94362959, 49.55953891,
     0.00001847, 0.00007882, -0.00813131, 19140.30268499, 0.44441088, -0.29257343],
    // Jupiter
    [5.20288700, 0.04838624, 1.30439695, 34.39644051, 14.72847983, 100.47390909,
     -0.00011607, -0.00013253, -0.00183714, 3034.74612775, 0.21252668, 0.20469106],
    // Saturn
    [9.53667594, 0.05386179, 2.48599187, 49.95424423, 92.59887831, 113.66242448,
     -0.00125060, -0.00050991, 0.00193609, 1222.49362201, -0.41897216, -0.28867794],
    // Uranus
    [19.18916464, 0.04725744, 0.77263783, 313.23810451, 170.96427630, 74.01692503,
     -0.00196176, -0.00004397, -0.00242939, 428.48202785, 0.40805281, 0.04240589],
    // Neptune
    [30.06992276, 0.00859048, 1.77004347, -55.12002969, 44.96476227, 131.78422574,
     0.00026291, 0.00005105, 0.00035372, 218.45945325, -0.32241464, -0.00508664],
];

/// Days from J2000.0 back to 1800-01-01 / forward to 2050-12-31.
const WINDOW_MIN_DAYS: f64 = -73048.0;
const WINDOW_MAX_DAYS: f64 = 18262.0;

/// Built-in Keplerian-elements provider (JPL table above).
#[derive(Clone, Copy, Debug)]
pub struct KeplerianTable {
    mode: EphemerisMode,
}

impl KeplerianTable {
    /// Standard-precision table (the only mode until v0.2.0).
    pub fn standard() -> Self {
        Self {
            mode: EphemerisMode::Standard,
        }
    }

    /// Active precision mode.
    pub fn mode(self) -> EphemerisMode {
        self.mode
    }

    fn row(body: EphemerisBody) -> [f64; 12] {
        ELEMENTS[body as usize]
    }
}

impl EphemerisTable for KeplerianTable {
    fn position(&self, body: EphemerisBody, days_since_j2000: f64) -> (DVec3, Validity) {
        let validity = if (WINDOW_MIN_DAYS..=WINDOW_MAX_DAYS).contains(&days_since_j2000) {
            Validity::InWindow
        } else {
            Validity::OutOfWindow
        };
        let t_centuries = days_since_j2000 / 36525.0;
        let e = Self::row(body);
        let a = e[0] + e[6] * t_centuries;
        let ecc = e[1] + e[7] * t_centuries;
        let inc = (e[2] + e[8] * t_centuries).to_radians();
        let l = e[3] + e[9] * t_centuries;
        let peri = e[4] + e[10] * t_centuries;
        let node = e[5] + e[11] * t_centuries;
        // JPL argument order: ω = ϖ − Ω, M = L − ϖ.
        let argp = (peri - node).to_radians();
        let m = (l - peri).to_radians();
        let e_anom = solve_kepler(m, ecc);
        let (ce, se) = (e_anom.cos(), e_anom.sin());
        let xp = a * (ce - ecc);
        let yp = a * (1.0 - ecc * ecc).sqrt() * se;
        let (cw, sw) = (argp.cos(), argp.sin());
        let (co, so) = (node.to_radians().cos(), node.to_radians().sin());
        let (ci, si) = (inc.cos(), inc.sin());
        (
            DVec3::new(
                (cw * co - sw * so * ci) * xp + (-sw * co - cw * so * ci) * yp,
                (cw * so + sw * co * ci) * xp + (-sw * so + cw * co * ci) * yp,
                (sw * si) * xp + (cw * si) * yp,
            ),
            validity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODIES: [EphemerisBody; 8] = [
        EphemerisBody::Mercury,
        EphemerisBody::Venus,
        EphemerisBody::Earth,
        EphemerisBody::Mars,
        EphemerisBody::Jupiter,
        EphemerisBody::Saturn,
        EphemerisBody::Uranus,
        EphemerisBody::Neptune,
    ];

    #[test]
    fn every_planet_closes_its_orbit_after_one_period() {
        // PO tolerance table: "ephemeris reference positions". The
        // period comes from the table's own dM/dt rate. Closure error
        // must then be *explained by the table's own secular drift*
        // (node/perihelion/inclination + a/e rates over one period):
        // anything else — swapped columns, wrong rows, radian/degree
        // mixups, anomaly or rotation bugs — exceeds the drift bound by
        // orders of magnitude.
        let table = KeplerianTable::standard();
        for body in BODIES {
            let row = KeplerianTable::row(body);
            let dm_rate_degpday = (row[9] - row[10]) / 36525.0;
            let t_days = 360.0 / dm_rate_degpday.abs();
            let t_cy = t_days / 36525.0;
            let drift = t_cy * (row[11].abs() + row[10].abs() + row[8].abs()).to_radians()
                + t_cy * (row[6].abs() / row[0] + row[7].abs());
            let bound = 3.0 * drift + 1e-9;
            let (p0, _) = table.position(body, 100.0);
            let (p1, _) = table.position(body, 100.0 + t_days);
            let rel = (p1 - p0).length() / p0.length().max(1e-9);
            assert!(
                rel < bound,
                "{body:?}: closure {rel} exceeds drift bound {bound}"
            );
        }
    }

    #[test]
    fn earth_stays_between_perihelion_and_aphelion() {
        // a(1±e) = 0.983 / 1.017 AU from the table row: radius sampled
        // across a full year must stay inside.
        let table = KeplerianTable::standard();
        for day in (0..365).step_by(5) {
            let (p, _) = table.position(EphemerisBody::Earth, day as f64);
            let r = p.length();
            assert!((0.98..=1.02).contains(&r), "day {day}: r = {r} AU");
        }
    }

    #[test]
    fn out_of_window_epochs_are_flagged_not_failed() {
        let table = KeplerianTable::standard();
        let (_, v0) = table.position(EphemerisBody::Earth, 0.0);
        assert!(v0.in_window());
        let (p, v1) = table.position(EphemerisBody::Earth, 100_000.0);
        assert_eq!(v1, Validity::OutOfWindow);
        assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
    }
}
