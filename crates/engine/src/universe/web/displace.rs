//! Stage B — Zel'dovich displacement into Eulerian density.
//!
//! Physics: comoving position `x(q) = q − D₊·∇Ψ(q)` — matter flows down
//! the initial-potential gradient, so overdensities contract and voids
//! evacuate. The displaced unit-mass tracers (one per lattice cell) are
//! deposited onto the lattice with nearest-grid-point (NGP), producing
//! the clumped Eulerian density the later stages classify. Gradient and
//! deposit use central differences, `round`, and comparisons only —
//! basic ops, deterministic on every IEEE-754 platform.

use super::field::index;
use super::params::CosmicWebParams;

/// Eulerian density grid plus its interior mean (normalization for the
/// density-ratio cuts in [`super::classify`]).
pub struct EulerianField {
    /// NGP-deposited tracer counts per cell (f64 for exact integer
    /// accumulation; values are whole numbers).
    pub density: Vec<f64>,
    /// Mean density over interior cells (border margin excluded).
    pub mean: f64,
    /// Lattice cells per axis.
    pub n: usize,
}

/// Periodic lattice read (the box wraps — see [`super::field`]).
fn at(field: &[f64], n: usize, x: i64, y: i64, z: i64) -> f64 {
    let w = |v: i64| v.rem_euclid(n as i64) as usize;
    field[index(n, w(x), w(y), w(z))]
}

/// Displace every lattice tracer by `−D₊·∇Ψ` (cell units) and deposit
/// with NGP, wrapping at the box faces (periodic). Mass is conserved
/// exactly: every tracer lands somewhere. The deposit is intentionally
/// NOT smoothed: NGP cell-scale texture is below the classification and
/// rendering scales, and smoothing was measured to collapse the void
/// fraction (0.63 → 0.18) by filling real evacuated cells (WS1 tuning
/// round 3 — kept raw).
pub fn displace_to_eulerian(potential: &[f64], params: &CosmicWebParams) -> EulerianField {
    let n = params.lattice_cells as usize;
    let growth = params.growth_factor;
    let wrap = |v: i64| v.rem_euclid(n as i64) as usize;
    let mut density = vec![0.0; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let gx = (at(potential, n, x as i64 + 1, y as i64, z as i64)
                    - at(potential, n, x as i64 - 1, y as i64, z as i64))
                    * 0.5;
                let gy = (at(potential, n, x as i64, y as i64 + 1, z as i64)
                    - at(potential, n, x as i64, y as i64 - 1, z as i64))
                    * 0.5;
                let gz = (at(potential, n, x as i64, y as i64, z as i64 + 1)
                    - at(potential, n, x as i64, y as i64, z as i64 - 1))
                    * 0.5;
                let ex = wrap((x as f64 - growth * gx).round() as i64);
                let ey = wrap((y as f64 - growth * gy).round() as i64);
                let ez = wrap((z as f64 - growth * gz).round() as i64);
                density[index(n, ex, ey, ez)] += 1.0;
            }
        }
    }
    let sum: f64 = density.iter().sum();
    EulerianField {
        density,
        mean: sum / (n * n * n) as f64,
        n,
    }
}

#[cfg(test)]
mod tests {
    use super::super::field::initial_field;
    use super::*;

    fn small_params() -> CosmicWebParams {
        CosmicWebParams::new(32, 4.0, 50.0).expect("small test params fit")
    }

    #[test]
    fn displacement_replays_identically() {
        let p = small_params();
        let potential = initial_field(42, &p);
        let a = displace_to_eulerian(&potential, &p);
        let b = displace_to_eulerian(&potential, &p);
        assert_eq!(a.density, b.density);
        assert_eq!(a.mean, b.mean);
    }

    #[test]
    fn mass_is_conserved_exactly() {
        let p = small_params();
        let potential = initial_field(9, &p);
        let euler = displace_to_eulerian(&potential, &p);
        // Periodic box: every tracer lands somewhere — the deposited
        // total equals the tracer count bit-exactly.
        let deposited: f64 = euler.density.iter().sum();
        assert_eq!(deposited, 32_usize.pow(3) as f64);
        assert!((euler.mean - 1.0).abs() < 1e-12);
    }

    #[test]
    fn displacement_clumps_and_evacuates() {
        let p = small_params();
        let potential = initial_field(9, &p);
        let euler = displace_to_eulerian(&potential, &p);
        let empty = euler.density.iter().filter(|d| **d == 0.0).count();
        let dense = euler
            .density
            .iter()
            .filter(|d| **d >= 3.0 * euler.mean)
            .count();
        assert!(empty > 0, "no evacuated cells — displacement did nothing");
        assert!(dense > 0, "no clumped cells — displacement did nothing");
    }
}
