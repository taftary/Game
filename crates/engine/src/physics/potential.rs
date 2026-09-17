//! Galactic bulk-kinematics potential: Miyamoto-Nagai disk + NFW halo
//! (spec §5 formulas, ADR-018).
//!
//! Galactic units throughout: kiloparsecs, km/s, solar masses, with
//! `G = 4.300917270e-6 kpc·(km/s)²/M☉`. Mass parameters are provisional
//! reference values (`M_disk`, `M_vir`); the tests pin behavior —
//! `v(8 kpc)` in the measured Milky Way range plus a flat curve — not
//! absolute truth.
//!
//! ```
//! use game_engine::physics::GalacticPotential;
//!
//! let mw = GalacticPotential::milky_way();
//! let v8 = mw.circular_velocity_kms(8.0);
//! assert!((200.0..=250.0).contains(&v8), "v(8 kpc) = {v8} km/s");
//! ```

use glam::DVec3;

/// `G` in kpc·(km/s)²/M☉ (derived from SI; standard galactic value).
pub const G_GALACTIC: f64 = 4.300_917_270e-6;

/// Analytic Milky-Way-like potential: Miyamoto-Nagai disk + NFW halo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GalacticPotential {
    /// Disk mass in M☉ (provisional reference 6.5e10).
    pub disk_mass: f64,
    /// Disk scale length `a` in kpc (spec: 6.5).
    pub disk_a: f64,
    /// Disk scale height `b` in kpc (spec: 0.26).
    pub disk_b: f64,
    /// Halo characteristic mass in M☉ (provisional reference 1.0e12,
    /// consistent with `r_s = r_vir/c`, `c = 12`).
    pub halo_mass: f64,
    /// NFW scale radius `r_s` in kpc (spec: 20).
    pub halo_rs: f64,
}

impl GalacticPotential {
    /// Milky Way reference model (spec §5 parameters + provisional masses).
    pub fn milky_way() -> Self {
        Self {
            disk_mass: 6.5e10,
            disk_a: 6.5,
            disk_b: 0.26,
            halo_mass: 1.0e12,
            halo_rs: 20.0,
        }
    }

    /// Total potential Φ in (km/s)² at cylindrical radius `r_kpc` and
    /// height `z_kpc` (spec §5: Φ_disk + Φ_NFW).
    pub fn potential(&self, r_kpc: f64, z_kpc: f64) -> f64 {
        let d = (z_kpc * z_kpc + self.disk_b * self.disk_b).sqrt();
        let s = (r_kpc * r_kpc + (self.disk_a + d) * (self.disk_a + d)).sqrt();
        let phi_disk = -G_GALACTIC * self.disk_mass / s;
        let r = (r_kpc * r_kpc + z_kpc * z_kpc).sqrt().max(1e-9);
        let phi_halo = -G_GALACTIC * self.halo_mass / r * (1.0 + r / self.halo_rs).ln();
        phi_disk + phi_halo
    }

    /// Acceleration (−∇Φ) in (km/s)²/kpc at `pos_kpc` (galactocentric
    /// Cartesian, plane = xy). Analytic gradients, not finite
    /// differences — consumed by the integrator and later by
    /// `soi-handoff` blending.
    pub fn acceleration(&self, pos_kpc: DVec3) -> DVec3 {
        let r_cyl = (pos_kpc.x * pos_kpc.x + pos_kpc.y * pos_kpc.y).sqrt();
        let z = pos_kpc.z;
        // Disk: ∂Φ/∂R = GM·R/s³, ∂Φ/∂z = GM·(a+d)·(z/d)/s³.
        let d = (z * z + self.disk_b * self.disk_b).sqrt();
        let s_sq = r_cyl * r_cyl + (self.disk_a + d) * (self.disk_a + d);
        let s = s_sq.sqrt().max(1e-9);
        let disk_factor = G_GALACTIC * self.disk_mass / (s_sq * s);
        let dphi_dr = disk_factor * r_cyl;
        let dphi_dz = disk_factor * (self.disk_a + d) * (z / d.max(1e-9));
        // Halo: dΦ/dr = GM·ln(1+r/rs)/r² − GM/(r·(r+rs)).
        let r = pos_kpc.length().max(1e-9);
        let gm = G_GALACTIC * self.halo_mass;
        let dphi_dr_halo =
            gm * (1.0 + r / self.halo_rs).ln() / (r * r) - gm / (r * (r + self.halo_rs));
        // Assemble: radial part along (x, y)/R plus z, halo along r̂.
        let (ux, uy) = if r_cyl > 1e-9 {
            (pos_kpc.x / r_cyl, pos_kpc.y / r_cyl)
        } else {
            (0.0, 0.0)
        };
        let r_hat = pos_kpc / r;
        DVec3::new(
            -dphi_dr * ux - dphi_dr_halo * r_hat.x,
            -dphi_dr * uy - dphi_dr_halo * r_hat.y,
            -dphi_dz - dphi_dr_halo * r_hat.z,
        )
    }

    /// Circular velocity in km/s at cylindrical radius `r_kpc` (z = 0):
    /// `v² = R·∂Φ/∂R`.
    pub fn circular_velocity_kms(&self, r_kpc: f64) -> f64 {
        let d = self.disk_b; // z = 0 → d = b
        let s_sq = r_kpc * r_kpc + (self.disk_a + d) * (self.disk_a + d);
        let s = s_sq.sqrt();
        let gm_d = G_GALACTIC * self.disk_mass;
        let v2_disk = gm_d * r_kpc * r_kpc / (s_sq * s);
        let gm_h = G_GALACTIC * self.halo_mass;
        let r = r_kpc;
        let dphi = gm_h * (1.0 + r / self.halo_rs).ln() / (r * r) - gm_h / (r * (r + self.halo_rs));
        (v2_disk + r * dphi).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceleration_matches_finite_difference_of_potential() {
        // Validates the analytic gradients against the potential itself:
        // any transcription slip in ∂Φ/∂R, ∂Φ/∂z, or dΦ/dr breaks this.
        let mw = GalacticPotential::milky_way();
        let h = 1e-6;
        for pos in [
            DVec3::new(8.0, 0.5, 0.3),
            DVec3::new(3.0, -4.0, 1.2),
            DVec3::new(15.0, 2.0, -0.8),
        ] {
            let analytic = mw.acceleration(pos);
            let r = (pos.x * pos.x + pos.y * pos.y).sqrt();
            let dphi_dr = (mw.potential(r + h, pos.z) - mw.potential(r - h, pos.z)) / (2.0 * h);
            let dphi_dz = (mw.potential(r, pos.z + h) - mw.potential(r, pos.z - h)) / (2.0 * h);
            let ux = pos.x / r;
            let uy = pos.y / r;
            // The total potential is axisymmetric, so this cylindrical
            // difference is the full gradient (disk + halo).
            let numeric = DVec3::new(-dphi_dr * ux, -dphi_dr * uy, -dphi_dz);
            let rel = (analytic - numeric).length() / analytic.length().max(1e-9);
            assert!(rel < 1e-6, "gradient mismatch {rel} at {pos:?}");
        }
    }

    #[test]
    fn rotation_curve_is_flat_with_solar_value() {
        // PO tolerance table: rotation-curve flatness. Physical grounding:
        // v(8 kpc) in the measured Milky Way range pins the provisional
        // masses; flatness pins the disk+halo balance.
        let mw = GalacticPotential::milky_way();
        let v8 = mw.circular_velocity_kms(8.0);
        assert!((200.0..=250.0).contains(&v8), "v(8 kpc) = {v8}");
        let v15 = mw.circular_velocity_kms(15.0);
        let ratio = v8 / v15;
        assert!((0.85..=1.15).contains(&ratio), "v8/v15 = {ratio}");
    }

    #[test]
    fn potential_matches_closed_form_within_one_percent() {
        // PO tolerance table: implementation vs the spec §5 formulas,
        // transcribed independently (pow-form, no shared intermediates).
        let mw = GalacticPotential::milky_way();
        for (r, z) in [(8.0f64, 0.0f64), (2.0, 1.0), (20.0, -3.0)] {
            let d = (z * z + mw.disk_b.powi(2)).sqrt();
            let s = (r.powi(2) + (mw.disk_a + d).powi(2)).sqrt();
            let phi_disk = -G_GALACTIC * mw.disk_mass / s;
            let rr = (r * r + z * z).sqrt().max(1e-9);
            let phi_halo = -G_GALACTIC * mw.halo_mass / rr * (1.0 + rr / mw.halo_rs).ln();
            let expected = phi_disk + phi_halo;
            let got = mw.potential(r, z);
            let rel = (got - expected).abs() / expected.abs();
            assert!(rel < 0.01, "closed-form mismatch {rel} at ({r}, {z})");
        }
    }

    #[test]
    fn acceleration_points_inward() {
        let mw = GalacticPotential::milky_way();
        for pos in [DVec3::new(8.0, 0.0, 0.0), DVec3::new(1.0, 2.0, 5.0)] {
            let a = mw.acceleration(pos);
            assert!(a.dot(pos) < 0.0, "gravity must attract at {pos:?}");
        }
    }
}
