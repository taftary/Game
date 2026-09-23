//! Cosmic tracer splats: procedural-draw helpers (CGT-005/006).
//!
//! The draw is `gl_VertexIndex`-driven (`SPLAT_PROC_VERT` in the binary):
//! a Lagrangian cell list + fixed sub-cell offsets + the displacement
//! grid behind it. This module holds the tier draw counts
//! ([`SplatK`]), the offset table ([`splat_sub_offsets`]), and the CPU
//! mirrors of the shader math (ramp, kernel, packing). No RNG, no
//! per-travel CPU work. The v0.3.3 CPU record path (stride budgets
//! over stored vertices) retired in CGT-010.

/// Density ramp stops: `log2(1+delta)` -> sRGB-ish color.
pub const DENSITY_RAMP_STOPS: [(f32, [f32; 3]); 5] = [
    (-2.0, [0.10, 0.08, 0.35]),
    (0.0, [0.35, 0.32, 0.80]),
    (1.5, [0.85, 0.85, 1.00]),
    (3.0, [1.00, 0.92, 0.60]),
    (4.5, [1.00, 0.45, 0.40]),
];

/// Render tier selecting the sub-sample draw count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplatTier {
    Low,
    Medium,
    High,
}

/// Adaptive kernel radius `h = h0 * (1+d)^(-1/3)`, clamped to [0.5, 4.0].
pub fn kernel_radius_mpc(overdensity: f32) -> f32 {
    let h = 2.0 * (-(overdensity.max(1e-3)).log2() / 3.0).exp2();
    h.clamp(0.5, 4.0)
}

/// Linear mix between [`DENSITY_RAMP_STOPS`], clamped to end stops.
pub fn ramp_color(log2od: f32) -> [f32; 3] {
    let stops = DENSITY_RAMP_STOPS;
    if log2od <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let (x0, c0) = w[0];
        let (x1, c1) = w[1];
        if log2od <= x1 {
            let t = (log2od - x0) / (x1 - x0);
            return [
                c0[0] + (c1[0] - c0[0]) * t,
                c0[1] + (c1[1] - c0[1]) * t,
                c0[2] + (c1[2] - c0[2]) * t,
            ];
        }
    }
    stops[stops.len() - 1].1
}

// Retired by `cosmic-void-contrast` (CVC-002): brightness is the
// transfer weight now (`cosmic_veil::transfer`), not a per-density
// growth factor (`emissive_scale` removed — the transfer tests pin
// the replacement).

/// Retired vertex packing (CTS-003) kept as the tested quantization
/// mirror: bits 0–15 hold `log2(1+δ)` over `[-8, +8)` (step 2.4e-4),
/// bits 16–17 a 2-bit tint, bit 18 a flag. The 16 B vertex stream
/// that consumed it retired in CGT-010.
pub fn splat_pack(log2od: f32, tint: u8, b: bool) -> u32 {
    let q = ((log2od.clamp(-8.0, 8.0 - 1e-3) + 8.0) / 16.0 * 65535.0).round() as u32;
    q | ((u32::from(tint) & 3) << 16) | ((u32::from(b)) << 18)
}

/// Inverse of [`splat_pack`] (CPU mirror of the vertex unpack).
pub fn splat_unpack(packed: u32) -> (f32, u8, bool) {
    let log2od = (packed & 0xFFFF) as f32 / 65535.0 * 16.0 - 8.0;
    let tint = ((packed >> 16) & 3) as u8;
    let b = (packed >> 18) & 1 == 1;
    (log2od, tint, b)
}

/// Projected kernel size in px, clamped to `[1.5, 64]` (CPU mirror of
/// the vertex shader clamp).
pub fn splat_kernel_px(h_mpc: f32, px_per_mpc: f32) -> f32 {
    (h_mpc * px_per_mpc).clamp(1.5, 64.0)
}

/// Sub-samples per Lagrangian cell for the procedural GPU tracer draw
/// (v0.3.4 `cosmic-gpu-tracers`, ADR-026 §2): a draw count per tier,
/// not a precompute. Low keeps today's invocation count (1 per cell);
/// High draws 8 (the retired `refine(2)` density, with none of its
/// 134 MB vertex cost).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplatK(pub u8);

impl SplatK {
    /// Tier draw counts: Low 1 / Medium 2 / High 8 (graded CGT-008/009:
    /// k8/k1 slab mean ratio 1.20, parity dice 0.81; `cells_ms` 46.6 ms
    /// release — no cut, px-clamp 64→32 armed first).
    pub fn for_tier(tier: SplatTier) -> Self {
        Self(match tier {
            SplatTier::Low => 1,
            SplatTier::Medium => 2,
            SplatTier::High => 8,
        })
    }

    /// Strict `1..=8` parse of `GAME_DEBUG_COSMIC_K` (the
    /// `GAME_DEBUG_COSMIC_VEIL` precedent: garbage never silently
    /// regrades a capture).
    pub fn parse_override(s: &str) -> Result<Self, String> {
        match s.parse::<u8>() {
            Ok(k) if (1..=8).contains(&k) => Ok(Self(k)),
            _ => Err(format!("bad sub-sample count {s:?}: want an integer 1..=8")),
        }
    }
}

/// Fixed sub-cell offsets for a draw count `k` (CGT-005): the retired
/// `WebField::refine()` pattern (`0.25 + 0.5·s`) generalized. `k = 1`
/// is the cell centre; otherwise the first `k` corners of the 8
/// (x fastest). Deterministic; the `SPLAT_PROC_VERT` table in the
/// binary mirrors this order bit-for-bit (pinned by the
/// `splat_proc_offset_table` string test there).
pub fn splat_sub_offsets(k: u8) -> Vec<[f32; 3]> {
    let k = k.clamp(1, 8) as usize;
    if k == 1 {
        return vec![[0.5, 0.5, 0.5]];
    }
    let mut out = Vec::with_capacity(k);
    for s in 0..8 {
        if out.len() == k {
            break;
        }
        out.push([
            0.25 + 0.5 * ((s & 1) as f32),
            0.25 + 0.5 * (((s >> 1) & 1) as f32),
            0.25 + 0.5 * (((s >> 2) & 1) as f32),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_monotone_and_clamped() {
        assert_eq!(kernel_radius_mpc(0.0), 4.0);
        assert_eq!(kernel_radius_mpc(1e9), 0.5);
        let mut prev = f32::INFINITY;
        for od in [0.5, 1.0, 2.0, 3.0, 8.0, 32.0] {
            let h = kernel_radius_mpc(od);
            assert!(h < prev, "kernel not decreasing at od={od}: {h} >= {prev}");
            assert!((0.5..=4.0).contains(&h));
            prev = h;
        }
        // h0 at od=1 is exactly 2.0.
        assert!((kernel_radius_mpc(1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn ramp_anchors_and_mid_band() {
        for (x, c) in DENSITY_RAMP_STOPS {
            assert_eq!(ramp_color(x), c, "ramp not exact at anchor {x}");
        }
        assert_eq!(ramp_color(-100.0), DENSITY_RAMP_STOPS[0].1);
        assert_eq!(ramp_color(100.0), DENSITY_RAMP_STOPS[4].1);
        // Mid between stops 1 and 2 (0.75) is the pairwise average.
        let mid = ramp_color(0.75);
        for (a, m) in mid.iter().enumerate() {
            let want = (DENSITY_RAMP_STOPS[1].1[a] + DENSITY_RAMP_STOPS[2].1[a]) * 0.5;
            assert!((m - want).abs() < 1e-6, "mid band off axis {a}");
        }
        // (`cosmic-void-contrast` retired `emissive_scale` here: the
        // transfer weight in `cosmic_veil::transfer` carries brightness
        // now — no per-density growth factor remains.)
    }

    #[test]
    fn pack_round_trips_within_half_step() {
        for (log2od, tint, b) in [
            (-2.0, 0, false),
            (0.0, 2, true),
            (4.5, 3, true),
            (1.5, 1, false),
        ] {
            let (back, tint_back, b_back) = splat_unpack(splat_pack(log2od, tint, b));
            assert!(
                (back - log2od).abs() <= 16.0 / 65535.0,
                "quant step too big"
            );
            assert_eq!(tint_back, tint);
            assert_eq!(b_back, b);
        }
        // Clamp rails.
        assert_eq!(splat_unpack(splat_pack(-100.0, 0, false)).0, -8.0);
        assert!(splat_unpack(splat_pack(100.0, 0, false)).0 <= 8.0);
    }

    #[test]
    fn splat_k_tier_mapping() {
        // CGT-006: Low 1 / Medium 2 / High 8 (Low keeps today's count).
        assert_eq!(SplatK::for_tier(SplatTier::Low), SplatK(1));
        assert_eq!(SplatK::for_tier(SplatTier::Medium), SplatK(2));
        assert_eq!(SplatK::for_tier(SplatTier::High), SplatK(8));
    }

    #[test]
    fn splat_k_parse_strict() {
        // CGT-006: strict 1..=8, garbage errors (never a silent grade).
        for good in ["1", "2", "8"] {
            assert!(SplatK::parse_override(good).is_ok(), "{good} must parse");
        }
        for bad in ["0", "9", "100", "", "high", "4.0", "-1", " 4"] {
            assert!(
                SplatK::parse_override(bad).is_err(),
                "{bad:?} must not parse"
            );
        }
        assert_eq!(SplatK::parse_override("4"), Ok(SplatK(4)));
    }

    #[test]
    fn splat_sub_offsets_shape() {
        // CGT-006: k=1 is the centre; k=8 is the refine() pattern;
        // other k are its prefix (x fastest).
        assert_eq!(splat_sub_offsets(1), vec![[0.5, 0.5, 0.5]]);
        let eight = splat_sub_offsets(8);
        assert_eq!(eight.len(), 8);
        assert_eq!(eight[0], [0.25, 0.25, 0.25]);
        assert_eq!(eight[1], [0.75, 0.25, 0.25]);
        assert_eq!(eight[7], [0.75, 0.75, 0.75]);
        assert_eq!(&splat_sub_offsets(2), &eight[..2]);
        assert_eq!(&splat_sub_offsets(4), &eight[..4]);
        // Clamp rails (never 0 verts, never > 8).
        assert_eq!(splat_sub_offsets(0).len(), 1);
        assert_eq!(splat_sub_offsets(255).len(), 8);
    }
}
