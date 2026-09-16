//! Quantization: snap floats to an integer grid before hashing,
//! storing, or cross-platform comparison (risks #2).
//!
//! Integer-driven topology plus fixed iteration counts already keep the
//! hexsphere bit-identical across platforms; these helpers do the same
//! job for continuous outputs. [`quantize_f64`] returns
//! `round(value * quantum)` as `i64`: two inputs within half a quantum
//! map to the same integer, so 1-ulp drift between ARM and x86 cannot
//! flip a descriptor hash. The quantum is unit-dependent — each
//! generator picks its own and documents it (e.g. micro-light-years for
//! galactic positions).
//!
//! `round` ties away from zero and is a single IEEE op, so it is exact
//! and identical on every platform. Inputs must be finite: generation
//! code must never emit NaN/inf (debug-asserted).
//!
//! ```
//! use game_engine::core::{quantize_f32, quantize_f64};
//!
//! // 1-ulp-apart inputs quantize equal at a sane quantum…
//! assert_eq!(quantize_f64(1.0, 1_000_000.0), quantize_f64(1.0 + f64::EPSILON, 1_000_000.0));
//! // …while genuinely different values stay apart.
//! assert_ne!(quantize_f64(1.0, 1_000_000.0), quantize_f64(1.5, 1_000_000.0));
//! assert_eq!(quantize_f32(-2.5, 10.0), -25);
//! ```

/// Snap an `f64` to the `1/quantum` grid, returned as `i64`.
///
/// # Panics
///
/// Debug-asserts finite `value` and a finite positive `quantum`.
pub fn quantize_f64(value: f64, quantum: f64) -> i64 {
    debug_assert!(value.is_finite(), "cannot quantize NaN/inf");
    debug_assert!(quantum.is_finite() && quantum > 0.0);
    (value * quantum).round() as i64
}

/// Snap an `f32` to the `1/quantum` grid, returned as `i32`.
///
/// # Panics
///
/// Debug-asserts finite `value` and a finite positive `quantum`.
pub fn quantize_f32(value: f32, quantum: f32) -> i32 {
    debug_assert!(value.is_finite(), "cannot quantize NaN/inf");
    debug_assert!(quantum.is_finite() && quantum > 0.0);
    (value * quantum).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulp_drift_cannot_flip_the_grid() {
        let quantum = 1_000_000.0;
        assert_eq!(
            quantize_f64(12_345.678, quantum),
            quantize_f64(12_345.678 + f64::EPSILON * 8.0, quantum)
        );
    }

    #[test]
    fn grid_values_are_exact() {
        assert_eq!(quantize_f64(2.5, 4.0), 10);
        assert_eq!(quantize_f64(-2.5, 4.0), -10);
        assert_eq!(quantize_f32(0.0, 100.0), 0);
    }

    #[test]
    fn galactic_scale_fits_i64() {
        // 100k ly at micro-ly quantum — far below i64::MAX.
        assert_eq!(quantize_f64(100_000.0, 1_000_000.0), 100_000_000_000);
    }
}
