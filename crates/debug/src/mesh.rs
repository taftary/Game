//! Viewer mesh builders: pure transforms from [`HexSphere`] into
//! GPU-uploadable buffers. No window, no GPU handle — the windowed binary
//! uploads the returned plain data.
//!
//! The fill fan + pentagon tint rule intentionally mirror
//! `engine::render::SeededPlanet::to_indexed_mesh` (closed fan
//! `(center, ring[i], ring[i+1 mod n])`, tint 1.0 on pentagon centers and
//! on corners touching a pentagon). `SeededPlanet` itself is tier-locked
//! (N ∈ {3,4,6}); the viewer slider needs arbitrary N, so the pattern is
//! rebuilt here over the public `HexSphere` API. Engine stays untouched.

use std::collections::BTreeSet;

use game_engine::hexsphere::HexSphere;
use game_engine::render::PlanetVertex;

/// Debug-side seam/island data aligned with [`build_fill`] vertex order
/// (centers then corners). Positions/uv live in [`PlanetVertex`]; this
/// sidecar feeds the debug-only shader varyings.
#[derive(Clone, Debug, PartialEq)]
pub struct FillDebug {
    /// 1.0 on island-cut vertices, 0.0 elsewhere.
    pub seam: Vec<f32>,
    /// Island id per vertex (0..20).
    pub island: Vec<f32>,
}

/// Read-only stats shown in the inputs panel after each regeneration.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewerStats {
    /// Dual cells (`10·4^N+2`).
    pub cells: usize,
    /// Dual corners (`20·4^N`).
    pub corners: usize,
    /// Always 12 on a valid mesh.
    pub pentagons: usize,
    /// First 8 hex chars of [`HexSphere::mesh_hash`] formatted as 16 hex digits.
    pub hash8: String,
    /// Wall time of `generate` + both builders, milliseconds.
    pub gen_ms: f64,
}

/// First 8 hex chars of a `mesh_hash` u64 (`{hash:016x}` prefix).
pub fn short_hash(hash: u64) -> String {
    format!("{hash:016x}")[..8].to_owned()
}

/// Build the filled dual-cell mesh: cell centers first, then corners;
/// one closed fan per cell. Returns `(vertices, indices)`. UVs come from
/// the engine icosa-net unwrap, so debug and release buffers agree.
pub fn build_fill(mesh: &HexSphere) -> (Vec<PlanetVertex>, Vec<u32>) {
    let cells = mesh.cell_count() as u32;
    let radius = mesh.radius();
    let uvs = game_engine::render::build_debug_uv(mesh);

    let mut corner_tint = vec![0.0f32; mesh.corner_count()];
    for cell in 0..cells {
        if mesh.is_pentagon(cell) {
            for corner in mesh.cell_corner_ids(cell) {
                corner_tint[corner as usize] = 1.0;
            }
        }
    }

    let mut vertices = Vec::with_capacity(cells as usize + mesh.corner_count());
    for cell in 0..cells {
        let center = mesh.cell_center(cell);
        vertices.push(PlanetVertex {
            position: center,
            normal: radial_normal(center, radius),
            tint: if mesh.is_pentagon(cell) { 1.0 } else { 0.0 },
            uv: uvs.uv[cell as usize],
        });
    }
    for corner in 0..mesh.corner_count() as u32 {
        let position = mesh.corner_position(corner);
        vertices.push(PlanetVertex {
            position,
            normal: radial_normal(position, radius),
            tint: corner_tint[corner as usize],
            uv: uvs.uv[cells as usize + corner as usize],
        });
    }

    let mut indices = Vec::with_capacity(3 * triangle_count(mesh));
    for cell in 0..cells {
        let ring: Vec<u32> = mesh
            .cell_corner_ids(cell)
            .map(|corner| cells + corner)
            .collect();
        for i in 0..ring.len() {
            indices.extend_from_slice(&[cell, ring[i], ring[(i + 1) % ring.len()]]);
        }
    }

    (vertices, indices)
}

/// Build the debug-only seam/island sidecar aligned with [`build_fill`]
/// vertex order. Consumed by the 6-mode debug fragment shader.
pub fn build_fill_debug(mesh: &HexSphere) -> FillDebug {
    let uvs = game_engine::render::build_debug_uv(mesh);
    FillDebug {
        seam: uvs
            .seam
            .iter()
            .map(|&s| if s { 1.0 } else { 0.0 })
            .collect(),
        island: uvs.island.iter().map(|&i| i as f32).collect(),
    }
}

/// Expected triangle count: 6 per hexagon, 5 per pentagon.
pub fn triangle_count(mesh: &HexSphere) -> usize {
    6 * (mesh.cell_count() - mesh.pentagon_count()) + 5 * mesh.pentagon_count()
}

/// Build the wireframe overlay: one deduplicated segment per dual ring
/// edge. Each ring edge is shared by exactly 2 cells, so the raw rings
/// are halved through an ordered corner-id pair set (deterministic
/// `BTreeSet`, so the buffer is identical across runs). Returns position
/// pairs (`lines.len()` is even; segment `k` is `lines[2k]` →
/// `lines[2k + 1]`).
pub fn build_wireframe(mesh: &HexSphere) -> Vec<[f32; 3]> {
    let cells = mesh.cell_count() as u32;
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for cell in 0..cells {
        let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let mut lines = Vec::with_capacity(edges.len() * 2);
    for (a, b) in edges {
        lines.push(mesh.corner_position(a));
        lines.push(mesh.corner_position(b));
    }
    lines
}

/// Snapshot stats for a mesh plus the measured generation time.
pub fn viewer_stats(mesh: &HexSphere, gen_ms: f64) -> ViewerStats {
    ViewerStats {
        cells: mesh.cell_count(),
        corners: mesh.corner_count(),
        pentagons: mesh.pentagon_count(),
        hash8: short_hash(mesh.mesh_hash()),
        gen_ms,
    }
}

fn radial_normal(point: [f32; 3], radius: f32) -> [f32; 3] {
    [point[0] / radius, point[1] / radius, point[2] / radius]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_counts_match_formulas() {
        for n in 0..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let (vertices, indices) = build_fill(&mesh);
            let cells = 10 * 4usize.pow(n) + 2;
            let corners = 20 * 4usize.pow(n);
            assert_eq!(mesh.cell_count(), cells, "N={n}");
            assert_eq!(mesh.corner_count(), corners, "N={n}");
            assert_eq!(vertices.len(), cells + corners, "N={n}");
            assert_eq!(indices.len(), 3 * triangle_count(&mesh), "N={n}");
        }
    }

    #[test]
    fn fill_closed_fan_on_dodecahedron() {
        // N=0: 12 pentagon cells, 20 corners, 12 fans of 5 tris.
        let mesh = HexSphere::generate(0, 1.0);
        let (vertices, indices) = build_fill(&mesh);
        assert_eq!(vertices.len(), 12 + 20);
        assert_eq!(indices.len(), 3 * 60);
        assert_eq!(triangle_count(&mesh), 60);
    }

    #[test]
    fn pentagon_tint_marks_twelve_sites() {
        for n in 1..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let (vertices, _) = build_fill(&mesh);
            let tinted = vertices.iter().filter(|v| v.tint == 1.0).count();
            assert_eq!(tinted, 12 * (1 + 5), "N={n}");
            assert!(vertices.iter().all(|v| v.tint == 0.0 || v.tint == 1.0));
        }
    }

    #[test]
    fn fill_normals_are_unit_length() {
        let mesh = HexSphere::generate(2, 2.5);
        let (vertices, _) = build_fill(&mesh);
        for v in &vertices {
            let len =
                (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2])
                    .sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{v:?}");
        }
    }

    #[test]
    fn wireframe_dedups_shared_edges() {
        for n in 0..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let lines = build_wireframe(&mesh);
            assert_eq!(lines.len() % 2, 0, "N={n}");
            // Raw rings emit Σring/2 shared edges; dedup must halve that.
            let ring_sum: usize = (0..mesh.cell_count() as u32)
                .map(|cell| mesh.cell_corner_ids(cell).len())
                .sum();
            assert_eq!(lines.len() / 2, ring_sum / 2, "N={n}");
        }
    }

    #[test]
    fn wireframe_endpoints_lie_on_sphere() {
        let mesh = HexSphere::generate(2, 3.0);
        for p in build_wireframe(&mesh) {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 3.0).abs() < 1e-4, "{p:?}");
        }
    }

    #[test]
    fn wireframe_has_no_duplicate_segments() {
        let mesh = HexSphere::generate(2, 1.0);
        let lines = build_wireframe(&mesh);
        let mut seen = BTreeSet::new();
        // Indexed `step_by` pairs instead of `chunks_exact(2)`: clippy
        // 1.98 pushes `as_chunks` (Rust 1.88+) but our floor is 1.87.
        for i in (0..lines.len()).step_by(2) {
            let pair = [lines[i], lines[i + 1]];
            // Match segments by quantized endpoints regardless of direction.
            let key = |p: [f32; 3]| (p.map(|c| (c * 1e6) as i32),);
            let (a, b) = (key(pair[0]), key(pair[1]));
            let ordered = if a < b { (a, b) } else { (b, a) };
            assert!(seen.insert(ordered), "duplicate segment {pair:?}");
        }
    }

    #[test]
    fn wireframe_uv_matches_wireframe_topology() {
        // Flat wireframe now lives in the engine clipper
        // (`render::build_wireframe_uv_clipped`, tested there): same-island
        // edges keep both corner endpoints, cross-island edges split at
        // each side's island boundary. Debug only asserts the shared raw
        // edge topology source stays aligned.
        for n in 0..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let lines3 = build_wireframe(&mesh);
            let lines2 = game_engine::render::build_wireframe_uv_clipped(&mesh);
            assert_eq!(lines2.len() % 2, 0, "N={n}");
            assert!(lines2.len() >= lines3.len(), "N={n}");
            for uv in &lines2 {
                assert!((0.0..=1.0).contains(&uv[0]), "N={n} {uv:?}");
                assert!((0.0..=1.0).contains(&uv[1]), "N={n} {uv:?}");
            }
        }
    }

    #[test]
    fn builds_are_deterministic() {
        let a = HexSphere::generate(2, 1.0);
        let b = HexSphere::generate(2, 1.0);
        assert_eq!(build_fill(&a).0, build_fill(&b).0);
        assert_eq!(build_fill(&a).1, build_fill(&b).1);
        assert_eq!(build_wireframe(&a), build_wireframe(&b));
    }

    #[test]
    fn matches_engine_indexed_mesh_at_high_tier() {
        use game_engine::render::{QualityTier, SeededPlanet};

        // High tier is N=6: the only tier the viewer default shares with
        // `SeededPlanet`. Bit-identical buffers prove the viewer fan, tint
        // rule and winding match the M1-proven mesh exactly.
        let engine = SeededPlanet::generate(1337, QualityTier::High, 1.0).to_indexed_mesh();
        let mesh = HexSphere::generate(6, 1.0);
        let (vertices, indices) = build_fill(&mesh);
        assert_eq!(vertices, engine.vertices);
        assert_eq!(indices, engine.indices);
    }

    #[test]
    fn fill_carries_unit_range_uvs_with_sidecar() {
        let mesh = HexSphere::generate(2, 1.0);
        let (vertices, _) = build_fill(&mesh);
        let debug = build_fill_debug(&mesh);
        assert_eq!(debug.seam.len(), vertices.len());
        assert_eq!(debug.island.len(), vertices.len());
        for v in &vertices {
            assert!((0.0..=1.0).contains(&v.uv[0]), "{v:?}");
            assert!((0.0..=1.0).contains(&v.uv[1]), "{v:?}");
        }
        assert!(debug.seam.iter().all(|&s| s == 0.0 || s == 1.0));
        assert!(
            debug.island.iter().all(|&i| (0.0..20.0).contains(&i)),
            "islands are base-face ids"
        );
        assert!(debug.seam.contains(&1.0), "some seam verts");
    }

    #[test]
    fn fill_faces_point_outward() {
        // Geometric winding guard: every fan triangle's cross-product
        // normal must agree with the outward radial direction. A flipped
        // fan order would fail here (and render inside-out under
        // backface culling).
        for n in 0..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let (vertices, indices) = build_fill(&mesh);
            let pos = |i: u32| {
                let p = vertices[i as usize].position;
                [p[0], p[1], p[2]]
            };
            let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
            let cross = |a: [f32; 3], b: [f32; 3]| {
                [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ]
            };
            let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            // Indexed `step_by` triples instead of `chunks_exact(3)`:
            // clippy 1.98 pushes `as_chunks` (Rust 1.88+) but our floor
            // is 1.87.
            for i in (0..indices.len()).step_by(3) {
                let tri = [indices[i], indices[i + 1], indices[i + 2]];
                let (a, b, c) = (pos(tri[0]), pos(tri[1]), pos(tri[2]));
                let normal = cross(sub(b, a), sub(c, a));
                let centroid = [
                    (a[0] + b[0] + c[0]) / 3.0,
                    (a[1] + b[1] + c[1]) / 3.0,
                    (a[2] + b[2] + c[2]) / 3.0,
                ];
                assert!(dot(normal, centroid) > 0.0, "N={n} inward tri {tri:?}");
            }
        }
    }

    #[test]
    fn tint_marks_only_pentagon_sites_positionally() {
        // Positional (not just count) tint check: a center is tinted iff
        // its cell is a pentagon; a corner is tinted iff it touches a
        // pentagon. Count-only checks cannot catch a center/corner swap.
        for n in 0..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let (vertices, _) = build_fill(&mesh);
            let cells = mesh.cell_count();
            for cell in 0..cells as u32 {
                assert_eq!(
                    vertices[cell as usize].tint,
                    if mesh.is_pentagon(cell) { 1.0 } else { 0.0 },
                    "N={n} center tint wrong for cell {cell}"
                );
            }
            let mut touching = vec![false; mesh.corner_count()];
            for cell in 0..cells as u32 {
                if mesh.is_pentagon(cell) {
                    for corner in mesh.cell_corner_ids(cell) {
                        touching[corner as usize] = true;
                    }
                }
            }
            for corner in 0..mesh.corner_count() as u32 {
                assert_eq!(
                    vertices[cells + corner as usize].tint,
                    if touching[corner as usize] { 1.0 } else { 0.0 },
                    "N={n} corner tint wrong for corner {corner}"
                );
            }
        }
    }

    #[test]
    fn short_hash_is_016x_prefix() {
        for hash in [0u64, 1, 11_459_543_604_394_007_386, u64::MAX] {
            let short = short_hash(hash);
            assert_eq!(short.len(), 8, "{hash}");
            assert_eq!(short, &format!("{hash:016x}")[..8], "{hash}");
        }
    }

    #[test]
    fn stats_snapshot_matches_mesh() {
        let mesh = HexSphere::generate(3, 1.0);
        let stats = viewer_stats(&mesh, 12.5);
        assert_eq!(stats.cells, 642);
        assert_eq!(stats.corners, 1280);
        assert_eq!(stats.pentagons, 12);
        assert_eq!(stats.hash8, short_hash(mesh.mesh_hash()));
        assert_eq!(stats.gen_ms, 12.5);
    }
}
