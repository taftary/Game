//! Stage-0 runtime parameters: the seeded Zel'dovich web.
//!
//! Every knob is a runtime parameter, never a constant baked into the
//! math: calibration rounds tune these (recorded in
//! `plans/v0.3.2/cosmic-scale-player/plan.md`, WS1 tuning outcome) while
//! the pipeline shape stays fixed. Changing any shipped value changes
//! output — a version-bump decision, never silent.

/// Tunable parameters for [`generate_cosmic_web`](super::generate_cosmic_web).
///
/// Nominal values reproduce the astrophysical calibration bands from the
/// notion (void fraction, void diameters, filament lengths, halo mass
/// function). Units are comoving megaparsecs unless noted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicWebParams {
    /// Lattice cells per axis (cubic lattice, `cells³` total).
    pub lattice_cells: u32,
    /// Comoving Mpc per lattice cell.
    pub cell_size_mpc: f64,
    /// Descriptor sphere radius in Mpc: a generation cut (ADR-026 §1,
    /// v0.3.4 `cosmic-sphere-clip`) — peaks whose refined position lies
    /// outside are rejected before acceptance, so every emitted node
    /// satisfies `|position_mpc| ≤ radius`. Must fit inside the lattice
    /// box with margin: `radius + 2 * cell <= cells * cell_size / 2`.
    pub descriptor_radius_mpc: f64,
    /// Gaussian smoothing scales (in cells) combined into the initial
    /// field, smallest first.
    pub smooth_sigmas_cells: [f64; 3],
    /// Weight per smoothing scale (same order as `smooth_sigmas_cells`).
    pub smooth_weights: [f64; 3],
    /// Zel'dovich growth factor D₊ ("cosmic time"): displacement is
    /// `x = q − D₊·∇Ψ` in cell units.
    pub growth_factor: f64,
    /// T-web eigenvalue threshold in field units: an axis counts as
    /// collapsed when its Hessian eigenvalue is below `−web_threshold`.
    pub web_threshold: f64,
    /// VoidFinder-style void cut: cells below this fraction of the mean
    /// Eulerian density are voids (verified working definition: < 10%
    /// of mean density).
    pub void_density_ratio: f64,
    /// Node-candidate cut: local maxima above this multiple of the mean
    /// Eulerian density.
    pub peak_density_ratio: f64,
    /// Filament-link cut: node-pair segments whose mean density exceeds
    /// this multiple of the mean Eulerian density become links.
    pub link_density_ratio: f64,
    /// Maximum node-pair separation linked into a filament, Mpc.
    pub linking_length_mpc: f64,
    /// Greedy exclusion radius between node peaks, Mpc.
    pub min_node_separation_mpc: f64,
    /// Upper bound on emitted nodes (peaks are taken densest-first).
    pub target_node_count: u32,
    /// Press–Schechter characteristic mass M\* in solar masses.
    pub mass_star_msun: f64,
    /// Lower mass cutoff in solar masses (rank mapping floor).
    pub mass_min_msun: f64,
    /// Home-node mass band in solar masses (Local-Group analog).
    pub home_mass_lo_msun: f64,
    /// Home-node mass band in solar masses (Local-Group analog).
    pub home_mass_hi_msun: f64,
    /// Dwarf glow points emitted per Mpc of filament (pre-budget).
    pub glow_per_mpc: f64,
    /// Hard cap on emitted glow points: the two-pass budget bounds the
    /// expected count, deterministic truncation enforces the cap.
    pub max_glow_points: u32,
    /// Transverse Gaussian sigma of glow jitter around a link, Mpc.
    pub glow_transverse_sigma_mpc: f64,
}

impl CosmicWebParams {
    /// Nominal v0.3.2 calibration (WS1 tuning outcome, recorded in
    /// `plan.md`): 128³ × 4 Mpc lattice = 512 Mpc box; the 250 Mpc
    /// descriptor sphere is the largest that fits with a 2-cell border
    /// margin (the notion's "~400 Mpc" was corrected for the box fit —
    /// see plan.md R-1 note).
    pub fn nominal() -> Self {
        Self {
            lattice_cells: 128,
            cell_size_mpc: 4.0,
            descriptor_radius_mpc: 250.0,
            smooth_sigmas_cells: [1.0, 2.0, 4.0],
            smooth_weights: [0.35, 0.7, 1.0],
            growth_factor: 3.0,
            web_threshold: 0.06,
            void_density_ratio: 0.1,
            peak_density_ratio: 1.7,
            link_density_ratio: 1.2,
            linking_length_mpc: 60.0,
            min_node_separation_mpc: 6.0,
            target_node_count: 6_000,
            mass_star_msun: 6.0e13,
            mass_min_msun: 5.0e12,
            home_mass_lo_msun: 1.0e12,
            home_mass_hi_msun: 1.0e13,
            glow_per_mpc: 2.0,
            max_glow_points: 150_000,
            glow_transverse_sigma_mpc: 2.0,
        }
    }

    /// Box half-width in Mpc.
    pub fn half_width_mpc(self) -> f64 {
        f64::from(self.lattice_cells) * self.cell_size_mpc * 0.5
    }

    /// Border margin in cells excluded from stencil loops.
    ///
    /// Zero: the lattice box is periodic (see `field.rs`), so stencils
    /// wrap instead of reading past an edge — there is no edge-effect
    /// zone and no margin to exclude.
    pub fn border_cells(self) -> i64 {
        0
    }

    /// Validated constructor: returns `None` when the descriptor sphere
    /// cannot fit inside the lattice box (one NGP-lookup cell of
    /// headroom required), or any band is degenerate.
    pub fn new(lattice_cells: u32, cell_size_mpc: f64, descriptor_radius_mpc: f64) -> Option<Self> {
        if lattice_cells < 16
            || !cell_size_mpc.is_finite()
            || cell_size_mpc <= 0.0
            || !descriptor_radius_mpc.is_finite()
            || descriptor_radius_mpc <= 0.0
        {
            return None;
        }
        let mut params = Self::nominal();
        params.lattice_cells = lattice_cells;
        params.cell_size_mpc = cell_size_mpc;
        params.descriptor_radius_mpc = descriptor_radius_mpc;
        if descriptor_radius_mpc + cell_size_mpc >= params.half_width_mpc() {
            return None;
        }
        Some(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_fits_the_box_with_margin() {
        let p = CosmicWebParams::nominal();
        assert!(p.descriptor_radius_mpc + p.cell_size_mpc < p.half_width_mpc());
    }

    #[test]
    fn constructor_rejects_oversize_spheres() {
        assert!(CosmicWebParams::new(128, 4.0, 250.0).is_some());
        // 400 Mpc does not fit a 512 Mpc box with margin.
        assert!(CosmicWebParams::new(128, 4.0, 400.0).is_none());
        assert!(CosmicWebParams::new(8, 4.0, 10.0).is_none());
        assert!(CosmicWebParams::new(32, 4.0, 50.0).is_some());
    }
}
