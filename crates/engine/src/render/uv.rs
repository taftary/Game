//! Icosahedron-net UV unwrap for the [`HexSphere`](crate::hexsphere::HexSphere)
//! dual mesh (debug texturing, `plans/sphere-uv-debug`).
//!
//! Scheme [`UvScheme::IcosaNet`]: the 20 base faces lay out as the classic
//! 5-top / 10-middle / 5-bottom triangle strip (absolute closed-form slot
//! triangles, normalized to `[0,1]²` — the Paul Bourke reference the viewer
//! is judged against). Each slot is assigned the forced icosa neighbor by
//! walking one rooted tree (slot 0 = base face 0): every tree edge is a
//! real shared icosa edge, so the 19 internal adjacencies are exact and
//! islands never overlap. Final primal faces inherit their island via
//! [`base_face_ids`](crate::hexsphere::base_face_ids); dual corners (face
//! centroids) and centers (primal vertices) both map by barycentric
//! projection onto their island (Bourke tessellation mapping). Centers on
//! base edges/vertices span islands: their fan triangles stretch across
//! the cut — flagged in [`UvData::seam`].
//!
//! Pure + deterministic: same subdivisions give bit-identical output on
//! every platform (closed-form templates, ordered maps, fixed walk order,
//! no hashing).

use std::collections::{BTreeMap, BTreeSet};

use glam::Vec3;

use crate::hexsphere::{BASE_FACE_COUNT, HexSphere, base_face_ids, base_faces, base_vertices};

/// UV unwrapping scheme. Only the icosahedron net exists in v1;
/// equirectangular / cube variants are reserved by this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvScheme {
    /// 20-island unfolded net (v1, implemented).
    IcosaNet,
}

/// Per-vertex UV data aligned with the `build_fill` order: cell centers
/// first (`cell_count`), then corners (`corner_count`).
#[derive(Clone, Debug, PartialEq)]
pub struct UvData {
    /// Texture coordinates in `[0,1]²`.
    pub uv: Vec<[f32; 2]>,
    /// True when the vertex touches an island cut (center spanning >1 base
    /// face). Corners are always interior → false.
    pub seam: Vec<bool>,
    /// Island id (`0..20`, = base-face id).
    pub island: Vec<u32>,
}

/// Builds [`UvData`] for a mesh: corners → net-triangle centroids, centers
/// → barycentric projection onto the smallest incident island.
pub fn build_debug_uv(mesh: &HexSphere) -> UvData {
    build_debug_uv_for_scheme(mesh, UvScheme::IcosaNet)
}

/// [`build_debug_uv`] with an explicit scheme (single variant today).
pub fn build_debug_uv_for_scheme(mesh: &HexSphere, scheme: UvScheme) -> UvData {
    assert_eq!(scheme, UvScheme::IcosaNet, "only IcosaNet exists in v1");
    let faces = base_faces();
    let verts3: Vec<Vec3> = base_vertices()
        .iter()
        .map(|p| Vec3::from_array(*p))
        .collect();
    let (net, slot_of_face, slot_vert) = strip_net(&faces);
    let face_ids = base_face_ids(mesh.subdivisions());
    debug_assert_eq!(face_ids.len(), mesh.corner_count());

    let cells = mesh.cell_count() as u32;
    let mut uv = Vec::with_capacity(cells as usize + mesh.corner_count());
    let mut seam = Vec::with_capacity(cells as usize + mesh.corner_count());
    let mut island = Vec::with_capacity(cells as usize + mesh.corner_count());

    // Centers: smallest incident island + barycentric projection.
    for cell in 0..cells {
        let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
        let mut incident: BTreeSet<u32> = BTreeSet::new();
        for &corner in &ring {
            incident.insert(face_ids[corner as usize]);
        }
        let chosen = *incident.iter().min().expect("cell ring is never empty");
        seam.push(incident.len() > 1);
        island.push(chosen);
        let dir = Vec3::from_array(mesh.cell_center(cell)).normalize();
        uv.push(map_to_slot(
            dir,
            chosen,
            &faces,
            &verts3,
            &net,
            &slot_of_face,
            &slot_vert,
        ));
    }
    // Corners: each is one primal-face centroid → barycentric projection
    // onto its own base face (Bourke tessellation mapping). Centroids are
    // always strictly interior, so corners land strictly inside their
    // slot triangle — never collapsed onto one point.
    for corner in 0..mesh.corner_count() as u32 {
        let base = face_ids[corner as usize];
        let dir = Vec3::from_array(mesh.corner_position(corner)).normalize();
        uv.push(map_to_slot(
            dir,
            base,
            &faces,
            &verts3,
            &net,
            &slot_of_face,
            &slot_vert,
        ));
        seam.push(false);
        island.push(base);
    }
    UvData { uv, seam, island }
}

/// Expanded flat-view buffers with proper seam duplication: every emitted
/// triangle has all three vertices in one island, so no fan ever stretches
/// across the net (update-2026-09-15-0730). Parallel arrays are indexed by
/// the flat vertex buffer: originals first (fill vertex order), then
/// duplicates in first-creation order.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatUnwrap {
    /// Texture coordinates in `[0,1]²`.
    pub uv: Vec<[f32; 2]>,
    /// Island id per flat vertex (0..20).
    pub island: Vec<u32>,
    /// 1.0 on seam-part vertices (cut-cell portions), 0.0 elsewhere.
    pub seam: Vec<bool>,
    /// Flat index buffer (triangle list). Non-seam cells reuse the
    /// original fill fan byte-equal; seam cells emit one fan per incident
    /// island using duplicates.
    pub indices: Vec<u32>,
    /// Fill vertex each flat vertex copies attributes from (`i` for
    /// originals, the source fill vertex for duplicates).
    pub source: Vec<u32>,
}

/// Builds [`FlatUnwrap`] for a mesh. Seam cells (dual centers spanning ≥2
/// islands) emit one fan per incident island; duplicates project onto the
/// target island via the same barycentric+snap path as [`build_debug_uv`].
pub fn build_flat_unwrap(mesh: &HexSphere) -> FlatUnwrap {
    let faces = base_faces();
    let verts3: Vec<Vec3> = base_vertices()
        .iter()
        .map(|p| Vec3::from_array(*p))
        .collect();
    let (net, slot_of_face, slot_vert) = strip_net(&faces);
    let face_ids = base_face_ids(mesh.subdivisions());
    let data = build_debug_uv(mesh);
    let cells = mesh.cell_count() as u32;

    let mut flat = FlatUnwrap {
        uv: data.uv.clone(),
        island: data.island.clone(),
        seam: data.seam.clone(),
        indices: Vec::new(),
        source: (0..(cells + mesh.corner_count() as u32)).collect(),
    };
    // Duplicate vertex table: (source fill index, island) → flat index.
    let mut dups: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut dup_of = |flat: &mut FlatUnwrap, src: u32, island: u32, uv: [f32; 2]| -> u32 {
        *dups.entry((src, island)).or_insert_with(|| {
            flat.uv.push(uv);
            flat.island.push(island);
            flat.seam.push(true);
            flat.source.push(src);
            flat.uv.len() as u32 - 1
        })
    };
    let corner_home = |corner: u32| face_ids[corner as usize];

    for cell in 0..cells {
        let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
        let mut incident: BTreeSet<u32> = BTreeSet::new();
        for &corner in &ring {
            incident.insert(corner_home(corner));
        }
        if incident.len() == 1 {
            // Non-seam cell: original fan, original vertex indices.
            for i in 0..ring.len() {
                flat.indices.extend_from_slice(&[
                    cell,
                    cells + ring[i],
                    cells + ring[(i + 1) % ring.len()],
                ]);
            }
            continue;
        }
        let dir = Vec3::from_array(mesh.cell_center(cell)).normalize();
        for &island in &incident {
            // Center: original index if this island is its chosen home
            // (matches `UvData`), else a duplicate snapped onto the island.
            let center_idx = if data.island[cell as usize] == island {
                cell
            } else {
                let uv = map_to_slot(
                    dir,
                    island,
                    &faces,
                    &verts3,
                    &net,
                    &slot_of_face,
                    &slot_vert,
                );
                dup_of(&mut flat, cell, island, uv)
            };
            for i in 0..ring.len() {
                let mut tri = [center_idx, 0, 0];
                for (slot, corner) in [(1, ring[i]), (2, ring[(i + 1) % ring.len()])] {
                    let home = corner_home(corner);
                    tri[slot] = if home == island {
                        cells + corner
                    } else {
                        let dir = Vec3::from_array(mesh.corner_position(corner)).normalize();
                        let uv = map_to_slot(
                            dir,
                            island,
                            &faces,
                            &verts3,
                            &net,
                            &slot_of_face,
                            &slot_vert,
                        );
                        dup_of(&mut flat, cells + corner, island, uv)
                    };
                }
                flat.indices.extend_from_slice(&tri);
            }
        }
    }
    flat
}

/// Clips the deduplicated dual-ring wireframe to island interiors: a
/// segment whose endpoints live in different islands is split at each
/// side's island boundary (wires break at cuts instead of spanning the
/// net). Returns position pairs (`lines.len()` even).
pub fn build_wireframe_uv_clipped(mesh: &HexSphere) -> Vec<[f32; 2]> {
    let cells = mesh.cell_count() as u32;
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for cell in 0..cells {
        let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let faces = base_faces();
    let (net, _, _) = strip_net(&faces);
    let face_ids = base_face_ids(mesh.subdivisions());
    let data = build_debug_uv(mesh);
    let corner_uv = |corner: u32| data.uv[cells as usize + corner as usize];
    let mut lines = Vec::with_capacity(edges.len() * 2);
    for (a, b) in edges {
        let (ia, ib) = (face_ids[a as usize], face_ids[b as usize]);
        if ia == ib {
            lines.push(corner_uv(a));
            lines.push(corner_uv(b));
        } else {
            // Each endpoint keeps the portion inside its own island.
            // Fallback (0,0): when the corner sits numerically just
            // outside its home slot and the segment points away, the
            // inside interval is empty — emit a degenerate zero-length
            // piece so every cross-island edge always yields 4 points.
            let (pa, pb) = (corner_uv(a), corner_uv(b));
            for (p, q, island) in [(pa, pb, ia), (pb, pa, ib)] {
                let tri = net[island as usize];
                let (t0, t1) = clip_segment_to_triangle(p, q, tri).unwrap_or((0.0, 0.0));
                lines.push(lerp2(p, q, t0));
                lines.push(lerp2(p, q, t1));
            }
        }
    }
    lines
}

fn lerp2(p: [f32; 2], q: [f32; 2], t: f32) -> [f32; 2] {
    [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]
}

/// Cyrus–Beck-style clip of segment `p→q` to a triangle's three
/// half-planes. Returns the inside parameter interval, if any.
fn clip_segment_to_triangle(p: [f32; 2], q: [f32; 2], tri: Tri2) -> Option<(f32, f32)> {
    const EPS: f32 = 1e-6;
    let mut interval = (0.0f32, 1.0f32);
    let d = [q[0] - p[0], q[1] - p[1]];
    for k in 0..3 {
        let a = tri[k];
        let b = tri[(k + 1) % 3];
        // Outward normal of edge a→b for a CCW triangle is (ey, -ex).
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let (nx, ny) = (ey, -ex);
        let dist = |r: [f32; 2]| (r[0] - a[0]) * nx + (r[1] - a[1]) * ny;
        let (dp, dd) = (dist(p), d[0] * nx + d[1] * ny);
        if dd.abs() < EPS {
            // Parallel: inside-or-on this plane only if dist ≤ 0.
            if dp > EPS {
                return None;
            }
            continue;
        }
        let t = -dp / dd;
        if dd < 0.0 {
            // Entering (outward normal points away from the interior).
            interval.0 = interval.0.max(t);
        } else {
            interval.1 = interval.1.min(t);
        }
        if interval.0 > interval.1 {
            return None;
        }
    }
    // Snap endpoints: start/end-on-boundary must stay bit-exact (callers
    // expect piece starts equal to the corner's own uv).
    let (mut t0, mut t1) = interval;
    if t0 < EPS {
        t0 = 0.0;
    }
    if t1 > 1.0 - EPS {
        t1 = 1.0;
    }
    Some((t0, t1))
}

/// Projects a unit direction onto base face `base` (barycentric vs the
/// unit 3D triangle) and maps it through that face's slot correspondence.
/// Template corners are not in face-triple order, so each weight is routed
/// to its corner explicitly.
#[allow(clippy::too_many_arguments)]
fn map_to_slot(
    dir: Vec3,
    base: u32,
    faces: &[[u32; 3]],
    verts3: &[Vec3],
    net: &[Tri2],
    slot_of_face: &[usize],
    slot_vert: &[[u32; 3]],
) -> [f32; 2] {
    let tri = faces[base as usize];
    let (a, b, c) = (
        verts3[tri[0] as usize],
        verts3[tri[1] as usize],
        verts3[tri[2] as usize],
    );
    let w = barycentric(dir, a, b, c);
    // Relaxation + spherical projection can drift boundary points
    // marginally across base edges: snap back onto the island. Interior
    // points (all weights positive) pass through untouched.
    let positive: f32 = w.iter().map(|&x| x.max(0.0)).sum();
    let w = if positive > 1e-6 {
        [
            w[0].max(0.0) / positive,
            w[1].max(0.0) / positive,
            w[2].max(0.0) / positive,
        ]
    } else {
        [1.0 / 3.0; 3]
    };
    let slot = slot_of_face[base as usize];
    let corners = slot_vert[slot];
    let mut wk = [0.0f32; 3];
    for (k, &v) in corners.iter().enumerate() {
        let j = tri
            .iter()
            .position(|&x| x == v)
            .expect("slot corner is a face vert");
        wk[k] = w[j];
    }
    mix2(net[base as usize], wk)
}

/// 2D triangle in net space.
type Tri2 = [[f32; 2]; 3];
/// One rooted tree edge: child + parent slots with the coincident
/// template corner pairs `(parent_corners, child_corners)`.
type WalkEdge = (usize, usize, (usize, usize), (usize, usize));
/// Strip tables: per-face triangles, slot of each face, icosa vert at
/// each slot corner.
type StripTables = (Vec<Tri2>, Vec<usize>, Vec<[u32; 3]>);

/// Reference strip slots: 0–4 top row (up), 5–9 second row (down),
/// 10 left cap (up), 11–14 middle joints (up), 15–19 bottom row (down).
/// Closed-form unit equilateral triangles, CCW corners.
fn slot_pos(slot: usize, h: f32) -> Tri2 {
    let i = match slot {
        0..=4 => slot as f32,
        5..=9 => (slot - 5) as f32,
        10 => 0.0,
        11..=14 => (slot - 10) as f32,
        _ => (slot - 15) as f32,
    };
    match slot {
        // Top row, apex up.
        0..=4 => [[i, 0.0], [i + 1.0, 0.0], [i + 0.5, h]],
        // Second row, apex down.
        5..=9 => [[i, 0.0], [i + 0.5, -h], [i + 1.0, 0.0]],
        // Left cap + middle joints, apex up.
        10 => [[0.0, 0.0], [-0.5, -h], [0.5, -h]],
        11..=14 => [[i, 0.0], [i - 0.5, -h], [i + 0.5, -h]],
        // Bottom row, apex down.
        _ => [[i - 0.5, -h], [i, -2.0 * h], [i + 0.5, -h]],
    }
}

/// One rooted tree edge: `child` attaches to `parent`; `(pa, pb)` are the
/// parent template corners and `(ca, cb)` the coincident child corners.
/// Array order guarantees parents place before children; the 19 edges form
/// a single tree rooted at slot 0.
const WALK: [WalkEdge; 19] = [
    (5, 0, (0, 1), (0, 2)),   // D_0 ← T_0
    (10, 5, (0, 1), (0, 2)),  // UL ← D_0
    (15, 10, (1, 2), (0, 2)), // B_0 ← UL
    (11, 5, (2, 1), (0, 1)),  // Btw_1 ← D_0
    (6, 11, (0, 2), (0, 1)),  // D_1 ← Btw_1
    (1, 6, (0, 2), (0, 1)),   // T_1 ← D_1
    (16, 11, (1, 2), (0, 2)), // B_1 ← Btw_1
    (12, 6, (2, 1), (0, 1)),  // Btw_2 ← D_1
    (7, 12, (0, 2), (0, 1)),  // D_2 ← Btw_2
    (2, 7, (0, 2), (0, 1)),   // T_2 ← D_2
    (17, 12, (1, 2), (0, 2)), // B_2 ← Btw_2
    (13, 7, (2, 1), (0, 1)),  // Btw_3 ← D_2
    (8, 13, (0, 2), (0, 1)),  // D_3 ← Btw_3
    (3, 8, (0, 2), (0, 1)),   // T_3 ← D_3
    (18, 13, (1, 2), (0, 2)), // B_3 ← Btw_3
    (14, 8, (2, 1), (0, 1)),  // Btw_4 ← D_3
    (9, 14, (0, 2), (0, 1)),  // D_4 ← Btw_4
    (4, 9, (0, 2), (0, 1)),   // T_4 ← D_4
    (19, 14, (1, 2), (0, 2)), // B_4 ← Btw_4
];

/// Face + corner verts per strip slot: `verts[s][k]` is the icosa vertex
/// id at template corner `k` of slot `s`. Slot 0 is base face 0 in triple
/// order; every other slot takes the forced icosa neighbor across the
/// shared parent edge, so all 19 internal adjacencies are real shared
/// edges (validated by `strip_walk_covers_each_face_once`).
fn strip_slots(faces: &[[u32; 3]]) -> (Vec<u32>, Vec<[u32; 3]>) {
    let mut edge_faces: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for (x, y) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
            edge_faces
                .entry((x.min(y), x.max(y)))
                .or_default()
                .push(f as u32);
        }
    }
    let across = |face: u32, a: u32, b: u32| -> u32 {
        let owners = &edge_faces[&(a.min(b), a.max(b))];
        assert_eq!(owners.len(), 2, "non-manifold base edge");
        if owners[0] == face {
            owners[1]
        } else {
            owners[0]
        }
    };
    let mut slot_face = vec![u32::MAX; BASE_FACE_COUNT];
    let mut slot_vert = vec![[u32::MAX; 3]; BASE_FACE_COUNT];
    slot_face[0] = 0;
    slot_vert[0] = faces[0];
    for (child, parent, (pa, pb), (ca, cb)) in WALK {
        let a = slot_vert[parent][pa];
        let b = slot_vert[parent][pb];
        assert_ne!(slot_face[parent], u32::MAX, "parent places first");
        let next = across(slot_face[parent], a, b);
        slot_face[child] = next;
        let third = *faces[next as usize]
            .iter()
            .find(|&&v| v != a && v != b)
            .expect("shared edge leaves one third vert");
        let mut corners = [u32::MAX; 3];
        corners[ca] = a;
        corners[cb] = b;
        corners[*[0, 1, 2]
            .iter()
            .find(|&&k| k != ca && k != cb)
            .expect("third corner")] = third;
        slot_vert[child] = corners;
    }
    (slot_face, slot_vert)
}

/// Reference strip net: absolute closed-form slot triangles assigned to
/// base faces by [`strip_slots`], normalized to `[0,1]²`. Returns
/// per-face 2D corners in face order plus the slot tables (slot of each
/// face, icosa vert at each slot corner) for barycentric mapping.
fn strip_net(faces: &[[u32; 3]]) -> StripTables {
    let h = (3.0f32).sqrt() / 2.0;
    let (slot_face, slot_vert) = strip_slots(faces);
    let mut slot_of_face = vec![usize::MAX; faces.len()];
    for (slot, &face) in slot_face.iter().enumerate() {
        slot_of_face[face as usize] = slot;
    }
    let mut net = vec![[[0.0f32; 2]; 3]; faces.len()];
    for (face, &slot) in slot_of_face.iter().enumerate() {
        net[face] = slot_pos(slot, h);
    }
    // Normalize bounding box to [0,1]².
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for tri in &net {
        for p in tri {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
    }
    let span = [(hi[0] - lo[0]).max(1e-6), (hi[1] - lo[1]).max(1e-6)];
    for tri in &mut net {
        for p in tri {
            // Clamped: arithmetic dust can land ±1e-8 outside the box;
            // UVs must stay in `[0,1]²` bit-exactly.
            p[0] = ((p[0] - lo[0]) / span[0]).clamp(0.0, 1.0);
            p[1] = ((p[1] - lo[1]) / span[1]).clamp(0.0, 1.0);
        }
    }
    (net, slot_of_face, slot_vert)
}

/// Barycentric weights of `p` over triangle `(a,b,c)` (Ericson 5.3.2,
/// slot order). Works for near-plane projections of on-sphere points.
fn barycentric(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> [f32; 3] {
    let v0 = b - a;
    let v1 = c - a;
    let v2 = p - a;
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let denom = d00 * d11 - d01 * d01;
    debug_assert!(denom > 0.0, "degenerate base triangle");
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    [1.0 - v - w, v, w]
}

fn mix2(p: [[f32; 2]; 3], w: [f32; 3]) -> [f32; 2] {
    // Clamped: off-plane projection dust can push weights marginally
    // outside the triangle; keep UVs in `[0,1]²` bit-exactly.
    [
        (w[0] * p[0][0] + w[1] * p[1][0] + w[2] * p[2][0]).clamp(0.0, 1.0),
        (w[0] * p[0][1] + w[1] * p[1][1] + w[2] * p[2][1]).clamp(0.0, 1.0),
    ]
}

/// Strict triangle overlap (shared edges/points are fine): true when
/// interiors intersect — a vertex strictly inside the other triangle or a
/// proper edge crossing. Epsilon-tolerant so touching boundaries pass.
/// Test-only helper for the no-overlap net proof.
#[cfg(test)]
fn triangles_overlap(a: Tri2, b: Tri2) -> bool {
    const EPS: f32 = 1e-6;
    let orient = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let strictly_inside = |p: [f32; 2], t: [[f32; 2]; 3]| {
        let (o1, o2, o3) = (
            orient(t[0], t[1], p),
            orient(t[1], t[2], p),
            orient(t[2], t[0], p),
        );
        (o1 > EPS && o2 > EPS && o3 > EPS) || (o1 < -EPS && o2 < -EPS && o3 < -EPS)
    };
    for p in a {
        if strictly_inside(p, b) {
            return true;
        }
    }
    for p in b {
        if strictly_inside(p, a) {
            return true;
        }
    }
    let edges = |t: [[f32; 2]; 3]| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
    for (p1, p2) in edges(a) {
        for (q1, q2) in edges(b) {
            let (a1, a2, b1, b2) = (
                orient(p1, p2, q1),
                orient(p1, p2, q2),
                orient(q1, q2, p1),
                orient(q1, q2, p2),
            );
            // Proper crossing only: endpoint touches are shared boundary.
            if a1 * a2 < -EPS * EPS && b1 * b2 < -EPS * EPS {
                return true;
            }
        }
    }
    false
}

/// Islands referenced by corners: always all 20 (each base face owns ≥1
/// final face at every level).
pub fn island_coverage(mesh: &HexSphere) -> usize {
    let face_ids = base_face_ids(mesh.subdivisions());
    let mut seen = [false; BASE_FACE_COUNT];
    for &id in &face_ids {
        seen[id as usize] = true;
    }
    seen.iter().filter(|&&s| s).count()
}

/// Walk-tree edges with 3D data: `(parent face, child face, [shared vert
/// a, shared vert b])` in propagation order (parents place before
/// children, tree rooted at face 0). These are the 19 adjacencies the net
/// unfolds along; the gnomonic checker propagates its frame across the
/// same edges so 3D fold lines and flat-net cuts coincide
/// ([`crate::render::checker`]).
pub fn walk_tree_edges() -> Vec<(u32, u32, [u32; 2])> {
    let faces = base_faces();
    let (slot_face, slot_vert) = strip_slots(&faces);
    WALK.iter()
        .map(|&(child, parent, (pa, pb), _)| {
            (
                slot_face[parent],
                slot_face[child],
                [slot_vert[parent][pa], slot_vert[parent][pb]],
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_walk_covers_each_face_once() {
        let faces = base_faces();
        let (slot_face, slot_vert) = strip_slots(&faces);
        assert_eq!(slot_face.len(), BASE_FACE_COUNT);
        // Bijection: every base face owns exactly one slot (a reused face
        // would leave geometry unmapped and overlap islands).
        let mut seen = [false; BASE_FACE_COUNT];
        for &face in &slot_face {
            assert!((face as usize) < BASE_FACE_COUNT);
            assert!(!seen[face as usize], "face {face} placed twice");
            seen[face as usize] = true;
        }
        // Slot 0 is base face 0 in triple order (walk root).
        assert_eq!(slot_face[0], 0);
        assert_eq!(slot_vert[0], faces[0]);
        // Every tree edge is a real shared icosa edge at coincident 2D
        // points (bit-exact: templates share closed-form coordinates).
        let h = (3.0f32).sqrt() / 2.0;
        for (child, parent, (pa, pb), (ca, cb)) in WALK {
            let pp = slot_pos(parent, h);
            let cp = slot_pos(child, h);
            assert_eq!(pp[pa], cp[ca], "slots {parent}/{child}");
            assert_eq!(pp[pb], cp[cb], "slots {parent}/{child}");
            let (a, b) = (slot_vert[parent][pa], slot_vert[parent][pb]);
            let (c, d) = (slot_vert[child][ca], slot_vert[child][cb]);
            assert_eq!((a.min(b), a.max(b)), (c.min(d), c.max(d)));
            let owners = [slot_face[parent], slot_face[child]];
            for &v in &[a, b] {
                assert!(faces[owners[0] as usize].contains(&v));
                assert!(faces[owners[1] as usize].contains(&v));
            }
        }
    }

    #[test]
    fn strip_triangles_never_overlap() {
        // Pairwise SAT: interiors must be disjoint (shared edges/points
        // are fine — only strict crossings fail, the screenshot defect).
        let (net, _, _) = strip_net(&base_faces());
        for i in 0..net.len() {
            for j in (i + 1)..net.len() {
                assert!(
                    !triangles_overlap(net[i], net[j]),
                    "islands {i}/{j} overlap"
                );
            }
        }
    }
    #[test]
    fn flat_unwrap_never_spans_islands() {
        // Core invariant: every emitted triangle has all three vertices in
        // one island — no cross-net stretch is representable.
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let flat = build_flat_unwrap(&mesh);
            let fill_len = mesh.cell_count() + mesh.corner_count();
            assert_eq!(flat.uv.len(), flat.island.len());
            assert_eq!(flat.seam.len(), flat.uv.len());
            assert_eq!(flat.source.len(), flat.uv.len());
            assert_eq!(flat.indices.len() % 3, 0);
            for (i, &s) in flat.source.iter().enumerate() {
                assert!((s as usize) < fill_len, "N={n} flat {i}");
                if i < fill_len {
                    assert_eq!(s, i as u32, "N={n} originals first");
                }
            }
            for uv in &flat.uv {
                assert!((0.0..=1.0).contains(&uv[0]), "N={n} {uv:?}");
                assert!((0.0..=1.0).contains(&uv[1]), "N={n} {uv:?}");
            }
            for tri in flat.indices.chunks_exact(3) {
                let (a, b, c) = (
                    flat.island[tri[0] as usize],
                    flat.island[tri[1] as usize],
                    flat.island[tri[2] as usize],
                );
                assert!(a == b && b == c, "N={n} tri {tri:?} spans islands");
            }
        }
    }

    #[test]
    fn flat_unwrap_keeps_non_seam_fans_byte_equal() {
        let mesh = HexSphere::generate(2, 1.0);
        let flat = build_flat_unwrap(&mesh);
        let data = build_debug_uv(&mesh);
        let face_ids = base_face_ids(2);
        let cells = mesh.cell_count() as u32;
        // Indices emit cell-by-cell: non-seam cells reproduce the original
        // fan byte-equal; seam cells emit `ring × incident` tris — track
        // the stream position by simulating.
        let mut pos = 0usize;
        let mut non_seam = 0usize;
        for cell in 0..cells {
            let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
            if data.seam[cell as usize] {
                let incident: BTreeSet<u32> = ring.iter().map(|&c| face_ids[c as usize]).collect();
                pos += ring.len() * 3 * incident.len();
                continue;
            }
            non_seam += 1;
            for i in 0..ring.len() {
                assert_eq!(flat.indices[pos], cell);
                assert_eq!(flat.indices[pos + 1], cells + ring[i]);
                assert_eq!(flat.indices[pos + 2], cells + ring[(i + 1) % ring.len()]);
                pos += 3;
            }
        }
        assert!(non_seam > 0, "some non-seam cells expected");
        assert_eq!(pos, flat.indices.len());
        assert!(
            flat.uv.len() > mesh.cell_count() + mesh.corner_count(),
            "duplicates appended"
        );
    }

    #[test]
    fn flat_unwrap_is_deterministic() {
        let mesh = HexSphere::generate(2, 1.0);
        assert_eq!(build_flat_unwrap(&mesh), build_flat_unwrap(&mesh));
    }

    #[test]
    fn clipped_wire_never_crosses_islands() {
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let lines = build_wireframe_uv_clipped(&mesh);
            assert_eq!(lines.len() % 2, 0, "N={n}");
            let (net, _, _) = strip_net(&base_faces());
            let face_ids = base_face_ids(mesh.subdivisions());
            let data = build_debug_uv(&mesh);
            let cells = mesh.cell_count();
            // Rebuild the raw edge set: same-island edges emit one piece,
            // cross-island edges emit two (one per side's boundary clip).
            let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
            for cell in 0..cells as u32 {
                let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
                for i in 0..ring.len() {
                    let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                    edges.insert((a.min(b), a.max(b)));
                }
            }
            let mut k = 0;
            for (a, b) in edges {
                let (ia, ib) = (face_ids[a as usize], face_ids[b as usize]);
                if ia == ib {
                    for p in [lines[k], lines[k + 1]] {
                        let w = barycentric_2d(p, net[ia as usize]);
                        assert!(w.iter().all(|&x| x > -1e-4), "N={n} {p:?}");
                    }
                    assert_eq!(lines[k], data.uv[cells + a as usize]);
                    assert_eq!(lines[k + 1], data.uv[cells + b as usize]);
                    k += 2;
                } else {
                    // [a_start, a_boundary, b_start, b_boundary].
                    for (p, island) in [
                        (lines[k], ia),
                        (lines[k + 1], ia),
                        (lines[k + 2], ib),
                        (lines[k + 3], ib),
                    ] {
                        let w = barycentric_2d(p, net[island as usize]);
                        assert!(
                            w.iter().all(|&x| x > -1e-4),
                            "N={n} edge ({a},{b}) islands ({ia},{ib}) point {p:?} weights {w:?}"
                        );
                    }
                    // Each piece starts exactly at its corner's uv.
                    assert_eq!(lines[k], data.uv[cells + a as usize]);
                    assert_eq!(lines[k + 2], data.uv[cells + b as usize]);
                    k += 4;
                }
            }
            assert_eq!(k, lines.len(), "N={n} emission order");
        }
    }

    #[test]
    fn clipped_wire_is_deterministic() {
        let mesh = HexSphere::generate(2, 1.0);
        assert_eq!(
            build_wireframe_uv_clipped(&mesh),
            build_wireframe_uv_clipped(&mesh)
        );
    }

    #[test]
    fn clip_segment_to_triangle_cases() {
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        // Fully inside.
        let (t0, t1) = clip_segment_to_triangle([0.1, 0.1], [0.2, 0.2], tri).expect("inside");
        assert!((t0 - 0.0).abs() < 1e-6 && (t1 - 1.0).abs() < 1e-6);
        // Starts inside, exits the hypotenuse.
        let (t0, t1) = clip_segment_to_triangle([0.1, 0.1], [0.9, 0.9], tri).expect("exit");
        assert!((t0 - 0.0).abs() < 1e-6);
        assert!((t1 - 0.5).abs() < 1e-6, "{t1}");
        // Fully outside (other side of hypotenuse).
        assert!(clip_segment_to_triangle([0.9, 0.9], [1.0, 1.0], tri).is_none());
        // Crosses the whole triangle: enters through x=0, exits the
        // hypotenuse before reaching the far endpoint.
        let (t0, t1) = clip_segment_to_triangle([-0.1, 0.2], [1.2, 0.2], tri).expect("cross");
        assert!(t0 > 0.0 && t1 < 1.0 && t0 < t1, "{t0} {t1}");
    }

    #[test]
    fn net_has_twenty_unit_range_triangles() {
        let (net, _, _) = strip_net(&base_faces());
        assert_eq!(net.len(), BASE_FACE_COUNT);
        for tri in &net {
            for p in tri {
                assert!((0.0..=1.0).contains(&p[0]), "{p:?}");
                assert!((0.0..=1.0).contains(&p[1]), "{p:?}");
            }
            // Non-degenerate: positive area.
            let area = ((tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1])
                - (tri[2][0] - tri[0][0]) * (tri[1][1] - tri[0][1]))
                .abs();
            assert!(area > 1e-6, "{tri:?}");
        }
    }

    #[test]
    fn debug_uv_covers_all_islands_in_unit_range() {
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let data = build_debug_uv(&mesh);
            assert_eq!(data.uv.len(), mesh.cell_count() + mesh.corner_count());
            assert_eq!(data.seam.len(), data.uv.len());
            assert_eq!(data.island.len(), data.uv.len());
            for uv in &data.uv {
                assert!((0.0..=1.0).contains(&uv[0]), "N={n} {uv:?}");
                assert!((0.0..=1.0).contains(&uv[1]), "N={n} {uv:?}");
                assert!(uv.iter().all(|c| c.is_finite()), "N={n} {uv:?}");
            }
            assert_eq!(island_coverage(&mesh), BASE_FACE_COUNT, "N={n}");
        }
    }

    #[test]
    fn seams_only_on_island_borders() {
        let mesh = HexSphere::generate(2, 1.0);
        let data = build_debug_uv(&mesh);
        let cells = mesh.cell_count();
        // Corners are face-interior by construction.
        assert!(data.seam[cells..].iter().all(|&s| !s), "corners interior");
        // Both populations exist at N=2: edge/vertex centers seam,
        // face-interior centers do not.
        assert!(data.seam[..cells].iter().any(|&s| s), "some seam");
        assert!(data.seam[..cells].iter().any(|&s| !s), "some interior");
        // Seam flag agrees with incident-island spread, recomputed here.
        let face_ids = base_face_ids(2);
        for cell in 0..cells as u32 {
            let mut distinct = BTreeSet::new();
            for corner in mesh.cell_corner_ids(cell) {
                distinct.insert(face_ids[corner as usize]);
            }
            assert_eq!(data.seam[cell as usize], distinct.len() > 1, "cell {cell}");
        }
    }

    #[test]
    fn corners_land_strictly_inside_their_islands() {
        // Bourke tessellation mapping: every subdivided face centroid maps
        // inside its own base triangle — never collapsed onto one point
        // (the flat-view line-soup defect).
        for n in 1..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let data = build_debug_uv(&mesh);
            let (net, _, _) = strip_net(&base_faces());
            let cells = mesh.cell_count();
            let face_ids = base_face_ids(mesh.subdivisions());
            let mut per_island: Vec<BTreeSet<[i32; 2]>> = vec![BTreeSet::new(); BASE_FACE_COUNT];
            for (k, corner) in (0..mesh.corner_count() as u32).enumerate() {
                let uv = data.uv[cells + k];
                let tri = net[face_ids[corner as usize] as usize];
                let w = barycentric_2d(uv, tri);
                // Inside-or-on (snapped): never pokes into a neighbor.
                assert!(
                    w.iter().all(|&x| x > -1e-4),
                    "N={n} corner {corner} outside island: {uv:?} {w:?}"
                );
                per_island[face_ids[corner as usize] as usize]
                    .insert([(uv[0] * 1e6) as i32, (uv[1] * 1e6) as i32]);
            }
            for (i, set) in per_island.iter().enumerate() {
                assert!(set.len() > 1, "N={n} island {i} collapsed");
            }
        }
    }

    /// 2D barycentric weights of `p` over `t` (same Ericson formula).
    fn barycentric_2d(p: [f32; 2], t: Tri2) -> [f32; 3] {
        let (a, b, c) = (t[0], t[1], t[2]);
        let v0 = [b[0] - a[0], b[1] - a[1]];
        let v1 = [c[0] - a[0], c[1] - a[1]];
        let v2 = [p[0] - a[0], p[1] - a[1]];
        let (d00, d01, d11, d20, d21) = (
            v0[0] * v0[0] + v0[1] * v0[1],
            v0[0] * v1[0] + v0[1] * v1[1],
            v1[0] * v1[0] + v1[1] * v1[1],
            v2[0] * v0[0] + v2[1] * v0[1],
            v2[0] * v1[0] + v2[1] * v1[1],
        );
        let denom = d00 * d11 - d01 * d01;
        let v = (d11 * d20 - d01 * d21) / denom;
        let w = (d00 * d21 - d01 * d20) / denom;
        [1.0 - v - w, v, w]
    }

    #[test]
    fn builds_are_deterministic() {
        let a = HexSphere::generate(2, 1.0);
        let b = HexSphere::generate(2, 1.0);
        assert_eq!(build_debug_uv(&a), build_debug_uv(&b));
    }
}
