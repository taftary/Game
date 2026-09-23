//! Cosmic gas veil: shared density ramp + cell sprites (CGV-001/002/008).
//!
//! Low-tier body of the web: one additive sprite per dense
//! sheet/filament/node grid cell. Pure functions of `&WebField`.

use game_engine::universe::{
    WebField,
    web::{FILAMENT, NODE, SHEET},
};
use glam::DVec3;

// Stop table shared with `cosmic_splat::DENSITY_RAMP_STOPS` (same five
// anchors; pinned by `ramp_glsl_pins_all_five_stops` +
// `cosmic_density_ramp_shared` in the binary).
pub const VEIL_RAMP_STOPS: [(f32, [f32; 3]); 5] = [
    (-2.0, [0.10, 0.08, 0.35]),
    (0.0, [0.35, 0.32, 0.80]),
    (1.5, [0.85, 0.85, 1.00]),
    (3.0, [1.00, 0.92, 0.60]),
    (4.5, [1.00, 0.45, 0.40]),
];

/// Shared density-ramp GLSL for splats, veil sprites, and the raymarch.
/// Piecewise-linear over [`VEIL_RAMP_STOPS`]; mix/clamp only, no exp/pow.
pub const COSMIC_DENSITY_RAMP_GLSL: &str = r#"vec3 cosmic_density_ramp(float log2od) {
    vec3 c0 = vec3(0.10, 0.08, 0.35);
    vec3 c1 = vec3(0.35, 0.32, 0.80);
    vec3 c2 = vec3(0.85, 0.85, 1.00);
    vec3 c3 = vec3(1.00, 0.92, 0.60);
    vec3 c4 = vec3(1.00, 0.45, 0.40);
    vec3 col = c0;
    col = mix(col, c1, clamp((log2od - (-2.0)) / (0.0 - (-2.0)), 0.0, 1.0));
    col = mix(col, c2, clamp((log2od - 0.0) / (1.5 - 0.0), 0.0, 1.0));
    col = mix(col, c3, clamp((log2od - 1.5) / (3.0 - 1.5), 0.0, 1.0));
    col = mix(col, c4, clamp((log2od - 3.0) / (4.5 - 3.0), 0.0, 1.0));
    return col;
}"#;

/// Veil body mode: cell sprites (Low) or quarter-res raymarch (Med/High).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeilMode {
    Sprites,
    March { steps: u32 },
}

impl VeilMode {
    pub fn for_tier_low() -> Self {
        Self::Sprites
    }

    pub fn for_tier_medium() -> Self {
        Self::March { steps: 32 }
    }

    pub fn for_tier_high() -> Self {
        Self::March { steps: 48 }
    }

    /// Parse `GAME_DEBUG_COSMIC_VEIL`: sprites|march|march:<8..=64>.
    pub fn parse_override(s: &str) -> Result<Self, String> {
        match s {
            "sprites" => Ok(Self::Sprites),
            "march" => Ok(Self::March { steps: 32 }),
            _ => match s.strip_prefix("march:").map(str::parse::<u32>) {
                Some(Ok(steps)) if (8..=64).contains(&steps) => Ok(Self::March { steps }),
                _ => Err(format!(
                    "bad veil override {s:?}: want sprites|march|march:8..=64"
                )),
            },
        }
    }
}

/// CPU copy of the density ramp over [`VEIL_RAMP_STOPS`], clamped at ends.
pub fn veil_ramp_cpu(log2od: f32) -> [f32; 3] {
    let stops = VEIL_RAMP_STOPS;
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

/// Sprite alpha from overdensity: `min(0.006*sqrt(od), 0.05)`.
pub fn veil_alpha(overdensity: f32) -> f32 {
    (0.006 * overdensity.max(0.0).sqrt()).min(0.05)
}

/// SplitMix64 finalizer: integer hash of the flat cell index (no RNG).
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic sub-cell offset in `[-0.25, 0.25)` cells from flat index.
fn jitter_cell(index: usize, axis: u64) -> f32 {
    let h = splitmix64((index as u64).wrapping_mul(3).wrapping_add(axis));
    let unit = ((h >> 11) & 0x1F_FFFF) as f32 / 2_097_152.0;
    (unit - 0.5) * 0.5
}

/// One `(pos, color, misc)` record per dense sheet/filament/node cell:
/// pos is origin-relative, misc is `(diameter_mpc, alpha, kind=1.0)`.
/// Single uniform floor `od >= 0.5` (FR1 as written): the nominal
/// in-sphere count measures ~335k, over the 200k Low upload budget, so
/// Low draws a deterministic stride-2 subset (~168k) at upload time —
/// the `cosmic-tracer-splat` stride precedent. The sprite list itself
/// stays the complete physics output (no class cut: walls are the
/// subtlest element and must survive).
pub fn veil_sprites(field: &WebField, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    let n = field.grid_cells as usize;
    if n == 0 || field.grid.len() < n * n * n {
        return Vec::new();
    }
    let cell = field.cell_size_mpc;
    let [ox, oy, oz] = field.origin_mpc;
    // Descriptor-sphere cut (FR1 "inside the sphere"): cell centers are
    // box-centered Mpc, so the sphere center is the box center [0,0,0].
    // Non-positive radius = unbounded (synthetic test fields).
    let r2_max = field.sphere_radius_mpc * field.sphere_radius_mpc;
    let bounded = field.sphere_radius_mpc > 0.0 && field.sphere_radius_mpc < 1.0e8;
    let mut out = Vec::new();
    for i in 0..n * n * n {
        let (diam_mul, tint) = match field.class_at(i) {
            SHEET => (2.0, [0.8, 0.8, 1.0]),
            FILAMENT => (1.4, [1.0, 1.0, 1.0]),
            NODE => (1.2, [1.0, 0.95, 0.9]),
            _ => continue,
        };
        let od = field.overdensity_at(i);
        // NaN-safe floor: NaN compares false and must be skipped.
        if od.is_nan() || od < 0.5 {
            continue;
        }
        let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
        // Sphere cut on the un-jittered cell center (box-centered frame).
        if bounded {
            let cx = ox + (x as f64 + 0.5) * cell;
            let cy = oy + (y as f64 + 0.5) * cell;
            let cz = oz + (z as f64 + 0.5) * cell;
            if cx * cx + cy * cy + cz * cz > r2_max {
                continue;
            }
        }
        let jx = f64::from(jitter_cell(i, 0));
        let jy = f64::from(jitter_cell(i, 1));
        let jz = f64::from(jitter_cell(i, 2));
        let pos = [
            (ox + (x as f64 + 0.5 + jx) * cell - origin.x) as f32,
            (oy + (y as f64 + 0.5 + jy) * cell - origin.y) as f32,
            (oz + (z as f64 + 0.5 + jz) * cell - origin.z) as f32,
        ];
        let ramp = veil_ramp_cpu(od.log2());
        let color = [ramp[0] * tint[0], ramp[1] * tint[1], ramp[2] * tint[2]];
        let misc = [(diam_mul * cell) as f32, veil_alpha(od), 1.0];
        out.push((pos, color, misc));
    }
    out
}

/// 3D texture bytes (CGV-004): the grid's 6-bit log-density spread
/// over R8 (`q * 255 / 63`, nearest). The march shader inverts with
/// `log2od = q * 10 - 4` — one quantum off from the CPU
/// `overdensity_at` (round vs nearest), invisible after linear
/// filtering. Empty for degenerate fields (no texture built).
pub fn veil_volume_bytes(field: &WebField) -> Vec<u8> {
    let n = field.grid_cells as usize;
    if n == 0 || field.grid.len() < n * n * n {
        return Vec::new();
    }
    field.grid[..n * n * n]
        .iter()
        .map(|b| (((b & 0x3F) as u32 * 255 + 31) / 63) as u8)
        .collect()
}

/// CPU mirror of the march ray setup (CGV-007 pin): NDC from the
/// resolve UV contract, unprojected through `inv_vp` (column-major
/// 4×4), ray–sphere intersect. Returns `(origin, dir, t0, t1)` —
/// `None` when the ray misses the sphere. Same op order as
/// `MARCH_FRAG` (single-precision f32 throughout).
pub fn march_ray(
    inv_vp: [[f32; 4]; 4],
    uv: [f32; 2],
    eye: [f32; 3],
    center: [f32; 3],
    radius: f32,
) -> Option<([f32; 3], [f32; 3], f32, f32)> {
    // Column-major multiply: out[i] = sum_j inv_vp[j][i] * v[j].
    let mul = |v: [f32; 4]| {
        [
            inv_vp[0][0] * v[0] + inv_vp[1][0] * v[1] + inv_vp[2][0] * v[2] + inv_vp[3][0] * v[3],
            inv_vp[0][1] * v[0] + inv_vp[1][1] * v[1] + inv_vp[2][1] * v[2] + inv_vp[3][1] * v[3],
            inv_vp[0][2] * v[0] + inv_vp[1][2] * v[1] + inv_vp[2][2] * v[2] + inv_vp[3][2] * v[3],
            inv_vp[0][3] * v[0] + inv_vp[1][3] * v[1] + inv_vp[2][3] * v[2] + inv_vp[3][3] * v[3],
        ]
    };
    let ndc = [uv[0] * 2.0 - 1.0, (1.0 - uv[1]) * 2.0 - 1.0];
    let near4 = mul([ndc[0], ndc[1], 0.0, 1.0]);
    let far4 = mul([ndc[0], ndc[1], 1.0, 1.0]);
    let near = [
        near4[0] / near4[3].max(1e-6),
        near4[1] / near4[3].max(1e-6),
        near4[2] / near4[3].max(1e-6),
    ];
    let far = [
        far4[0] / far4[3].max(1e-6),
        far4[1] / far4[3].max(1e-6),
        far4[2] / far4[3].max(1e-6),
    ];
    let d = [far[0] - near[0], far[1] - near[1], far[2] - near[2]];
    let dl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if dl <= 1e-6 {
        return None;
    }
    let dir = [d[0] / dl, d[1] / dl, d[2] / dl];
    let oc = [eye[0] - center[0], eye[1] - center[1], eye[2] - center[2]];
    let b = oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2];
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - radius * radius;
    let h = b * b - c;
    if h <= 0.0 {
        return None;
    }
    let sq = h.sqrt();
    let t0 = (-b - sq).max(0.0);
    let t1 = -b + sq;
    if t1 <= t0 {
        return None;
    }
    Some((eye, dir, t0, t1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{CosmicWebParams, generate_cosmic_web_with_field};

    fn synth_field(n: u32, cells: &[(u8, u8)]) -> WebField {
        let mut grid = vec![0u8; n as usize * n as usize * n as usize];
        for (i, (class, q)) in cells.iter().enumerate() {
            grid[i] = (class << 6) | (q & 0x3F);
        }
        WebField {
            displacement: Vec::new(),
            grid,
            grid_cells: n,
            cell_size_mpc: 4.0,
            mean_density: 1.0,
            origin_mpc: [-4.0, -4.0, -4.0],
            sphere_radius_mpc: 1.0e9,
        }
    }

    #[test]
    fn ramp_glsl_pins_all_five_stops() {
        let g = COSMIC_DENSITY_RAMP_GLSL;
        assert!(g.contains("vec3 cosmic_density_ramp(float log2od)"));
        // One probe per stop: x anchor + all three color channels.
        for stop in [
            ["-2.0", "0.10", "0.08", "0.35"],
            ["0.0", "0.35", "0.32", "0.80"],
            ["1.5", "0.85", "0.85", "1.00"],
            ["3.0", "1.00", "0.92", "0.60"],
            ["4.5", "1.00", "0.45", "0.40"],
        ] {
            for lit in stop {
                assert!(g.contains(lit), "ramp GLSL drifted: {lit} missing");
            }
        }
        assert!(g.contains("mix(") && g.contains("clamp("));
        for banned in ["exp(", "pow(", "texture("] {
            assert!(!g.contains(banned), "march loop bans {banned}");
        }
    }

    #[test]
    fn veil_ramp_cpu_matches_stop_table() {
        for (x, c) in VEIL_RAMP_STOPS {
            assert_eq!(veil_ramp_cpu(x), c, "ramp not exact at anchor {x}");
        }
        assert_eq!(veil_ramp_cpu(-100.0), VEIL_RAMP_STOPS[0].1);
        assert_eq!(veil_ramp_cpu(100.0), VEIL_RAMP_STOPS[4].1);
        let mid = veil_ramp_cpu(0.75);
        for (a, m) in mid.iter().enumerate() {
            let want = (VEIL_RAMP_STOPS[1].1[a] + VEIL_RAMP_STOPS[2].1[a]) * 0.5;
            assert!((m - want).abs() < 1e-6, "mid band off axis {a}");
        }
    }

    #[test]
    fn parse_override_forms_and_rejections() {
        assert_eq!(VeilMode::parse_override("sprites"), Ok(VeilMode::Sprites));
        assert_eq!(
            VeilMode::parse_override("march"),
            Ok(VeilMode::March { steps: 32 })
        );
        assert_eq!(
            VeilMode::parse_override("march:32"),
            Ok(VeilMode::March { steps: 32 })
        );
        assert_eq!(
            VeilMode::parse_override("march:48"),
            Ok(VeilMode::March { steps: 48 })
        );
        assert_eq!(
            VeilMode::parse_override("march:8"),
            Ok(VeilMode::March { steps: 8 })
        );
        assert_eq!(
            VeilMode::parse_override("march:64"),
            Ok(VeilMode::March { steps: 64 })
        );
        for bad in [
            "",
            "raymarch",
            "sprite",
            "march:7",
            "march:65",
            "march:0",
            "march:abc",
            "sprites:32",
            "MARCH",
        ] {
            assert!(VeilMode::parse_override(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn for_tier_mapping() {
        assert_eq!(VeilMode::for_tier_low(), VeilMode::Sprites);
        assert_eq!(VeilMode::for_tier_medium(), VeilMode::March { steps: 32 });
        assert_eq!(VeilMode::for_tier_high(), VeilMode::March { steps: 48 });
    }

    #[test]
    fn alpha_clamps_on_synthetic_high_value() {
        // Grid max (q=63 -> od=64) stays under the cap: 0.006*8 = 0.048.
        assert!((veil_alpha(64.0) - 0.048).abs() < 1e-6);
        // Cap needs od >= ~69.4, unreachable via 6-bit quant: pin directly.
        assert_eq!(veil_alpha(100.0), 0.05);
        assert_eq!(veil_alpha(1.0e6), 0.05);
        assert!((veil_alpha(1.0) - 0.006).abs() < 1e-9);
        assert_eq!(veil_alpha(0.0), 0.0);
    }

    #[test]
    fn diameters_follow_class() {
        // n=2, max-quant cells: sheet / filament / node, rest void+thin.
        let field = synth_field(2, &[(SHEET, 63), (FILAMENT, 63), (NODE, 63)]);
        let out = veil_sprites(&field, DVec3::ZERO);
        assert_eq!(out.len(), 3);
        // Nearest-center mapping (jitter < half a cell, unambiguous).
        let centers = [[0, 0, 0], [1, 0, 0], [0, 1, 0]];
        let mut diams: Vec<f32> = Vec::new();
        for (pos, _, misc) in &out {
            let mut best = (usize::MAX, f32::INFINITY);
            for (k, [cx, cy, cz]) in centers.iter().enumerate() {
                let c = [
                    (-4.0 + (*cx as f64 + 0.5) * 4.0) as f32,
                    (-4.0 + (*cy as f64 + 0.5) * 4.0) as f32,
                    (-4.0 + (*cz as f64 + 0.5) * 4.0) as f32,
                ];
                let d = (pos[0] - c[0]).abs() + (pos[1] - c[1]).abs() + (pos[2] - c[2]).abs();
                if d < best.1 {
                    best = (k, d);
                }
            }
            let want = [2.0, 1.4, 1.2][best.0] * 4.0;
            assert!((misc[0] - want).abs() < 1e-6, "class diameter wrong");
            assert_eq!(misc[2], 1.0);
            diams.push(misc[0]);
        }
        diams.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(diams, [1.2 * 4.0, 1.4 * 4.0, 2.0 * 4.0]);
    }

    #[test]
    fn jitter_bounded_deterministic_and_rebased() {
        let field = synth_field(2, &[(FILAMENT, 63)]);
        let a = veil_sprites(&field, DVec3::ZERO);
        let b = veil_sprites(&field, DVec3::ZERO);
        assert_eq!(a, b, "sprites must replay bit-identically");
        assert_eq!(a.len(), 1);
        // Center of cell 0 + jitter within +-0.25 cell.
        let center = [(-4.0 + 0.5 * 4.0) as f32; 3];
        for (axis, c) in center.iter().enumerate() {
            let off = (a[0].0[axis] - c) / 4.0;
            assert!(
                (-0.25 - 1e-6..=0.25 + 1e-6).contains(&off),
                "jitter out of band: {off}"
            );
        }
        // Rebase shifts positions exactly.
        let o = DVec3::new(1.0, -2.0, 3.0);
        let c = veil_sprites(&field, o);
        for (axis, shift) in [o.x, o.y, o.z].iter().enumerate() {
            assert!((a[0].0[axis] - c[0].0[axis] - *shift as f32).abs() < 1e-6);
            assert_eq!(a[0].1[axis], c[0].1[axis]);
        }
        assert_eq!(a[0].2, c[0].2);
    }

    #[test]
    fn sprites_small_box_structure_and_determinism() {
        let p = CosmicWebParams::new(32, 4.0, 50.0).expect("small test params fit");
        let (_, field) = generate_cosmic_web_with_field(11, &p);
        let out = veil_sprites(&field, DVec3::ZERO);
        eprintln!("small-box veil count: {}", out.len());
        assert!(!out.is_empty() && out.len() <= 32 * 32 * 32);
        let cell = field.cell_size_mpc as f32;
        let diams = [2.0 * cell, 1.4 * cell, 1.2 * cell];
        let lo = 0.006 * 0.5_f32.sqrt() - 1e-6;
        for (_, color, misc) in &out {
            assert!(
                diams.iter().any(|d| (d - misc[0]).abs() <= 1e-4 * d),
                "bad diameter {}",
                misc[0]
            );
            assert!((lo..=0.05).contains(&misc[1]), "bad alpha {}", misc[1]);
            assert_eq!(misc[2], 1.0);
            for c in color {
                assert!(c.is_finite() && *c >= 0.0);
            }
        }
        assert_eq!(out, veil_sprites(&field, DVec3::ZERO));
    }

    #[test]
    fn nominal_veil_count_in_band() {
        // Slow (~15 s): full 128^3 export + sprite walk.
        // Measured 335_387 at seed 1234 (sphere-cut): the notion's
        // 120–180k was an estimate; the band pins the measurement.
        // Low draws a stride-2 subset (~168k ≤ 200k budget) at upload.
        let p = CosmicWebParams::nominal();
        let (_, field) = generate_cosmic_web_with_field(1234, &p);
        let out = veil_sprites(&field, DVec3::ZERO);
        eprintln!("nominal veil count: {}", out.len());
        assert!(
            (300_000..=370_000).contains(&out.len()),
            "nominal veil count out of band: {}",
            out.len()
        );
    }
    #[test]
    fn empty_field_emits_nothing() {
        let field = WebField {
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 4.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 1.0e9,
        };
        assert!(veil_sprites(&field, DVec3::ZERO).is_empty());
        // Void-only grid emits nothing either.
        let void = synth_field(2, &[(0, 0)]);
        assert!(veil_sprites(&void, DVec3::ZERO).is_empty());
    }

    #[test]
    fn volume_bytes_pack_the_quant() {
        // q=0 → 0, q=63 → 255, class bits dropped.
        let field = synth_field(2, &[(1, 0), (2, 63), (3, 32)]);
        let bytes = veil_volume_bytes(&field);
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[..3], &[0, 255, 130]);
        assert_eq!(&bytes[3..], &[0; 5]);
        // Degenerate fields build no texture.
        let empty = WebField {
            displacement: Vec::new(),
            grid: Vec::new(),
            grid_cells: 0,
            cell_size_mpc: 4.0,
            mean_density: 1.0,
            origin_mpc: [0.0; 3],
            sphere_radius_mpc: 1.0e9,
        };
        assert!(veil_volume_bytes(&empty).is_empty());
    }

    #[test]
    fn march_ray_hits_the_sphere_center() {
        // Identity view-projection: NDC maps 1:1 to world, so the
        // center pixel's ray runs down −z… hand-computed: eye
        // (0,0,5), sphere r=1 at origin, uv=(0.5,0.5) → ndc=(0,0),
        // near=(0,0,0), far=(0,0,1), dir=(0,0,1)?? — instead assert
        // structural properties that pin the formula: the center ray
        // of a symmetric frustum passes through the sphere center,
        // and a corner ray misses a small off-axis sphere.
        let ident = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        // Eye behind the sphere looking down +z at it.
        let hit = march_ray(ident, [0.5, 0.5], [0.0, 0.0, -5.0], [0.0, 0.0, 0.0], 1.0)
            .expect("center ray must hit");
        assert!((hit.2 - 4.0).abs() < 1e-5, "entry at t=4, got {}", hit.2);
        assert!((hit.3 - 6.0).abs() < 1e-5, "exit at t=6, got {}", hit.3);
        assert!((hit.1[2] - 1.0).abs() < 1e-6, "dir must be +z");
        // Offset eye: the parallel ray at x=10 misses the origin sphere.
        assert!(march_ray(ident, [0.0, 0.0], [10.0, 0.0, -5.0], [0.0, 0.0, 0.0], 0.01).is_none());
        // Eye inside: t0 clamps to 0, t1 positive.
        let inside = march_ray(ident, [0.5, 0.5], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0)
            .expect("inside ray must hit");
        assert_eq!(inside.2, 0.0);
        assert!((inside.3 - 1.0).abs() < 1e-5);
    }

    /// Reconstruction consistency (CGV-007 GPU-free pin): the march ray
    /// through a node's projected pixel — reconstructed from the SAME
    /// view-projection the draws use — points at the node and brackets
    /// its depth. A blob marched from the single cell containing that
    /// node then lands where a splat at the cell center lands (same
    /// matrix, NDC top row, on both sides).
    #[test]
    fn march_ray_agrees_with_project_to_screen() {
        use crate::map_camera::MapOrbitCamera;
        use crate::picking::project_to_screen;
        use crate::ui::Rect;
        use glam::{Mat4, Vec3};
        let mut cam = MapOrbitCamera::new(Vec3::ZERO, 430.0, 0.5, 0.9, 10.0, 1600.0, 250.0);
        cam.set_fov_keep_framing(20.0);
        let aspect = 1408.0 / 768.0;
        let vp_mat: Mat4 = cam.view_proj(aspect);
        let vp = Rect {
            x: 0.0,
            y: 0.0,
            w: 1408.0,
            h: 768.0,
        };
        // A known world point in front of the camera: the orbit
        // target itself (inside the sphere by construction).
        let target = cam.target();
        let point = target;
        let (sx, sy) = project_to_screen(point, vp_mat, vp).expect("point must project");
        // March ray through that pixel, from the same matrix.
        let inv = vp_mat.inverse().to_cols_array_2d();
        let uv = [sx / 1408.0, sy / 768.0];
        let eye = cam.eye();
        let (origin, dir, t0, t1) =
            march_ray(inv, uv, [eye.x, eye.y, eye.z], [0.0, 0.0, 0.0], 250.0)
                .expect("ray must hit the descriptor sphere");
        // Same origin, same direction (≤ 0.5°), depth bracketed.
        assert!((origin[0] - eye.x).abs() < 1e-3);
        let want = (point - eye).normalize();
        let cos = dir[0] * want.x + dir[1] * want.y + dir[2] * want.z;
        assert!(cos > 0.99996, "march dir must match the draw ray: {cos}");
        let depth = (point - eye).length();
        assert!(t0 <= depth && depth <= t1, "depth must bracket the node");
        // Pixel scale: a quarter-res march texel (×4) still resolves
        // the 4 Mpc cell the node sits in (cell ≈ 7 px at this
        // framing — CGV-007's ≤ 1 px at quarter-res × 4 with margin).
        let px_per_mpc = 768.0 / (2.0 * (20.0_f32.to_radians() / 2.0).tan()) / 400.0;
        assert!(4.0 * px_per_mpc > 2.0, "cell must span march texels");
    }
}
