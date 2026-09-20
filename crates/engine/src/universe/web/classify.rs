//! Stage C — T-web classification, node peaks, halo masses.
//!
//! Physics: the tidal tensor T_ij = ∂²Φ/∂x_i∂x_j of the initial
//! potential decides collapse dimensionality — an axis is "collapsed"
//! when its Hessian eigenvalue λ < −web_threshold (strongly negative).
//! Classification by count of collapsed axes: 3 = node, 2 = filament,
//! 1 = sheet, 0 = void (Forero-Romero T-web). Halo masses follow the
//! Press–Schechter n=0 shape (power law with exponential cutoff above
//! M\*), assigned by rank: the densest peak gets the rarest mass.
//!
//! Determinism design (read carefully — this is the delicate file):
//! eigenvalues come from Jacobi rotations with a FIXED sweep count and
//! sqrt-only rotation formulas (no `sin`/`cos`, no convergence branch,
//! no `acos`); peak ordering uses `total_cmp` + index tiebreaks; masses
//! come from a Simpson-integrated CDF table (the only `exp` calls in the
//! stage, confined to a value transform whose output is quantized before
//! hashing — the `cue.rs` precedent); virial radii use `cbrt` the same
//! way. Comparisons and integer decisions are exact everywhere.

use super::displace::EulerianField;
use super::field::index;
use super::params::CosmicWebParams;
use std::collections::BTreeMap;

/// Web type per cell: 0 = void, 1 = sheet, 2 = filament, 3 = node.
pub const VOID: u8 = 0;
/// Web type per cell: sheet (one collapsed axis).
pub const SHEET: u8 = 1;
/// Web type per cell: filament (two collapsed axes).
pub const FILAMENT: u8 = 2;
/// Web type per cell: node (three collapsed axes).
pub const NODE: u8 = 3;

/// Jacobi eigenvalues of a symmetric 3×3, descending. Fixed 8 sweeps
/// (no convergence branch — platform-independent iteration count);
/// rotations need only `sqrt`, which IEEE-754 mandates correctly
/// rounded, so the result replays bit-identically everywhere.
pub fn jacobi_eigenvalues(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    for _ in 0..8 {
        for (p, q) in [(0_usize, 1_usize), (0, 2), (1, 2)] {
            let apq = a[p][q];
            if apq != 0.0 {
                let tau = (a[q][q] - a[p][p]) / (2.0 * apq);
                let t =
                    (if tau >= 0.0 { 1.0 } else { -1.0 }) / (tau.abs() + (tau * tau + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                // Apply the (p, q) rotation JᵀAJ: rows, then the two
                // affected rows zipped for the column pass (q > p holds
                // for every pair, so split_at_mut(q) is exact).
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (head, tail) = a.split_at_mut(q);
                let (row_p, row_q) = (&mut head[p], &mut tail[0]);
                for (apk, aqk) in row_p.iter_mut().zip(row_q.iter_mut()) {
                    let (a0, b0) = (*apk, *aqk);
                    *apk = c * a0 - s * b0;
                    *aqk = s * a0 + c * b0;
                }
            }
        }
    }
    // Descending sort via a 3-element sorting network (exact comparisons).
    let mut d = [a[0][0], a[1][1], a[2][2]];
    if d[0] < d[1] {
        d.swap(0, 1);
    }
    if d[1] < d[2] {
        d.swap(1, 2);
    }
    if d[0] < d[1] {
        d.swap(0, 1);
    }
    d
}

/// Second central difference of the potential along one axis (cell units,
/// periodic wrap — callers iterate the full lattice).
fn second(field: &[f64], n: usize, x: usize, y: usize, z: usize, axis: usize) -> f64 {
    let w = |v: usize, d: i64| (v as i64 + d).rem_euclid(n as i64) as usize;
    let (xp, xm) = match axis {
        0 => (index(n, w(x, 1), y, z), index(n, w(x, -1), y, z)),
        1 => (index(n, x, w(y, 1), z), index(n, x, w(y, -1), z)),
        _ => (index(n, x, y, w(z, 1)), index(n, x, y, w(z, -1))),
    };
    field[xp] - 2.0 * field[index(n, x, y, z)] + field[xm]
}

/// Mixed second difference ∂²/∂a∂b (cell units), central, /4.
fn mixed(field: &[f64], n: usize, x: usize, y: usize, z: usize, a: usize, b: usize) -> f64 {
    let corner = |dx: i64, dy: i64, dz: i64| {
        let w = |v: usize, d: i64| (v as i64 + d).rem_euclid(n as i64) as usize;
        field[index(n, w(x, dx), w(y, dy), w(z, dz))]
    };
    let (da, db) = match (a, b) {
        (0, 1) => ((1, 0, 0), (0, 1, 0)),
        (0, 2) => ((1, 0, 0), (0, 0, 1)),
        _ => ((0, 1, 0), (0, 0, 1)),
    };
    (corner(da.0 + db.0, da.1 + db.1, da.2 + db.2)
        - corner(da.0 - db.0, da.1 - db.1, da.2 - db.2)
        - corner(-da.0 + db.0, -da.1 + db.1, -da.2 + db.2)
        + corner(-da.0 - db.0, -da.1 - db.1, -da.2 - db.2))
        * 0.25
}

/// One node-candidate peak: lattice cell + Eulerian density.
#[derive(Clone, Copy, Debug)]
pub struct Peak {
    /// Flat lattice index.
    pub cell: usize,
    /// Eulerian (deposited) density at the cell.
    pub density: f64,
}

/// Classified web: per-cell types, ordered peaks, void fraction.
pub struct ClassifiedWeb {
    /// `n³` web types ([`VOID`]/[`SHEET`]/[`FILAMENT`]/[`NODE`]).
    pub web_type: Vec<u8>,
    /// Node-candidate peaks, densest-first (density desc, flat index
    /// asc tiebreak — fully deterministic).
    pub peaks: Vec<Peak>,
    /// Fraction of interior cells classified void.
    pub void_fraction: f64,
}

/// T-web classification of the potential Hessian + peak extraction on
/// the Eulerian density. Deep voids (Eulerian density below the
/// VoidFinder cut) skip the eigensolver entirely.
pub fn classify(
    potential: &[f64],
    euler: &EulerianField,
    params: &CosmicWebParams,
) -> ClassifiedWeb {
    let n = params.lattice_cells as usize;
    let margin = params.border_cells() as usize;
    let threshold = params.web_threshold;
    let void_cut = params.void_density_ratio * euler.mean;
    let peak_cut = params.peak_density_ratio * euler.mean;
    let mut web_type = vec![VOID; n * n * n];
    let mut void_cells = 0_usize;
    let mut interior = 0_usize;
    for z in margin..n - margin {
        for y in margin..n - margin {
            for x in margin..n - margin {
                let idx = index(n, x, y, z);
                interior += 1;
                if euler.density[idx] < void_cut {
                    void_cells += 1;
                    continue;
                }
                let h = [
                    [
                        second(potential, n, x, y, z, 0),
                        mixed(potential, n, x, y, z, 0, 1),
                        mixed(potential, n, x, y, z, 0, 2),
                    ],
                    [
                        mixed(potential, n, x, y, z, 0, 1),
                        second(potential, n, x, y, z, 1),
                        mixed(potential, n, x, y, z, 1, 2),
                    ],
                    [
                        mixed(potential, n, x, y, z, 0, 2),
                        mixed(potential, n, x, y, z, 1, 2),
                        second(potential, n, x, y, z, 2),
                    ],
                ];
                let eig = jacobi_eigenvalues(h);
                // Node = local maximum: all three eigenvalues strongly
                // negative (concave-down in every direction).
                let collapsed = eig.iter().filter(|e| **e < -threshold).count();
                web_type[idx] = match collapsed {
                    3 => NODE,
                    2 => FILAMENT,
                    1 => SHEET,
                    _ => {
                        void_cells += 1;
                        VOID
                    }
                };
            }
        }
    }
    // Peaks: collapsed-environment cells (node OR filament — real groups
    // live inside filaments, not only at triple-collapse nodes) above the
    // peak cut that are local maxima in their 3×3×3 neighborhood (ties →
    // lowest flat index wins).
    let mut peaks = Vec::new();
    for z in margin..n - margin {
        for y in margin..n - margin {
            for x in margin..n - margin {
                let idx = index(n, x, y, z);
                if web_type[idx] < FILAMENT || euler.density[idx] < peak_cut {
                    continue;
                }
                let d = euler.density[idx];
                let mut is_peak = true;
                for dz in -1..=1 {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 && dz == 0 {
                                continue;
                            }
                            let nx = (x as i64 + dx).rem_euclid(n as i64) as usize;
                            let ny = (y as i64 + dy).rem_euclid(n as i64) as usize;
                            let nz = (z as i64 + dz).rem_euclid(n as i64) as usize;
                            let nidx = index(n, nx, ny, nz);
                            let nd = euler.density[nidx];
                            if nd > d || (nd == d && nidx < idx) {
                                is_peak = false;
                                break;
                            }
                        }
                        if !is_peak {
                            break;
                        }
                    }
                    if !is_peak {
                        break;
                    }
                }
                if is_peak {
                    peaks.push(Peak {
                        cell: idx,
                        density: d,
                    });
                }
            }
        }
    }
    peaks.sort_by(|a, b| {
        b.density
            .total_cmp(&a.density)
            .then_with(|| a.cell.cmp(&b.cell))
    });
    ClassifiedWeb {
        web_type,
        peaks,
        void_fraction: void_cells as f64 / interior as f64,
    }
}

/// Press–Schechter-shaped (n=0) mass sampler: pdf ∝ x^-1/2 · e^-x on
/// x = M/M\* in `[x_min, x_max]`, inverted from a Simpson-integrated CDF
/// table. Substituting y = √x turns the density into a truncated
/// Gaussian (`2·e^-y²`), so a uniform Simpson grid in y is accurate and
/// inversion is binary search + lerp (basic ops). The `exp` calls live
/// only in the one-time table build — a value transform whose output is
/// quantized before hashing (the `cue.rs` precedent).
pub struct PsMassTable {
    y: Vec<f64>,
    cdf: Vec<f64>,
    x_min: f64,
    x_max: f64,
}

impl PsMassTable {
    /// Build the table: `points` Simpson intervals (even count required
    /// internally — rounded up), x range `[x_min, x_max]`.
    pub fn new(x_min: f64, x_max: f64, points: usize) -> Self {
        let intervals = points.max(8) + (points.max(8) % 2);
        let y_lo = x_min.sqrt();
        let y_hi = x_max.sqrt();
        let h = (y_hi - y_lo) / intervals as f64;
        let density = |y: f64| (-y * y).exp();
        // Simpson cumulative integral of 2·e^-y².
        let mut y = Vec::with_capacity(intervals + 1);
        let mut cdf = Vec::with_capacity(intervals + 1);
        let mut acc = 0.0;
        y.push(y_lo);
        cdf.push(0.0);
        let mut prev = density(y_lo);
        for i in 1..=intervals {
            let yi = y_lo + i as f64 * h;
            let di = density(yi);
            let mid = density(yi - 0.5 * h);
            acc += (h / 6.0) * (prev + 4.0 * mid + di);
            y.push(yi);
            cdf.push(2.0 * acc);
            prev = di;
        }
        let total = 2.0 * acc;
        for v in cdf.iter_mut() {
            *v /= total;
        }
        Self {
            y,
            cdf,
            x_min,
            x_max,
        }
    }

    /// Quantile: rank `u ∈ [0, 1]` → x = M/M\*. Monotone by construction.
    pub fn quantile(&self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        let mut lo = 0_usize;
        let mut hi = self.cdf.len() - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.cdf[mid] <= u {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (c0, c1) = (self.cdf[lo], self.cdf[hi]);
        // Flat segments (c1 == c0) arise where tail increments stagnate
        // below accumulator resolution: resolve to the right edge when
        // at/above the flat, the left edge when below — monotone kept.
        let t = if c1 > c0 {
            (u - c0) / (c1 - c0)
        } else if u >= c1 {
            1.0
        } else {
            0.0
        };
        let y = self.y[lo] * (1.0 - t) + self.y[hi] * t;
        (y * y).clamp(self.x_min, self.x_max)
    }
}

/// Virial radius from mass: M = 200·ρc·(4/3)πr³ with the conventional
/// critical density ρc ≈ 1.36e11 M☉/Mpc³ (H₀ = 70 km/s/Mpc). `cbrt` is a
/// value transform — the result is quantized before hashing.
pub fn virial_radius_mpc(mass_msun: f64) -> f64 {
    const RHO_CRIT_MSUN_PER_MPC3: f64 = 1.36e11;
    (3.0 * mass_msun / (800.0 * std::f64::consts::PI * RHO_CRIT_MSUN_PER_MPC3)).cbrt()
}

/// Greedy peak acceptance with a spatial grid: densest-first, deterministic.
pub fn accept_peaks(
    peaks: &[Peak],
    params: &CosmicWebParams,
    positions_mpc: impl Fn(usize) -> [f64; 3],
) -> Vec<usize> {
    let sep = params.min_node_separation_mpc;
    let mut grid: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    let key = |p: [f64; 3]| {
        [
            (p[0] / sep).floor() as i64,
            (p[1] / sep).floor() as i64,
            (p[2] / sep).floor() as i64,
        ]
    };
    let mut accepted: Vec<usize> = Vec::new();
    for peak in peaks.iter().take(params.target_node_count as usize * 4) {
        if accepted.len() >= params.target_node_count as usize {
            break;
        }
        let pos = positions_mpc(peak.cell);
        let [kx, ky, kz] = key(pos);
        let mut clear = true;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(cell) = grid.get(&[kx + dx, ky + dy, kz + dz]) {
                        for other in cell {
                            let q = positions_mpc(accepted[*other]);
                            let d2 = (pos[0] - q[0]) * (pos[0] - q[0])
                                + (pos[1] - q[1]) * (pos[1] - q[1])
                                + (pos[2] - q[2]) * (pos[2] - q[2]);
                            if d2 < sep * sep {
                                clear = false;
                                break;
                            }
                        }
                    }
                    if !clear {
                        break;
                    }
                }
                if !clear {
                    break;
                }
            }
            if !clear {
                break;
            }
        }
        if clear {
            grid.entry(key(pos)).or_default().push(accepted.len());
            accepted.push(peak.cell);
        }
    }
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobi_is_exact_on_diagonals() {
        let eig = jacobi_eigenvalues([[3.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]]);
        assert_eq!(eig, [3.0, 2.0, 1.0]);
    }

    #[test]
    fn jacobi_matches_a_hand_computed_case() {
        // [[2,1,0],[1,2,0],[0,0,5]] → eigenvalues 3, 1, 5.
        let eig = jacobi_eigenvalues([[2.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 0.0, 5.0]]);
        for (got, want) in eig.iter().zip([5.0, 3.0, 1.0]) {
            assert!((got - want).abs() < 1e-9, "got {eig:?}");
        }
    }

    #[test]
    fn jacobi_replays_identically() {
        let m = [[4.0, 1.0, 2.0], [1.0, 3.0, 0.5], [2.0, 0.5, 6.0]];
        assert_eq!(jacobi_eigenvalues(m), jacobi_eigenvalues(m));
    }

    #[test]
    fn ps_table_is_monotone_and_bounded() {
        let table = PsMassTable::new(5.0e12 / 6.0e13, 40.0, 512);
        assert!((table.quantile(0.0) - 5.0e12 / 6.0e13).abs() < 1e-9);
        assert!((table.quantile(1.0) - 40.0).abs() < 1e-9);
        let mut prev = 0.0;
        for i in 0..=100 {
            let q = table.quantile(i as f64 / 100.0);
            assert!(q >= prev, "not monotone at {i}");
            prev = q;
        }
        // Median well below M\* (power-law dominated, cutoff shape).
        assert!(table.quantile(0.5) < 1.0);
    }

    #[test]
    fn virial_radius_matches_a_coma_scale_cluster() {
        // 1e15 M☉ → ~2 Mpc (Coma scale); 1e12 → ~0.2 Mpc (group scale).
        let coma = virial_radius_mpc(1.0e15);
        assert!((1.5..3.0).contains(&coma), "coma r_vir = {coma}");
        let group = virial_radius_mpc(1.0e12);
        assert!((0.1..0.4).contains(&group), "group r_vir = {group}");
    }
}
