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

/// SNORM quantization range for the displacement export (CGT-001): the
/// growth-scaled displacement `D₊·∇Ψ` is stored in cell units over
/// `±DISP_QUANT_RANGE_CELLS`, so one LSB is
/// `8 / 32767 ≈ 2.44e-4` cells (≈ 1 kpc at 4 Mpc cells). Values beyond
/// the range clamp (recorded; the nominal calibration measures the max).
pub const DISP_QUANT_RANGE_CELLS: f64 = 8.0;

/// Quantize a growth-scaled displacement component (cell units) to SNORM-ish `i16`.
pub fn quantize_disp(d: f64) -> i16 {
    (d / DISP_QUANT_RANGE_CELLS * f64::from(i16::MAX))
        .round()
        .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

/// Inverse of [`quantize_disp`]: dequantized displacement in cell units.
pub fn dequantize_disp(q: i16) -> f64 {
    f64::from(q) / f64::from(i16::MAX) * DISP_QUANT_RANGE_CELLS
}

/// Render sidecar: displaced tracers + packed grid. Never hashed, never
/// saved, never read by gameplay (ADR-025 §1).
#[derive(Clone, Debug, PartialEq)]
pub struct WebField {
    /// Tracers inside the descriptor sphere (nominal ≈ 1.0–1.1M).
    /// Kept until CGT-010 retires it in favour of `displacement`.
    pub tracers: Vec<WebTracer>,
    /// Growth-scaled Zel'dovich displacement `D₊·∇Ψ` per lattice cell
    /// (Lagrangian order, x fastest), quantized with [`quantize_disp`]
    /// over `±DISP_QUANT_RANGE_CELLS` (CGT-001). Length is `grid_cells³`.
    /// The splat vertex shader (CGT-005) fetches this instead of reading
    /// `tracers`; [`displace_sample`] is its exact CPU mirror.
    pub displacement: Vec<[i16; 3]>,
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
    /// Descriptor sphere radius in Mpc (tracers + veil cut to it;
    /// sphere center is the box center `[0, 0, 0]`).
    pub sphere_radius_mpc: f64,
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
            displacement: self.displacement.clone(),
            grid: self.grid.clone(),
            grid_cells: self.grid_cells,
            cell_size_mpc: self.cell_size_mpc,
            mean_density: self.mean_density,
            origin_mpc: self.origin_mpc,
            sphere_radius_mpc: self.sphere_radius_mpc,
        }
    }
}

/// CPU mirror of the `SPLAT_VERT` v2 shader mapping (CGT-002, ADR-026
/// §2): trilinear displacement fetch at Lagrangian lattice coords `q`
/// (fractional cell units) → Eulerian lattice coords, wrapped periodic.
///
/// `q` uses the same frame as the export loop (cell `(x, y, z)` spans
/// `[x, x+1)`); the return is wrapped to `[0, n)`. Integer lattice
/// points are exact trilinear nodes, so there the result equals
/// `q − dequantized(stored)` bit-exactly up to summation order, and
/// agrees with the unquantized `q − D₊·∇Ψ` within one quantum
/// (`DISP_QUANT_RANGE_CELLS / 32767` per axis — pinned by test).
pub fn displace_sample(field: &WebField, q: [f64; 3]) -> [f64; 3] {
    let n = field.grid_cells as usize;
    debug_assert!(n > 0, "displace_sample needs a built field");
    let n_f = n as f64;
    let wrap = |v: f64| v.rem_euclid(n_f);
    let (qx, qy, qz) = (wrap(q[0]), wrap(q[1]), wrap(q[2]));
    let (x0, y0, z0) = (
        (qx.floor() as usize) % n,
        (qy.floor() as usize) % n,
        (qz.floor() as usize) % n,
    );
    let (x1, y1, z1) = ((x0 + 1) % n, (y0 + 1) % n, (z0 + 1) % n);
    let (fx, fy, fz) = (qx - qx.floor(), qy - qy.floor(), qz - qz.floor());
    let at = |x: usize, y: usize, z: usize| {
        let d = field.displacement[super::field::index(n, x, y, z)];
        [
            dequantize_disp(d[0]),
            dequantize_disp(d[1]),
            dequantize_disp(d[2]),
        ]
    };
    // Same weight order as `trilinear_grad` so the shader port (CGT-005)
    // can be diffed against this function corner for corner.
    let c000 = at(x0, y0, z0);
    let c100 = at(x1, y0, z0);
    let c010 = at(x0, y1, z0);
    let c110 = at(x1, y1, z0);
    let c001 = at(x0, y0, z1);
    let c101 = at(x1, y0, z1);
    let c011 = at(x0, y1, z1);
    let c111 = at(x1, y1, z1);
    let mut d = [0.0; 3];
    for axis in 0..3 {
        let c00 = c000[axis] * (1.0 - fx) + c100[axis] * fx;
        let c10 = c010[axis] * (1.0 - fx) + c110[axis] * fx;
        let c01 = c001[axis] * (1.0 - fx) + c101[axis] * fx;
        let c11 = c011[axis] * (1.0 - fx) + c111[axis] * fx;
        let c0 = c00 * (1.0 - fy) + c10 * fy;
        let c1 = c01 * (1.0 - fy) + c11 * fy;
        d[axis] = c0 * (1.0 - fz) + c1 * fz;
    }
    [
        (qx - d[0]).rem_euclid(n_f),
        (qy - d[1]).rem_euclid(n_f),
        (qz - d[2]).rem_euclid(n_f),
    ]
}

/// Compact Lagrangian cell list driving the procedural splat draw
/// (CGT-003, ADR-026 §2): indices (x fastest) of cells whose displaced
/// centre lands within `sphere_radius_mpc + cell_size_mpc`, via
/// [`displace_sample`]. Pure function of `WebField`, ascending by
/// construction (canonical lattice order). Built once per seed on the
/// reseed / load path — never per travel.
pub fn cell_list(field: &WebField) -> Vec<u32> {
    let n = field.grid_cells as usize;
    let half = n as f64 * field.cell_size_mpc * 0.5;
    let bound = field.sphere_radius_mpc + field.cell_size_mpc;
    let mut out = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let e = displace_sample(field, [x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5]);
                let px = e[0] * field.cell_size_mpc - half;
                let py = e[1] * field.cell_size_mpc - half;
                let pz = e[2] * field.cell_size_mpc - half;
                if px * px + py * py + pz * pz <= bound * bound {
                    out.push((x + n * (y + n * z)) as u32);
                }
            }
        }
    }
    out
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
    // Displacement export (CGT-001): growth-scaled `D₊·∇Ψ` per cell from
    // the same central-difference stencil stage B uses (`build_gradient`
    // shares the stencil — no second math, just the stored grid the old
    // tracer loop re-derived inline). Quantized SNORM over ±8 cells.
    let growth = params.growth_factor;
    let displacement = build_gradient(potential, params)
        .iter()
        .map(|g| {
            [
                quantize_disp(growth * g[0]),
                quantize_disp(growth * g[1]),
                quantize_disp(growth * g[2]),
            ]
        })
        .collect();
    WebField {
        tracers,
        displacement,
        grid,
        grid_cells: params.lattice_cells,
        cell_size_mpc: cell,
        mean_density: mean,
        origin_mpc: [-half, -half, -half],
        sphere_radius_mpc: radius,
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

    #[test]
    fn displacement_quantagrees_within_one_quantum_on_small_box() {
        // CGT-002 agreement pin (FR7): at integer lattice points the
        // trilinear mirror is exact, so `displace_sample` must agree with
        // the unquantized `q − D₊·∇Ψ` within one quantum per axis
        // (quantum = 8/32767 cells). Integer points only — at
        // half-integer centres trilinear blends neighbours by design.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        assert_eq!(field.displacement.len(), 32 * 32 * 32);
        let n = p.lattice_cells as usize;
        let grad = build_gradient(&potential, &p);
        let quantum = DISP_QUANT_RANGE_CELLS / f64::from(i16::MAX);
        let mut max_err: f64 = 0.0;
        let mut max_abs_cells: f64 = 0.0;
        for z in (0..n).step_by(3) {
            for y in (0..n).step_by(3) {
                for x in (0..n).step_by(3) {
                    let g = grad[crate::universe::web::field::index(n, x, y, z)];
                    for gi in g.iter() {
                        max_abs_cells = max_abs_cells.max((p.growth_factor * gi).abs());
                    }
                    let q = [x as f64, y as f64, z as f64];
                    let got = displace_sample(&field, q);
                    for axis in 0..3 {
                        let want = (q[axis] - p.growth_factor * g[axis]).rem_euclid(n as f64);
                        let err = (got[axis] - want).abs();
                        max_err = max_err.max(err);
                        assert!(
                            err <= quantum + 1e-9,
                            "mirror drift at ({x},{y},{z}) axis {axis}: {err} > {quantum}"
                        );
                    }
                }
            }
        }
        // Range check: nothing may clamp on the calibration boxes, or the
        // shader would silently fold large displacements (recorded value).
        assert!(
            max_abs_cells < DISP_QUANT_RANGE_CELLS,
            "displacement clamps: max {max_abs_cells} cells ≥ 8"
        );
    }

    #[test]
    fn cell_list_sorted_inside_and_deterministic_on_small_box() {
        // CGT-003 pins: determinism, ascending order, every listed cell's
        // displaced centre inside R + cell.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(
            11,
            &potential,
            &euler,
            &classified,
            &p,
            WebFieldBudget::Full,
        );
        let a = cell_list(&field);
        let b = cell_list(&field);
        assert_eq!(a, b, "cell_list not deterministic");
        assert!(!a.is_empty());
        let mut prev = 0u32;
        let n = p.lattice_cells as usize;
        let half = n as f64 * p.cell_size_mpc * 0.5;
        let bound = p.descriptor_radius_mpc + p.cell_size_mpc;
        for (k, &i) in a.iter().enumerate() {
            if k > 0 {
                assert!(i > prev, "cell_list not ascending at {k}");
            }
            prev = i;
            let x = (i as usize) % n;
            let y = ((i as usize) / n) % n;
            let z = (i as usize) / (n * n);
            let e = displace_sample(&field, [x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5]);
            let px = e[0] * p.cell_size_mpc - half;
            let py = e[1] * p.cell_size_mpc - half;
            let pz = e[2] * p.cell_size_mpc - half;
            let r2 = px * px + py * py + pz * pz;
            assert!(
                r2 <= bound * bound + 1e-6,
                "listed cell {i} outside R+cell: r²={r2}"
            );
        }
    }

    #[test]
    fn cell_list_nominal_count_band() {
        // CGT-003 count band at nominal (128³, seed 1234): displaced
        // centres inside R + cell ≈ sphere shell volume ≈ tracer band
        // widened by the +cell margin (~5 %). Slow (~1 min + ≤ 50 ms
        // list); same cost class as `nominal_tracer_count_band`.
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
        let cells = cell_list(&field);
        eprintln!("nominal cell count: {}", cells.len());
        // No SNORM clamp on nominal: the shader would silently fold larger
        // displacements (recorded value for the CGT-005 port).
        let grad = build_gradient(&potential, &p);
        let max_d = grad
            .iter()
            .flat_map(|g| g.iter())
            .map(|v| (p.growth_factor * v).abs())
            .fold(0.0f64, f64::max);
        eprintln!("nominal max |D+ grad|: {max_d:.4} cells");
        assert!(
            max_d < DISP_QUANT_RANGE_CELLS,
            "nominal displacement clamps: {max_d} cells ≥ 8"
        );
        assert!(
            (900_000..=1_300_000).contains(&cells.len()),
            "nominal cell count out of band: {}",
            cells.len()
        );
        // Memory (NFR2): grid 16.8 MB + list ≈ 4.4 MB.
        let bytes = field.displacement.len() * size_of::<[i16; 3]>()
            + field.grid.len()
            + cells.len() * size_of::<u32>();
        assert!(
            bytes <= 24 * 1024 * 1024,
            "procedural sidecar over budget: {bytes} B"
        );
    }
}
