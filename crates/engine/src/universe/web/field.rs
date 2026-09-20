//! Stage A — Gaussian initial field on an integer Lagrangian lattice.
//!
//! Physics: primordial perturbations are Gaussian with a ΛCDM-like
//! spectrum (P(k) ∝ k at large scales, rolling over toward k⁻³). The game
//! approximation: hashed white noise (one domain-separated stream per
//! z-slice, ADR-019), Irwin–Hall Gaussian shaping, and a weighted
//! combination of dyadic box smoothings that reproduces the large-to-small
//! scale rollover. Everything here is integer decisions + basic float ops
//! (`+ − * /`, comparisons) on the `f64` dyadic grid — no `sin`/`cos`/
//! `ln`/`exp`/`powf` anywhere in this file, so the field replays
//! bit-identically on every IEEE-754 platform.
//!
//! Stream grammar: `region_seed(master, RegionId { frame: Cosmological,
//! cell: [0, 0, slice] }).layer("cosmic_web/field")`, consumed
//! sequentially with x fastest. Slice streams (not per-cell streams) keep
//! boot-time hashing at 128 domains instead of 2M.

use super::params::CosmicWebParams;
use crate::frames::FrameId;
use crate::seeding::{RegionId, region_seed};

/// Flat index, x fastest: `x + n * (y + n * z)`.
pub fn index(n: usize, x: usize, y: usize, z: usize) -> usize {
    x + n * (y + n * z)
}

/// Irwin–Hall Gaussian: the sum of 12 uniforms has exactly unit variance
/// (12/12), so `sum − 6` is ~N(0, 1) with pure arithmetic — no
/// Box-Muller, no transcendentals.
fn gaussian(rng: &mut crate::core::SeededRng) -> f64 {
    let mut sum = 0.0;
    for _ in 0..12 {
        sum += rng.unit_f64();
    }
    sum - 6.0
}

/// White Gaussian field, one z-slice stream at a time.
fn white_noise(master: u64, n: u32) -> Vec<f64> {
    let n_usize = n as usize;
    let mut field = vec![0.0; n_usize * n_usize * n_usize];
    for z in 0..n {
        let region = RegionId {
            frame: FrameId::Cosmological,
            cell: [0, 0, i64::from(z)],
        };
        let mut rng = region_seed(master, region).layer("cosmic_web/field");
        for y in 0..n_usize {
            for x in 0..n_usize {
                field[index(n_usize, x, y, z as usize)] = gaussian(&mut rng);
            }
        }
    }
    field
}

/// One axis of a separable box blur (sliding window). The lattice box is
/// PERIODIC (cosmology-simulation convention): indices wrap, so there is
/// no edge-effect zone and the descriptor sphere may touch the box faces.
/// Requires `2 * radius + 1 <= len` (window must not overlap itself).
fn blur_axis(field: &[f64], out: &mut [f64], n: usize, axis: usize, radius: usize) {
    let window = 2 * radius + 1;
    let inv = 1.0 / window as f64;
    // Stride layout: axis 0 → stride 1, axis 1 → stride n, axis 2 → stride n².
    let (stride, lines, len) = match axis {
        0 => (1, n * n, n),
        1 => (n, n * n, n),
        _ => (n * n, n * n, n),
    };
    debug_assert!(window <= len, "blur window overlaps itself");
    let wrap = |v: i64| v.rem_euclid(len as i64) as usize;
    for line in 0..lines {
        // Base offset of this line in flat storage.
        let base = match axis {
            0 => (line / n) * n * n + (line % n) * n,
            1 => (line / n) * n * n + (line % n),
            _ => line,
        };
        let mut acc = 0.0;
        for i in 0..window {
            let k = wrap(i as i64 - radius as i64);
            acc += field[base + k * stride];
        }
        for i in 0..len {
            out[base + i * stride] = acc * inv;
            let add = wrap(i as i64 + radius as i64 + 1);
            let sub = wrap(i as i64 - radius as i64);
            acc += field[base + add * stride] - field[base + sub * stride];
        }
    }
}

/// Separable 3-axis box blur approximating Gaussian σ ≈ radius/√3.
fn smooth_into(field: &[f64], work: &mut [f64], out: &mut [f64], n: usize, radius: usize) {
    blur_axis(field, work, n, 0, radius);
    blur_axis(work, out, n, 1, radius);
    blur_axis(out, work, n, 2, radius);
    out.copy_from_slice(work);
}

/// Build the initial field: white noise + weighted dyadic smoothings,
/// standardized to unit variance (so downstream thresholds are
/// scale-independent and calibration-stable).
pub fn initial_field(master: u64, params: &CosmicWebParams) -> Vec<f64> {
    let n = params.lattice_cells as usize;
    let white = white_noise(master, params.lattice_cells);
    let mut combined = vec![0.0; n * n * n];
    let mut work = vec![0.0; n * n * n];
    let mut smoothed = vec![0.0; n * n * n];
    for (scale, weight) in params
        .smooth_sigmas_cells
        .iter()
        .zip(params.smooth_weights.iter())
    {
        // Box radius approximating the Gaussian sigma (σ ≈ r/√3).
        let radius = ((*scale * 1.7320508075688772).round() as usize).max(1);
        smooth_into(&white, &mut work, &mut smoothed, n, radius);
        for (dst, src) in combined.iter_mut().zip(smoothed.iter()) {
            *dst += *weight * *src;
        }
    }
    // Standardize: unit variance, zero mean (basic ops only).
    let count = combined.len() as f64;
    let mean = combined.iter().sum::<f64>() / count;
    let var = combined
        .iter()
        .map(|v| (v - mean) * (v - mean))
        .sum::<f64>()
        / count;
    let inv_std = if var > 0.0 { 1.0 / var.sqrt() } else { 1.0 };
    for v in combined.iter_mut() {
        *v = (*v - mean) * inv_std;
    }
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_params() -> CosmicWebParams {
        CosmicWebParams::new(32, 4.0, 50.0).expect("small test params fit")
    }

    #[test]
    fn field_replays_identically() {
        let p = small_params();
        assert_eq!(initial_field(42, &p), initial_field(42, &p));
    }

    #[test]
    fn field_is_standardized() {
        let p = small_params();
        let field = initial_field(7, &p);
        let count = field.len() as f64;
        let mean = field.iter().sum::<f64>() / count;
        let var = field.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / count;
        assert!(mean.abs() < 1e-9, "mean drifted: {mean}");
        assert!((var - 1.0).abs() < 1e-9, "variance drifted: {var}");
    }

    #[test]
    fn smoothing_kills_small_scale_power() {
        // A white field has ~half its neighbor pairs anti-correlated;
        // the smoothed combination must correlate neighbors positively.
        let p = small_params();
        let field = initial_field(11, &p);
        let n = 32_usize;
        let mut same_sign = 0;
        let mut total = 0;
        for z in 0..n {
            for y in 0..n {
                for x in 0..n - 1 {
                    let a = field[index(n, x, y, z)];
                    let b = field[index(n, x + 1, y, z)];
                    if a * b > 0.0 {
                        same_sign += 1;
                    }
                    total += 1;
                }
            }
        }
        let frac = same_sign as f64 / total as f64;
        assert!(frac > 0.6, "smoothing did not correlate: {frac}");
    }

    #[test]
    fn blur_axis_is_a_moving_average() {
        let n = 8_usize;
        let field: Vec<f64> = (0..n * n * n).map(|i| i as f64).collect();
        let mut out = vec![0.0; n * n * n];
        blur_axis(&field, &mut out, n, 0, 1);
        // Center of the x=3 line on the y=0,z=0 row: mean of {2,3,4} in
        // flat units along stride 1.
        let got = out[index(n, 3, 0, 0)];
        assert!((got - 3.0).abs() < 1e-12, "got {got}");
    }
}
