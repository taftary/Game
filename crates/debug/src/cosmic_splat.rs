//! Cosmic tracer splats: CPU-side derivation (CTS-001/002).
//!
//! Pure function of `(WebField, WebDescriptor, origin, tier)`: stride
//! subset selection, class-B galaxy flag, class-C hub tint. No RNG.

use game_engine::universe::{WebDescriptor, WebField};
use glam::DVec3;
use std::collections::HashMap;

/// Density ramp stops: `log2(1+delta)` -> sRGB-ish color.
pub const DENSITY_RAMP_STOPS: [(f32, [f32; 3]); 5] = [
    (-2.0, [0.10, 0.08, 0.35]),
    (0.0, [0.35, 0.32, 0.80]),
    (1.5, [0.85, 0.85, 1.00]),
    (3.0, [1.00, 0.92, 0.60]),
    (4.5, [1.00, 0.45, 0.40]),
];

/// Render tier selecting the tracer budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplatTier {
    Low,
    Medium,
    High,
}

/// Low-tier splat budget (300k points).
pub const SPLAT_BUDGET_LOW: usize = 300_000;
/// Medium-tier splat budget (1M points).
pub const SPLAT_BUDGET_MEDIUM: usize = 1_000_000;

/// Point budget for a tier; High draws every tracer.
pub fn splat_budget(tier: SplatTier) -> usize {
    match tier {
        SplatTier::Low => SPLAT_BUDGET_LOW,
        SplatTier::Medium => SPLAT_BUDGET_MEDIUM,
        SplatTier::High => usize::MAX,
    }
}

/// One derived splat: origin-relative position, density, packed classes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplatRecord {
    /// Position in Mpc relative to `origin`.
    pub pos: [f32; 3],
    /// Smoothed overdensity `(1+delta)` copied from the tracer.
    pub overdensity: f32,
    /// Bit 0 = class-B core; bits 1-2 = class-C tint level 0..3.
    pub class_tint: u8,
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

/// Emissive boost so dense cores cross the bloom threshold.
pub fn emissive_scale(log2od: f32) -> f32 {
    1.0 + 0.5 * (0.0_f32.max(log2od - 1.5))
}

/// GPU vertex packing (CTS-003): one `u32` beside the position keeps
/// `SplatVertex` at 16 B (Low: 300k × 16 B = 4.8 MB ≤ 5 MB budget).
/// Bits 0–15: `log2(1+δ)` quantized over `[-8, +8)` (step 2.4e-4,
/// invisible); bits 16–17: class-C tint level 0–3; bit 18: class-B
/// galaxy-core flag. The vertex shader unpacks with bit ops (no
/// textures, no transcendentals in the fragment).
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

/// Overdraw estimate (CTS-008): Σ kernel px² over viewport px, i.e.
/// the average stacked-sprite depth. `px_per_mpc` is the inspector
/// `px_scale / depth` at the nominal framing. Logged, not gated —
/// the cut order (kernel 64→32 px, then budget 300k→200k) arms on it.
pub fn overdraw_estimate(records: &[SplatRecord], px_per_mpc: f32, viewport_px: (u32, u32)) -> f64 {
    if records.is_empty() || viewport_px.0 == 0 || viewport_px.1 == 0 {
        return 0.0;
    }
    let stacked: f64 = records
        .iter()
        .map(|r| {
            let px = splat_kernel_px(kernel_radius_mpc(r.overdensity), px_per_mpc);
            f64::from(px * px)
        })
        .sum();
    stacked / (f64::from(viewport_px.0) * f64::from(viewport_px.1))
}

/// Grid cell size (Mpc) for the hub proximity lookup.
const HUB_GRID_MPC: f64 = 60.0;

fn grid_key(p: [f64; 3]) -> [i64; 3] {
    [
        (p[0] / HUB_GRID_MPC).floor() as i64,
        (p[1] / HUB_GRID_MPC).floor() as i64,
        (p[2] / HUB_GRID_MPC).floor() as i64,
    ]
}

/// Derive splats: deterministic stride subset + B flag + C tint.
pub fn splat_records(
    field: &WebField,
    web: &WebDescriptor,
    origin: DVec3,
    tier: SplatTier,
) -> Vec<SplatRecord> {
    let n = field.tracers.len();
    if n == 0 {
        return Vec::new();
    }
    let stride = n.div_ceil(splat_budget(tier)).max(1);

    // Top 1% nodes by rank (descriptor order is mass rank).
    let hub_count = ((web.nodes.len() as f64 * 0.01).ceil() as usize).min(web.nodes.len());
    let hubs = &web.nodes[..hub_count];

    // Spatial grid over hubs (60 Mpc cells) for the proximity test.
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (hi, node) in hubs.iter().enumerate() {
        grid.entry(grid_key(node.position_mpc))
            .or_default()
            .push(hi);
    }
    let max_reach = hubs
        .iter()
        .map(|h| 1.5 * h.virial_radius_mpc)
        .fold(0.0_f64, f64::max);
    let reach = (max_reach / HUB_GRID_MPC).ceil() as i64;

    let mut out = Vec::new();
    for (i, t) in field.tracers.iter().enumerate() {
        if i % stride != 0 {
            continue;
        }
        let mut class_tint: u8 = 0;
        if t.overdensity >= 3.0 {
            class_tint |= 1;
        }
        // C tint: deepest enclosing hub wins (level 1..3 by proximity).
        if !hubs.is_empty() && max_reach > 0.0 {
            let p = [
                f64::from(t.pos_mpc[0]),
                f64::from(t.pos_mpc[1]),
                f64::from(t.pos_mpc[2]),
            ];
            let k = grid_key(p);
            let mut level = 0u8;
            for dx in -reach..=reach {
                for dy in -reach..=reach {
                    for dz in -reach..=reach {
                        if let Some(cell) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                            for &hi in cell {
                                let hub = &hubs[hi];
                                let r = 1.5 * hub.virial_radius_mpc;
                                if r <= 0.0 {
                                    continue;
                                }
                                let q = hub.position_mpc;
                                let d2 = (p[0] - q[0]).powi(2)
                                    + (p[1] - q[1]).powi(2)
                                    + (p[2] - q[2]).powi(2);
                                if d2 <= r * r {
                                    let frac = d2.sqrt() / r;
                                    let lv = if frac <= 1.0 / 3.0 {
                                        3
                                    } else if frac <= 2.0 / 3.0 {
                                        2
                                    } else {
                                        1
                                    };
                                    level = level.max(lv);
                                }
                            }
                        }
                    }
                }
            }
            class_tint |= level << 1;
        }
        out.push(SplatRecord {
            pos: [
                (f64::from(t.pos_mpc[0]) - origin.x) as f32,
                (f64::from(t.pos_mpc[1]) - origin.y) as f32,
                (f64::from(t.pos_mpc[2]) - origin.z) as f32,
            ],
            overdensity: t.overdensity,
            class_tint,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{WebField, WebLink, WebNode, WebTracer};

    fn synth_field(n: usize) -> WebField {
        let tracers = (0..n)
            .map(|i| WebTracer {
                pos_mpc: [i as f32, 0.0, 0.0],
                overdensity: 1.0 + (i % 5) as f32,
            })
            .collect();
        WebField {
            tracers,
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 1.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 1.0e9,
        }
    }

    fn synth_web(nodes: Vec<WebNode>) -> WebDescriptor {
        WebDescriptor::new(7, nodes, Vec::<WebLink>::new(), Vec::new(), 0, 0.5)
    }

    fn hub_node(pos: [f64; 3], r_vir: f64) -> WebNode {
        WebNode {
            node_index: 0,
            position_mpc: pos,
            mass_msun: 1.0e14,
            virial_radius_mpc: r_vir,
        }
    }

    #[test]
    fn stride_subset_low_in_medium() {
        let field = synth_field(500);
        let web = synth_web(Vec::new());
        let origin = DVec3::ZERO;
        let low = splat_records(&field, &web, origin, SplatTier::Low);
        let med = splat_records(&field, &web, origin, SplatTier::Medium);
        // Budgets exceed n here, so stride is 1 and both keep everything.
        assert_eq!(low.len(), 500);
        assert_eq!(med.len(), 500);
        // Force a real stride with small budgets via direct stride logic.
        let stride_low = 500_usize.div_ceil(SPLAT_BUDGET_LOW).max(1);
        let stride_med = 500_usize.div_ceil(SPLAT_BUDGET_MEDIUM).max(1);
        assert_eq!((stride_low, stride_med), (1, 1));

        // Larger web where stride > 1: Low stride is a multiple of Medium.
        let big = synth_field(2_500_000);
        let low = splat_records(&big, &web, origin, SplatTier::Low);
        let med = splat_records(&big, &web, origin, SplatTier::Medium);
        assert_eq!(low.len(), 2_500_000 / 9 + 1); // ceil stride 9
        assert_eq!(med.len(), 2_500_000 / 3 + 1); // ceil stride 3
        let med_pos: std::collections::HashSet<[u32; 3]> = med
            .iter()
            .map(|r| [r.pos[0].to_bits(), r.pos[1].to_bits(), r.pos[2].to_bits()])
            .collect();
        for r in &low {
            let k = [r.pos[0].to_bits(), r.pos[1].to_bits(), r.pos[2].to_bits()];
            assert!(med_pos.contains(&k), "low record missing from medium set");
        }
    }

    #[test]
    fn b_flag_threshold() {
        let field = WebField {
            tracers: vec![
                WebTracer {
                    pos_mpc: [0.0; 3],
                    overdensity: 2.99,
                },
                WebTracer {
                    pos_mpc: [1.0, 0.0, 0.0],
                    overdensity: 3.0,
                },
                WebTracer {
                    pos_mpc: [2.0, 0.0, 0.0],
                    overdensity: 10.0,
                },
            ],
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 1.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 1.0e9,
        };
        let web = synth_web(Vec::new());
        let out = splat_records(&field, &web, DVec3::ZERO, SplatTier::High);
        assert_eq!(out[0].class_tint & 1, 0);
        assert_eq!(out[1].class_tint & 1, 1);
        assert_eq!(out[2].class_tint & 1, 1);
    }

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
        // Emissive scale pins.
        assert_eq!(emissive_scale(0.0), 1.0);
        assert_eq!(emissive_scale(1.5), 1.0);
        assert!((emissive_scale(3.0) - 1.75).abs() < 1e-6);
    }

    #[test]
    fn c_tint_only_near_top_percent_hubs() {
        // 200 nodes -> top 1% = first 2 nodes.
        let mut nodes: Vec<WebNode> = (0..200)
            .map(|i| WebNode {
                node_index: i,
                position_mpc: [1000.0 + i as f64 * 10.0, 0.0, 0.0],
                mass_msun: 1.0e12,
                virial_radius_mpc: 2.0,
            })
            .collect();
        nodes[0] = hub_node([0.0, 0.0, 0.0], 2.0);
        nodes[1] = hub_node([50.0, 0.0, 0.0], 2.0);
        // A non-hub node with a huge radius must not tint.
        nodes[100] = hub_node([500.0, 0.0, 0.0], 100.0);
        let web = synth_web(nodes);
        let field = WebField {
            tracers: vec![
                WebTracer {
                    pos_mpc: [0.0, 0.0, 0.0],
                    overdensity: 1.0,
                }, // deep: 3
                WebTracer {
                    pos_mpc: [1.5, 0.0, 0.0],
                    overdensity: 1.0,
                }, // mid: 2
                WebTracer {
                    pos_mpc: [2.9, 0.0, 0.0],
                    overdensity: 1.0,
                }, // edge: 1
                WebTracer {
                    pos_mpc: [3.1, 0.0, 0.0],
                    overdensity: 1.0,
                }, // out: 0
                WebTracer {
                    pos_mpc: [500.0, 0.0, 0.0],
                    overdensity: 1.0,
                }, // non-hub: 0
                WebTracer {
                    pos_mpc: [49.0, 0.0, 0.0],
                    overdensity: 1.0,
                }, // hub 2: tinted
            ],
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 1.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 1.0e9,
        };
        let out = splat_records(&field, &web, DVec3::ZERO, SplatTier::High);
        assert_eq!(out[0].class_tint >> 1, 3);
        assert_eq!(out[1].class_tint >> 1, 2);
        assert_eq!(out[2].class_tint >> 1, 1);
        assert_eq!(out[3].class_tint >> 1, 0);
        assert_eq!(out[4].class_tint >> 1, 0, "non-top-1% hub must not tint");
        assert!(out[5].class_tint >> 1 >= 1);
    }

    #[test]
    fn replay_identity() {
        let field = synth_field(1000);
        let web = synth_web(vec![hub_node([10.0, 0.0, 0.0], 2.0)]);
        let origin = DVec3::new(1.0, 2.0, 3.0);
        let a = splat_records(&field, &web, origin, SplatTier::Low);
        let b = splat_records(&field, &web, origin, SplatTier::Low);
        assert_eq!(a, b);
    }
    #[test]
    fn translation_invariance_full() {
        let field = synth_field(200);
        let web = synth_web(vec![hub_node([10.0, 0.0, 0.0], 2.0)]);
        let o0 = DVec3::ZERO;
        let o1 = DVec3::new(5.0, -3.0, 2.0);
        let a = splat_records(&field, &web, o0, SplatTier::Medium);
        let b = splat_records(&field, &web, o1, SplatTier::Medium);
        assert_eq!(a.len(), b.len());
        for (r0, r1) in a.iter().zip(b.iter()) {
            assert_eq!(r0.overdensity, r1.overdensity);
            assert_eq!(r0.class_tint, r1.class_tint);
            for (axis, shift) in [o1.x - o0.x, o1.y - o0.y, o1.z - o0.z].iter().enumerate() {
                assert!(
                    (r0.pos[axis] - r1.pos[axis] - *shift as f32).abs() < 1e-4,
                    "pos shifted incorrectly on axis {axis}"
                );
            }
        }
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
    fn overdraw_estimate_scales_with_kernel_area() {
        let one = |od: f32| SplatRecord {
            pos: [0.0; 3],
            overdensity: od,
            class_tint: 0,
        };
        assert_eq!(overdraw_estimate(&[], 10.0, (1920, 1080)), 0.0);
        // Dense splats (small kernels) stack less than void splats (big).
        let dense = vec![one(100.0); 1000];
        let void = vec![one(0.01); 1000];
        let area = 1920 * 1080;
        assert!(
            overdraw_estimate(&dense, 10.0, (1920, 1080))
                < overdraw_estimate(&void, 10.0, (1920, 1080))
        );
        // Single void splat at 10 px/Mpc: h=4 Mpc → 40 px → 1600/area.
        let single = overdraw_estimate(&void[..1], 10.0, (1920, 1080));
        assert!((single - 1600.0 / area as f64).abs() < 1e-9);
    }
}
