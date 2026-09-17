//! Closed-form two-body Kepler solver (ship dominant case, spec §5).
//!
//! Elements ⇄ state-vector conversion plus the period law, in caller
//! units with caller `μ`. Elliptic orbits only (`elements_from_state`
//! returns `None` otherwise) — hyperbolic flybys land with
//! `soi-handoff` / `free-flight-navigation` if ever needed.
//!
//! ```
//! use game_engine::physics::{period, state_from_elements, Elements};
//!
//! // Earth, SI: a = 1 AU, e = 0.0167, μ☉ — period ≈ 365.25 days.
//! let el = Elements { a: 1.495_978_707e11, e: 0.0167, ..Elements::circular(1.495_978_707e11) };
//! let t = period(el.a, 1.327_124_400_18e20);
//! assert!((t / 86_400.0 - 365.25).abs() / 365.25 < 1e-3);
//! ```

use glam::DVec3;

/// Classical orbital elements (angles in radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elements {
    /// Semi-major axis (caller length units, > 0).
    pub a: f64,
    /// Eccentricity [0, 1).
    pub e: f64,
    /// Inclination.
    pub inc: f64,
    /// Longitude of ascending node.
    pub raan: f64,
    /// Argument of periapsis.
    pub argp: f64,
    /// Mean anomaly at epoch.
    pub m0: f64,
}

impl Elements {
    /// Circular equatorial helper (angles zeroed).
    pub fn circular(a: f64) -> Self {
        Self {
            a,
            e: 0.0,
            inc: 0.0,
            raan: 0.0,
            argp: 0.0,
            m0: 0.0,
        }
    }
}

/// Orbital period `2π√(a³/μ)` in caller time units.
pub fn period(a: f64, mu: f64) -> f64 {
    2.0 * std::f64::consts::PI * (a * a * a / mu).sqrt()
}

/// Solve Kepler's equation `M = E − e·sinE` for `E` (Newton iterations,
/// elliptic `e < 1`). Residual < 1e-12 on return.
pub fn solve_kepler(m: f64, e: f64) -> f64 {
    debug_assert!((0.0..1.0).contains(&e), "elliptic only");
    let mut e_anom = m + e * m.sin();
    for _ in 0..64 {
        let f = e_anom - e * e_anom.sin() - m;
        let fp = 1.0 - e * e_anom.cos();
        let step = f / fp;
        e_anom -= step;
        if step.abs() < 1e-14 {
            break;
        }
    }
    e_anom
}

/// State vector from elements at mean anomaly `m` (caller units).
pub fn state_from_elements(el: Elements, mu: f64, m: f64) -> (DVec3, DVec3) {
    let e_anom = solve_kepler(m, el.e);
    let (ce, se) = (e_anom.cos(), e_anom.sin());
    let n = (mu / (el.a * el.a * el.a)).sqrt();
    let denom = 1.0 - el.e * ce;
    // Perifocal position / velocity.
    let xp = el.a * (ce - el.e);
    let yp = el.a * (1.0 - el.e * el.e).sqrt() * se;
    let vxp = -el.a * n * se / denom;
    let vyp = el.a * n * (1.0 - el.e * el.e).sqrt() * ce / denom;
    // Perifocal → inertial (Vallado R3(−Ω)R1(−i)R3(−ω)).
    let (c_o, s_o) = (el.raan.cos(), el.raan.sin());
    let (c_i, s_i) = (el.inc.cos(), el.inc.sin());
    let (c_w, s_w) = (el.argp.cos(), el.argp.sin());
    let r11 = c_o * c_w - s_o * s_w * c_i;
    let r12 = -c_o * s_w - s_o * c_w * c_i;
    let r21 = s_o * c_w + c_o * s_w * c_i;
    let r22 = -s_o * s_w + c_o * c_w * c_i;
    let r31 = s_w * s_i;
    let r32 = c_w * s_i;
    (
        DVec3::new(
            r11 * xp + r12 * yp,
            r21 * xp + r22 * yp,
            r31 * xp + r32 * yp,
        ),
        DVec3::new(
            r11 * vxp + r12 * vyp,
            r21 * vxp + r22 * vyp,
            r31 * vxp + r32 * vyp,
        ),
    )
}

/// Classical elements from a state vector. `None` for non-elliptic
/// (`energy ≥ 0`) or degenerate (zero angular momentum) inputs.
pub fn elements_from_state(pos: DVec3, vel: DVec3, mu: f64) -> Option<Elements> {
    let r = pos.length();
    let v2 = vel.length_squared();
    let energy = v2 / 2.0 - mu / r;
    if energy >= 0.0 {
        return None;
    }
    let h_vec = pos.cross(vel);
    let h = h_vec.length();
    if h < 1e-300 {
        return None;
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let norm_angle = |mut x: f64| {
        x %= two_pi;
        if x < 0.0 {
            x += two_pi;
        }
        x
    };
    let a = -mu / (2.0 * energy);
    let e_vec = (pos * (v2 - mu / r) - vel * pos.dot(vel)) / mu;
    let e = e_vec.length();
    let inc = (h_vec.z / h).clamp(-1.0, 1.0).acos();
    // Node vector n = K × h.
    let n = DVec3::new(-h_vec.y, h_vec.x, 0.0);
    let n_len = n.length();
    let raan = if n_len > 1e-12 * h {
        norm_angle(n.y.atan2(n.x))
    } else {
        0.0
    };
    let argp = if e > 1e-12 {
        if n_len > 1e-12 * h {
            let cos_w = (n.dot(e_vec) / (n_len * e)).clamp(-1.0, 1.0);
            let w = cos_w.acos();
            if e_vec.z < 0.0 { two_pi - w } else { w }
        } else {
            norm_angle(e_vec.y.atan2(e_vec.x))
        }
    } else {
        0.0
    };
    // True anomaly → eccentric anomaly → mean anomaly.
    let cos_nu = (e_vec.dot(pos) / (e.max(1e-300) * r)).clamp(-1.0, 1.0);
    let mut nu = cos_nu.acos();
    if pos.dot(vel) < 0.0 {
        nu = two_pi - nu;
    }
    let m0 = if e > 1e-12 {
        let e_anom = 2.0 * (((1.0 - e).sqrt() / (1.0 + e).sqrt()) * (nu / 2.0).tan()).atan();
        norm_angle(e_anom - e * e_anom.sin())
    } else {
        norm_angle(nu)
    };
    Some(Elements {
        a,
        e,
        inc,
        raan,
        argp,
        m0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MU_SUN_SI: f64 = 1.327_124_400_18e20;
    const AU_M: f64 = 1.495_978_707e11;

    #[test]
    fn earth_period_matches_analytic_within_tolerance() {
        // PO tolerance table: orbital period within 0.1% of analytic.
        let t = period(AU_M, MU_SUN_SI);
        let expected_days = 365.25;
        let rel = (t / 86_400.0 - expected_days).abs() / expected_days;
        assert!(rel < 1e-3, "period {t} s, rel err {rel}");
    }

    #[test]
    fn kepler_equation_residual_is_tiny_everywhere() {
        for m in [0.0, 0.5, 1.0, 2.0, 3.0, 5.0, -1.0] {
            for e in [0.0, 0.0167, 0.2, 0.7, 0.9] {
                let e_anom = solve_kepler(m, e);
                let residual = (e_anom - e * e_anom.sin() - m).abs();
                assert!(residual < 1e-12, "M={m} e={e}: residual {residual}");
            }
        }
    }

    #[test]
    fn elements_state_round_trips() {
        let el = Elements {
            a: 2.5,
            e: 0.3,
            inc: 0.5,
            raan: 1.2,
            argp: 0.7,
            m0: 2.0,
        };
        let (pos, vel) = state_from_elements(el, 1.0, el.m0);
        let back = elements_from_state(pos, vel, 1.0).expect("elliptic");
        assert!((back.a - el.a).abs() / el.a < 1e-9);
        assert!((back.e - el.e).abs() < 1e-9);
        assert!((back.inc - el.inc).abs() < 1e-9);
        // Back at the same mean anomaly: propagate and compare position.
        let (pos2, _) = state_from_elements(back, 1.0, back.m0);
        assert!((pos2 - pos).length() / pos.length() < 1e-9);
    }

    #[test]
    fn hyperbolic_inputs_are_rejected() {
        // Escape velocity at r = 1, μ = 1: energy ≥ 0 → None.
        let out = elements_from_state(DVec3::X, DVec3::Y * 1.5, 1.0);
        assert!(out.is_none());
    }
}
