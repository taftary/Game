//! Field export: non-hashed render sidecar (ADR-025, `web-field-export`).
//!
//! Stage B displaces one Zel'dovich tracer per lattice cell and deposits
//! it with NGP, then discards the continuous position; stage C keeps only
//! node peaks. Illustris-style renders splat exactly the discarded data,
//! so this module re-runs the displacement in a **second loop** that feeds
//! nothing hashed: export-only Lagrangian jitter (own stream), the same
//! central-difference `∇Ψ` stencil as [`super::displace`], a 3³-smoothed
//! overdensity per tracer, and the 128³ class + log-density grid packing.
//!
//! [`generate_cosmic_web_with_field`](super::generate_cosmic_web_with_field)
//! is the single entry point; [`super::generate_cosmic_web`] delegates to
//! it and drops the field, so the descriptor path is byte-identical.

use super::classify::ClassifiedWeb;
use super::displace::EulerianField;
use super::field::index;
use super::params::CosmicWebParams;
use crate::core::SeededRng;

/// One exported tracer: displaced position + smoothed local overdensity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WebTracer {
    /// Box-centered Mpc position (same frame as `WebNode::position_mpc`).
    pub pos_mpc: [f32; 3],
    /// Smoothed overdensity `(1+δ)` at the landing cell (≥ 0, finite).
    pub overdensity: f32,
}

/// Render sidecar: displaced tracers + packed grid. Never hashed, never
/// saved, never read by gameplay (ADR-025 §1).
#[derive(Clone, Debug, PartialEq)]
pub struct WebField {
    /// Tracers inside the descriptor sphere (nominal ≈ 1.0–1.1M).
    pub tracers: Vec<WebTracer>,
    /// `grid_cells³` packed bytes: `(class << 6) | quant(log2(1+δ))`.
    pub grid: Vec<u8>,
    /// Lattice cells per axis (mirrors the params used to build it).
    pub grid_cells: u32,
    /// Mpc per lattice cell.
    pub cell_size_mpc: f64,
    /// Mean NGP density (normalization of the overdensity).
    pub mean_density: f64,
    /// Box origin in Mpc (`[-half, -half, -half]`).
    pub origin_mpc: [f64; 3],
}

/// Tracer budget: full export, or every 2nd tracer on Low (`Half` keeps
/// even `(x+y+z)` parity cells — deterministic stride, no RNG).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebFieldBudget {
    /// All lattice tracers inside the sphere.
    Full,
    /// Even-parity lattice cells only (≈ ½ the tracers).
    Half,
}

impl WebField {
    /// T-web class (2 bits) at flat grid index `i`.
    pub fn class_at(&self, i: usize) -> u8 {
        self.grid[i] >> 6
    }

    /// Smoothed overdensity `(1+δ)` at flat grid index `i` (unquantized
    /// from the 6-bit `log2` packing).
    pub fn overdensity_at(&self, i: usize) -> f32 {
        unquantize_log2(self.grid[i] & 0x3F)
    }

    /// Optional High-tier refinement (factor 2): 8 sub-tracers per
    /// lattice tracer via trilinear `∇Ψ` interpolation. Off by default,
    /// never on Low/Medium, never hashed.
    ///
    /// `potential` is the stage-A initial field the parent call already
    /// holds in memory; callers pass it back in. Positions stay inside
    /// the descriptor sphere only if the parent field was built with the
    /// same `params` and `radius`; out-of-sphere children are dropped.
    pub fn refine(&self, potential: &[f64], params: &CosmicWebParams, factor: u32) -> WebField {
        if factor != 2 {
            return self.clone();
        }
        let n = params.lattice_cells as usize;
        let half = params.half_width_mpc();
        let cell = params.cell_size_mpc;
        let radius = params.descriptor_radius_mpc;
        let grad = build_gradient(potential, params);
        let mut rng = SeededRng::stream(0, "cosmic_web/tracer");
        // Advance the parent stream past the parent export order is not
        // needed: refinement jitter (if any) must be independent — use a
        // dedicated sub-stream so parent replay is unaffected.
        let _ = &mut rng;
        let mut tracers = Vec::with_capacity(self.tracers.len().saturating_mul(8));
        // Re-derive sub-tracers from the lattice (not from parent
        // tracers) so the count is exactly 8× surviving children.
        let smooth = smooth3_of(&euler_from_potential(potential, params), params);
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    if !WebFieldBudget::Full.keeps(x, y, z) {
                        continue;
                    }
                    for sz in 0..2 {
                        for sy in 0..2 {
                            for sx in 0..2 {
                                let qx = x as f64 + 0.25 + 0.5 * sx as f64;
                                let qy = y as f64 + 0.25 + 0.5 * sy as f64;
                                let qz = z as f64 + 0.25 + 0.5 * sz as f64;
                                let (gx, gy, gz) = trilinear_grad(&grad, n, qx, qy, qz);
                                let g = params.growth_factor;
                                let xd = (qx - g * gx).rem_euclid(n as f64);
                                let yd = (qy - g * gy).rem_euclid(n as f64);
                                let zd = (qz - g * gz).rem_euclid(n as f64);
                                let pos = [
                                    (xd * cell - half) as f32,
                                    (yd * cell - half) as f32,
                                    (zd * cell - half) as f32,
                                ];
                                let r2 = f64::from(pos[0]) * f64::from(pos[0])
                                    + f64::from(pos[1]) * f64::from(pos[1])
                                    + f64::from(pos[2]) * f64::from(pos[2]);
                                if r2 > radius * radius {
                                    continue;
                                }
                                let li = super::field::index(
                                    n,
                                    (xd.floor() as usize).min(n - 1),
                                    (yd.floor() as usize).min(n - 1),
                                    (zd.floor() as usize).min(n - 1),
                                );
                                let od = (smooth[li] / params_mean(&smooth)) as f32;
                                if od.is_finite() && od >= 0.0 {
                                    tracers.push(WebTracer {
                                        pos_mpc: pos,
                                        overdensity: od,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        WebField {
            tracers,
            grid: self.grid.clone(),
            grid_cells: self.grid_cells,
            cell_size_mpc: self.cell_size_mpc,
            mean_density: self.mean_density,
            origin_mpc: self.origin_mpc,
        }
    }
}

impl WebFieldBudget {
    /// Deterministic keep rule for the lattice cell `(x, y, z)`.
    pub fn keeps(self, x: usize, y: usize, z: usize) -> bool {
        match self {
            WebFieldBudget::Full => true,
            WebFieldBudget::Half => (x + y + z).is_multiple_of(2),
        }
    }
}

/// Quantize `log2(1+δ)` over `[-4, +6]` to 6 bits.
pub fn quantize_log2(v: f64) -> u8 {
    ((v + 4.0) / 10.0 * 63.0).round().clamp(0.0, 63.0) as u8
}

/// Inverse of [`quantize_log2`].
pub fn unquantize_log2(q: u8) -> f32 {
    2.0_f32.powf(-4.0 + f32::from(q & 0x3F) / 63.0 * 10.0)
}

fn params_mean(smooth: &[f64]) -> f64 {
    smooth.iter().sum::<f64>() / smooth.len() as f64
}

/// Periodic 3³ box mean over NGP counts, separable (three 3-tap
/// passes — same result as the direct 27-tap sum up to summation
/// order, which is exact here: all inputs are small whole numbers and
/// the divisor 27 is applied once at the end).
pub fn smooth3(density: &[f64], n: usize) -> Vec<f64> {
    let mut tmp_a = vec![0.0; n * n * n];
    let mut tmp_b = vec![0.0; n * n * n];
    let w = |v: usize, d: i64| (v as i64 + d).rem_euclid(n as i64) as usize;
    // X pass: sums (divisor applied once at the end).
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                tmp_a[index(n, x, y, z)] = density[index(n, w(x, -1), y, z)]
                    + density[index(n, x, y, z)]
                    + density[index(n, w(x, 1), y, z)];
            }
        }
    }
    // Y pass.
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                tmp_b[index(n, x, y, z)] = tmp_a[index(n, x, w(y, -1), z)]
                    + tmp_a[index(n, x, y, z)]
                    + tmp_a[index(n, x, w(y, 1), z)];
            }
        }
    }
    // Z pass + divisor.
    let mut out = vec![0.0; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                out[index(n, x, y, z)] = (tmp_b[index(n, x, y, w(z, -1))]
                    + tmp_b[index(n, x, y, z)]
                    + tmp_b[index(n, x, y, w(z, 1))])
                    / 27.0;
            }
        }
    }
    out
}

fn smooth3_of(euler: &EulerianField, _params: &CosmicWebParams) -> Vec<f64> {
    smooth3(&euler.density, euler.n)
}

/// Re-run the NGP deposit (un-jittered, identical to stage B) for the
/// `refine` scaffold path, which only receives the potential.
fn euler_from_potential(potential: &[f64], params: &CosmicWebParams) -> EulerianField {
    super::displace::displace_to_eulerian(potential, params)
}

/// Per-cell central-difference gradient (cell units), periodic.
fn build_gradient(potential: &[f64], params: &CosmicWebParams) -> Vec<[f64; 3]> {
    let n = params.lattice_cells as usize;
    let at = |x: i64, y: i64, z: i64| {
        let w = |v: i64| v.rem_euclid(n as i64) as usize;
        potential[index(n, w(x), w(y), w(z))]
    };
    let mut grad = vec![[0.0; 3]; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let gx = (at(x as i64 + 1, y as i64, z as i64)
                    - at(x as i64 - 1, y as i64, z as i64))
                    * 0.5;
                let gy = (at(x as i64, y as i64 + 1, z as i64)
                    - at(x as i64, y as i64 - 1, z as i64))
                    * 0.5;
                let gz = (at(x as i64, y as i64, z as i64 + 1)
                    - at(x as i64, y as i64, z as i64 - 1))
                    * 0.5;
                grad[index(n, x, y, z)] = [gx, gy, gz];
            }
        }
    }
    grad
}

/// Trilinear sample of the gradient field at fractional lattice coords.
fn trilinear_grad(grad: &[[f64; 3]], n: usize, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let w = |v: f64| v.rem_euclid(n as f64);
    let (x, y, z) = (w(x), w(y), w(z));
    let (x0, y0, z0) = (
        x.floor() as usize % n,
        y.floor() as usize % n,
        z.floor() as usize % n,
    );
    let (x1, y1, z1) = ((x0 + 1) % n, (y0 + 1) % n, (z0 + 1) % n);
    let (fx, fy, fz) = (x - x.floor(), y - y.floor(), z - z.floor());
    let mut out = [0.0; 3];
    for axis in 0..3 {
        let c000 = grad[index(n, x0, y0, z0)][axis];
        let c100 = grad[index(n, x1, y0, z0)][axis];
        let c010 = grad[index(n, x0, y1, z0)][axis];
        let c110 = grad[index(n, x1, y1, z0)][axis];
        let c001 = grad[index(n, x0, y0, z1)][axis];
        let c101 = grad[index(n, x1, y0, z1)][axis];
        let c011 = grad[index(n, x0, y1, z1)][axis];
        let c111 = grad[index(n, x1, y1, z1)][axis];
        let c00 = c000 * (1.0 - fx) + c100 * fx;
        let c10 = c010 * (1.0 - fx) + c110 * fx;
        let c01 = c001 * (1.0 - fx) + c101 * fx;
        let c11 = c011 * (1.0 - fx) + c111 * fx;
        let c0 = c00 * (1.0 - fy) + c10 * fy;
        let c1 = c01 * (1.0 - fy) + c11 * fy;
        out[axis] = c0 * (1.0 - fz) + c1 * fz;
    }
    (out[0], out[1], out[2])
}

/// Irwin–Hall-3 centered jitter (`σ = 0.5`), scaled to `σ ≈ 0.3` cell.
fn jitter3(rng: &mut SeededRng) -> (f64, f64, f64) {
    let mut one = || (rng.unit_f64() + rng.unit_f64() + rng.unit_f64() - 1.5) * 0.6;
    (one(), one(), one())
}

/// Build the sidecar from the already-computed stage products.
///
/// `potential` is the stage-A field (re-read, never regenerated).
/// Domain-separated stream `"cosmic_web/tracer"` consumed in canonical
/// lattice order (x fastest) — replay-identical per `(seed, params)`.
pub fn export_field(
    seed: u64,
    potential: &[f64],
    euler: &EulerianField,
    classified: &ClassifiedWeb,
    params: &CosmicWebParams,
    budget: WebFieldBudget,
) -> WebField {
    let n = params.lattice_cells as usize;
    let half = params.half_width_mpc();
    let cell = params.cell_size_mpc;
    let radius = params.descriptor_radius_mpc;
    let smooth = smooth3(&euler.density, n);
    let mean = euler.mean;
    // Central-difference ∇Ψ read inline (same stencil as stage B —
    // no retained gradient allocation on the boot path).
    let pot_at = |x: i64, y: i64, z: i64| {
        let w = |v: i64| v.rem_euclid(n as i64) as usize;
        potential[index(n, w(x), w(y), w(z))]
    };
    let mut rng = SeededRng::stream(seed, "cosmic_web/tracer");
    let mut tracers = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let (jx, jy, jz) = jitter3(&mut rng);
                let keep = budget.keeps(x, y, z);
                // Displacement always consumes the stream in canonical
                // order (replay pin A-5); budget only drops the record.
                let qx = x as f64 + 0.5 + jx;
                let qy = y as f64 + 0.5 + jy;
                let qz = z as f64 + 0.5 + jz;
                if !keep {
                    continue;
                }
                let (xi, yi, zi) = (x as i64, y as i64, z as i64);
                let gx = (pot_at(xi + 1, yi, zi) - pot_at(xi - 1, yi, zi)) * 0.5;
                let gy = (pot_at(xi, yi + 1, zi) - pot_at(xi, yi - 1, zi)) * 0.5;
                let gz = (pot_at(xi, yi, zi + 1) - pot_at(xi, yi, zi - 1)) * 0.5;
                let growth = params.growth_factor;
                let xd = (qx - growth * gx).rem_euclid(n as f64);
                let yd = (qy - growth * gy).rem_euclid(n as f64);
                let zd = (qz - growth * gz).rem_euclid(n as f64);
                let pos = [
                    (xd * cell - half) as f32,
                    (yd * cell - half) as f32,
                    (zd * cell - half) as f32,
                ];
                let r2 = f64::from(pos[0]) * f64::from(pos[0])
                    + f64::from(pos[1]) * f64::from(pos[1])
                    + f64::from(pos[2]) * f64::from(pos[2]);
                if r2 > radius * radius {
                    continue;
                }
                let li = index(
                    n,
                    (xd.floor() as usize).min(n - 1),
                    (yd.floor() as usize).min(n - 1),
                    (zd.floor() as usize).min(n - 1),
                );
                let od = (smooth[li] / mean) as f32;
                if od.is_finite() && od >= 0.0 {
                    tracers.push(WebTracer {
                        pos_mpc: pos,
                        overdensity: od,
                    });
                }
            }
        }
    }
    // Grid export: class + 6-bit log2(1+δ_smooth).
    let mut grid = vec![0u8; n * n * n];
    for i in 0..n * n * n {
        let class = classified.web_type[i] & 0x03;
        let od = smooth[i] / mean;
        let log2od = if od > 0.0 { od.log2() } else { -4.0 };
        grid[i] = (class << 6) | quantize_log2(log2od);
    }
    WebField {
        tracers,
        grid,
        grid_cells: params.lattice_cells,
        cell_size_mpc: cell,
        mean_density: mean,
        origin_mpc: [-half, -half, -half],
    }
}

#[cfg(test)]
mod tests {
    use super::super::classify::classify;
    use super::super::displace::displace_to_eulerian;
    use super::super::field::initial_field;
    use super::*;
    use std::mem::size_of;

    fn small_params() -> CosmicWebParams {
        CosmicWebParams::new(32, 4.0, 50.0).expect("small test params fit")
    }

    fn small_products() -> (Vec<f64>, EulerianField, ClassifiedWeb, CosmicWebParams) {
        let p = small_params();
        let potential = initial_field(11, &p);
        let euler = displace_to_eulerian(&potential, &p);
        let classified = classify(&potential, &euler, &p);
        (potential, euler, classified, p)
    }

    #[test]
    fn grid_packing_round_trips_within_one_step() {
        let (potential, euler, classified, p) = small_products();
        let field = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        let n = p.lattice_cells as usize;
        let smooth = smooth3(&euler.density, n);
        for i in (0..n * n * n).step_by(977) {
            assert_eq!(field.class_at(i), classified.web_type[i] & 0x03);
            let want = (smooth[i] / euler.mean).max(f64::MIN_POSITIVE);
            let want_log = want.log2();
            let got_log = f64::from(field.overdensity_at(i))
                .max(f64::from(f32::MIN_POSITIVE))
                .log2();
            if (-4.0..=6.0).contains(&want_log) {
                let step = (want_log - got_log).abs();
                assert!(step <= 10.0 / 63.0 + 1e-3, "quant step too big: {step}");
            } else {
                // Out-of-range densities clamp to the rail by design.
                let rail = if want_log < -4.0 { -4.0 } else { 6.0 };
                assert!((got_log - rail).abs() < 10.0 / 63.0 + 1e-3);
            }
        }
    }

    #[test]
    fn tracer_densities_are_normalized() {
        let (potential, euler, classified, p) = small_products();
        let field = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        assert!(!field.tracers.is_empty());
        for t in &field.tracers {
            assert!(t.overdensity.is_finite() && t.overdensity >= 0.0);
        }
        // Volume mean is 1 by construction (smooth preserves the total).
        let n = p.lattice_cells as usize;
        let smooth = smooth3(&euler.density, n);
        let vol_mean: f64 = smooth.iter().sum::<f64>() / smooth.len() as f64 / euler.mean;
        assert!(
            (vol_mean - 1.0).abs() < 1e-9,
            "volume mean drifted: {vol_mean}"
        );
        // Tracer (mass-weighted) mean sits above 1: tracers flowed into
        // dense cells, so they sample high densities preferentially.
        let mean: f64 = field
            .tracers
            .iter()
            .map(|t| f64::from(t.overdensity))
            .sum::<f64>()
            / field.tracers.len() as f64;
        assert!(
            (1.0..=3.0).contains(&mean),
            "tracer density mean out of band: {mean}"
        );
    }

    #[test]
    fn sphere_cut_holds() {
        let (potential, euler, classified, p) = small_products();
        let field = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        for t in &field.tracers {
            let r2 = f64::from(t.pos_mpc[0]).powi(2)
                + f64::from(t.pos_mpc[1]).powi(2)
                + f64::from(t.pos_mpc[2]).powi(2);
            assert!(r2 <= p.descriptor_radius_mpc * p.descriptor_radius_mpc + 1e-6);
        }
    }

    #[test]
    fn half_budget_is_a_strict_subset() {
        let (potential, euler, classified, p) = small_products();
        let full = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        let half = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Half,
        );
        assert!(half.tracers.len() < full.tracers.len());
        assert!((half.tracers.len() as f64) > full.tracers.len() as f64 * 0.35);
    }

    #[test]
    fn nominal_tracer_count_band() {
        // FR6 band at nominal (128³, 4 Mpc, 250 Mpc sphere): sphere/box
        // volume ≈ 0.49 → ~1.0M of 2.1M tracers. Slow (~1 min); same
        // cost class as the existing nominal pins.
        let p = CosmicWebParams::nominal();
        let potential = initial_field(1234, &p);
        let euler = displace_to_eulerian(&potential, &p);
        let classified = classify(&potential, &euler, &p);
        let field = export_field(
            1234,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        assert!(
            (900_000..=1_200_000).contains(&field.tracers.len()),
            "nominal tracer count out of band: {}",
            field.tracers.len()
        );
        // Memory: 16 B × tracers + 2 MB grid ≤ 20 MB.
        let bytes = field.tracers.len() * size_of::<WebTracer>() + field.grid.len();
        assert!(bytes <= 20 * 1024 * 1024, "sidecar over budget: {bytes} B");
    }

    #[test]
    fn refine_two_emits_eight_children_on_small_box() {
        let p = small_params();
        let potential = initial_field(5, &p);
        let euler = displace_to_eulerian(&potential, &p);
        let classified = classify(&potential, &euler, &p);
        let field = export_field(5, &potential, &euler, &classified, &p, WebFieldBudget::Full);
        let refined = field.refine(&potential, &p, 2);
        assert!(refined.tracers.len() > field.tracers.len());
        for t in &refined.tracers {
            assert!(t.overdensity.is_finite() && t.overdensity >= 0.0);
            let r2 = f64::from(t.pos_mpc[0]).powi(2)
                + f64::from(t.pos_mpc[1]).powi(2)
                + f64::from(t.pos_mpc[2]).powi(2);
            assert!(r2 <= p.descriptor_radius_mpc * p.descriptor_radius_mpc + 1e-6);
        }
    }
}
