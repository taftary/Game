//! Field export: non-hashed render sidecar (ADR-025, `web-field-export`,
//! reshaped by ADR-026 `cosmic-gpu-tracers`).
//!
//! The sidecar carries what the procedural splat shader fetches: the
//! growth-scaled Zel'dovich displacement grid `D₊·∇Ψ` (quantized,
//! [`DISP_QUANT_RANGE_CELLS`]) plus the 128³ class + log-density grid
//! packing. The v0.3.3 displaced-vertex list (budget tiers and the
//! refinement helper) retired in CGT-010 — sub-samples are a draw count now, never stored vertices.
//!
//! [`generate_cosmic_web_with_field`](super::generate_cosmic_web_with_field)
//! is the single entry point; [`super::generate_cosmic_web`] delegates to
//! it and drops the field, so the descriptor path is byte-identical.

use super::classify::ClassifiedWeb;
use super::displace::EulerianField;
use super::field::index;
use super::params::CosmicWebParams;

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

/// Render sidecar: displacement grid + packed grid. Never hashed, never
/// saved, never read by gameplay (ADR-025 §1).
#[derive(Clone, Debug, PartialEq)]
pub struct WebField {
    /// Growth-scaled Zel'dovich displacement `D₊·∇Ψ` per lattice cell
    /// (Lagrangian order, x fastest), quantized with [`quantize_disp`]
    /// over `±DISP_QUANT_RANGE_CELLS` (CGT-001). Length is `grid_cells³`.
    /// The splat vertex shader (CGT-005) fetches this; [`displace_sample`]
    /// is its exact CPU mirror.
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
    /// Descriptor sphere radius in Mpc (cell list + veil cut to it;
    /// sphere center is the box center `[0, 0, 0]`).
    pub sphere_radius_mpc: f64,
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

/// Centre-sample fast path for [`cell_list`] (CGT-009, NFR3): the exact
/// arithmetic of [`displace_sample`] at a cell centre (`fx = fy = fz =
/// 0.5`), minus the per-call overhead the general path pays — no
/// `floor`, no float `rem_euclid` on the way in (integer centres are
/// already in `[0, n)`), one conditional wrap on the way out (exact:
/// `e ∈ [−8, n + 8)` with `n ≥ 16`, so a single add/sub equals
/// `rem_euclid`), same corner indices, same dequantization, same
/// lerp-tree order with `0.5` literals. Bit-identical to
/// `displace_sample([x + 0.5, y + 0.5, z + 0.5])` — pinned by
/// `centre_sample_matches_general_path` — so the list, its count band,
/// and the determinism pins are untouched; only the wall time drops
/// (dev 873 ms → release target ≤ 50 ms nominal).
pub fn displace_sample_centre(field: &WebField, x: usize, y: usize, z: usize) -> [f64; 3] {
    let n = field.grid_cells as usize;
    debug_assert!(n > 0, "centre sample needs a built field");
    let n_f = n as f64;
    // Next-corner wrap without float modulo (`n ≥ 16` per params).
    let x1 = if x + 1 < n { x + 1 } else { 0 };
    let y1 = if y + 1 < n { y + 1 } else { 0 };
    let z1 = if z + 1 < n { z + 1 } else { 0 };
    let at = |cx: usize, cy: usize, cz: usize| {
        let d = field.displacement[super::field::index(n, cx, cy, cz)];
        [
            dequantize_disp(d[0]),
            dequantize_disp(d[1]),
            dequantize_disp(d[2]),
        ]
    };
    let c000 = at(x, y, z);
    let c100 = at(x1, y, z);
    let c010 = at(x, y1, z);
    let c110 = at(x1, y1, z);
    let c001 = at(x, y, z1);
    let c101 = at(x1, y, z1);
    let c011 = at(x, y1, z1);
    let c111 = at(x1, y1, z1);
    let mut d = [0.0; 3];
    for axis in 0..3 {
        let c00 = c000[axis] * 0.5 + c100[axis] * 0.5;
        let c10 = c010[axis] * 0.5 + c110[axis] * 0.5;
        let c01 = c001[axis] * 0.5 + c101[axis] * 0.5;
        let c11 = c011[axis] * 0.5 + c111[axis] * 0.5;
        let c0 = c00 * 0.5 + c10 * 0.5;
        let c1 = c01 * 0.5 + c11 * 0.5;
        d[axis] = c0 * 0.5 + c1 * 0.5;
    }
    let wrap = |v: f64| {
        if v < 0.0 {
            v + n_f
        } else if v >= n_f {
            v - n_f
        } else {
            v
        }
    };
    [
        wrap(x as f64 + 0.5 - d[0]),
        wrap(y as f64 + 0.5 - d[1]),
        wrap(z as f64 + 0.5 - d[2]),
    ]
}
/// Memoized centre evaluation over a pre-dequantized grid (CGT-009
/// NFR3): identical arithmetic to [`displace_sample_centre` — same
/// corner values, same lerp-tree order — but the caller dequantizes
/// each lattice cell once instead of 8× (every cell corners 8
/// stencils). Bit-identity with the oracle follows value-for-value;
/// pinned by `centre_sample_matches_general_path_bit_exact`.
fn centre_from_memo(dq: &[[f64; 3]], n: usize, x: usize, y: usize, z: usize) -> [f64; 3] {
    debug_assert!(n > 0, "centre sample needs a built field");
    let n_f = n as f64;
    let x1 = if x + 1 < n { x + 1 } else { 0 };
    let y1 = if y + 1 < n { y + 1 } else { 0 };
    let z1 = if z + 1 < n { z + 1 } else { 0 };
    let at = |cx: usize, cy: usize, cz: usize| dq[cx + n * (cy + n * cz)];
    let c000 = at(x, y, z);
    let c100 = at(x1, y, z);
    let c010 = at(x, y1, z);
    let c110 = at(x1, y1, z);
    let c001 = at(x, y, z1);
    let c101 = at(x1, y, z1);
    let c011 = at(x, y1, z1);
    let c111 = at(x1, y1, z1);
    let mut d = [0.0; 3];
    for axis in 0..3 {
        let c00 = c000[axis] * 0.5 + c100[axis] * 0.5;
        let c10 = c010[axis] * 0.5 + c110[axis] * 0.5;
        let c01 = c001[axis] * 0.5 + c101[axis] * 0.5;
        let c11 = c011[axis] * 0.5 + c111[axis] * 0.5;
        let c0 = c00 * 0.5 + c10 * 0.5;
        let c1 = c01 * 0.5 + c11 * 0.5;
        d[axis] = c0 * 0.5 + c1 * 0.5;
    }
    let wrap = |v: f64| {
        if v < 0.0 {
            v + n_f
        } else if v >= n_f {
            v - n_f
        } else {
            v
        }
    };
    [
        wrap(x as f64 + 0.5 - d[0]),
        wrap(y as f64 + 0.5 - d[1]),
        wrap(z as f64 + 0.5 - d[2]),
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
    // Dequantize once per lattice cell (CGT-009 NFR3): the inline path
    // re-derives each corner value 8× (every cell corners 8 stencils).
    // Transient `n³ × 24 B` (50 MB nominal), freed on return; the
    // resident sidecar (NFR2) is unchanged.
    let dq: Vec<[f64; 3]> = field.displacement[..n * n * n]
        .iter()
        .map(|d| {
            [
                dequantize_disp(d[0]),
                dequantize_disp(d[1]),
                dequantize_disp(d[2]),
            ]
        })
        .collect();
    // Conservative Lagrangian pre-filter (exact: skips only cells whose
    // displaced centre provably lands outside — dequantized displacement
    // is bounded by ±8 cells per axis, so a centre farther than
    // `bound + 8√3·cell` cannot come back; kept cells evaluate
    // identically, so the list is bit-for-bit the unfiltered one).
    let reach = bound + 8.0 * 3.0_f64.sqrt() * field.cell_size_mpc;
    let mut out = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let qx = (x as f64 + 0.5) * field.cell_size_mpc - half;
                let qy = (y as f64 + 0.5) * field.cell_size_mpc - half;
                let qz = (z as f64 + 0.5) * field.cell_size_mpc - half;
                if qx * qx + qy * qy + qz * qz > reach * reach {
                    continue;
                }
                let e = centre_from_memo(&dq, n, x, y, z);
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

/// Quantize `log2(1+δ)` over `[-4, +6]` to 6 bits.
pub fn quantize_log2(v: f64) -> u8 {
    ((v + 4.0) / 10.0 * 63.0).round().clamp(0.0, 63.0) as u8
}

/// Inverse of [`quantize_log2`].
pub fn unquantize_log2(q: u8) -> f32 {
    2.0_f32.powf(-4.0 + f32::from(q & 0x3F) / 63.0 * 10.0)
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

/// Staging bytes for the displacement 3D image (CGT-004, ADR-026 §2):
/// `displacement` (`[i16; 3]`, x fastest) as RGBA16_SNORM little-endian
/// (alpha 0 — the format has no RGB-only 3D variant). `128³ × 8 B ≈
/// 16.8 MB`. Empty for degenerate fields (no texture built).
pub fn displacement_image_bytes(field: &WebField) -> Vec<u8> {
    let n = field.grid_cells as usize;
    if n == 0 || field.displacement.len() < n * n * n {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n * n * n * 8);
    for d in &field.displacement[..n * n * n] {
        for c in [d[0], d[1], d[2], 0] {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

/// Build the sidecar from the already-computed stage products.
///
/// `potential` is the stage-A field (re-read, never regenerated).
/// No RNG, no budget: every lattice cell exports its displacement;
/// the draw count (`k`) lives in the shader, not here.
pub fn export_field(
    potential: &[f64],
    euler: &EulerianField,
    classified: &ClassifiedWeb,
    params: &CosmicWebParams,
) -> WebField {
    let n = params.lattice_cells as usize;
    let half = params.half_width_mpc();
    let cell = params.cell_size_mpc;
    let radius = params.descriptor_radius_mpc;
    let smooth = smooth3(&euler.density, n);
    let mean = euler.mean;
    // Grid export: class + 6-bit log2(1+δ_smooth).
    let mut grid = vec![0u8; n * n * n];
    for i in 0..n * n * n {
        let class = classified.web_type[i] & 0x03;
        let od = smooth[i] / mean;
        let log2od = if od > 0.0 { od.log2() } else { -4.0 };
        grid[i] = (class << 6) | quantize_log2(log2od);
    }
    // Displacement export (CGT-001): growth-scaled `D₊·∇Ψ` per cell from
    // the same central-difference stencil stage B uses. Quantized SNORM
    // over ±8 cells.
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
        let field = export_field(&potential, &euler, &classified, &p);
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
    fn sphere_cut_holds_for_grid_densities() {
        // The packed grid covers the whole lattice (no cut), but every
        // density decodes finite — the sphere cut lives in `cell_list`.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(&potential, &euler, &classified, &p);
        let n = p.lattice_cells as usize;
        for i in (0..n * n * n).step_by(977) {
            let od = field.overdensity_at(i);
            assert!(od.is_finite() && od >= 0.0);
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
        let field = export_field(&potential, &euler, &classified, &p);
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
    fn displacement_image_bytes_round_trip() {
        // CGT-004: RGBA16_SNORM LE staging — xyz echo the quantized
        // grid, alpha is 0, length is n³ × 8.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(&potential, &euler, &classified, &p);
        let n = p.lattice_cells as usize;
        let bytes = displacement_image_bytes(&field);
        assert_eq!(bytes.len(), n * n * n * 8);
        let cell = |i: usize| {
            let o = i * 8;
            [
                i16::from_le_bytes([bytes[o], bytes[o + 1]]),
                i16::from_le_bytes([bytes[o + 2], bytes[o + 3]]),
                i16::from_le_bytes([bytes[o + 4], bytes[o + 5]]),
                i16::from_le_bytes([bytes[o + 6], bytes[o + 7]]),
            ]
        };
        for i in [0, 1, 1000, n * n * n - 1] {
            assert_eq!(cell(i)[..3], field.displacement[i], "cell {i} drifted");
            assert_eq!(cell(i)[3], 0, "alpha must be 0 at cell {i}");
        }
        // Degenerate fields build no texture.
        let empty = WebField {
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 4.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 50.0,
        };
        assert!(displacement_image_bytes(&empty).is_empty());
    }

    #[test]
    fn cell_list_sorted_inside_and_deterministic_on_small_box() {
        // CGT-003 pins: determinism, ascending order, every listed cell's
        // displaced centre inside R + cell.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(&potential, &euler, &classified, &p);
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
    fn centre_sample_matches_general_path_bit_exact() {
        // CGT-009: the NFR3 fast path must be bit-identical to the
        // `displace_sample` oracle on every cell of the small box, so
        // the list, its count band, and determinism are untouched.
        let (potential, euler, classified, p) = small_products();
        let field = export_field(&potential, &euler, &classified, &p);
        let n = p.lattice_cells as usize;
        let dq: Vec<[f64; 3]> = field.displacement[..n * n * n]
            .iter()
            .map(|d| {
                [
                    dequantize_disp(d[0]),
                    dequantize_disp(d[1]),
                    dequantize_disp(d[2]),
                ]
            })
            .collect();
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    assert_eq!(
                        displace_sample_centre(&field, x, y, z),
                        displace_sample(&field, [x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5]),
                        "centre/general mismatch at ({x},{y},{z})"
                    );
                    assert_eq!(
                        centre_from_memo(&dq, n, x, y, z),
                        displace_sample(&field, [x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5]),
                        "memo/general mismatch at ({x},{y},{z})"
                    );
                }
            }
        }
    }

    #[test]
    fn cell_list_nominal_count_band() {
        // CGT-003 count band at nominal (128³, seed 1234): displaced
        // centres inside R + cell ≈ 51 % of the lattice (sphere shell
        // volume plus the +cell margin). Slow (~1 min); same cost class
        // as the other nominal pins.
        let p = CosmicWebParams::nominal();
        let potential = initial_field(1234, &p);
        let euler = displace_to_eulerian(&potential, &p);
        let classified = classify(&potential, &euler, &p);
        let field = export_field(&potential, &euler, &classified, &p);
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
