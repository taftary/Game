//! Hex-dominant geodesic sphere: the geometry base for every spherical
//! body in the game (planets, stars, moons).
//!
//! Pipeline (see `plans/hex-sphere/notion.md`): icosahedron → N
//! subdivisions (each triangle → 4, shared-edge midpoint dedup) →
//! spherical projection after every level → fixed Lloyd relaxation
//! ([`RELAX_ITERATIONS`] passes, reproject each pass) → dual mesh
//! (triangle centroids → polygon cells with neighbor graph).
//!
//! The mesh is seed-independent: one [`HexSphere`] per `(N, radius)` is
//! shared across all bodies; per-seed data layers stack on top later.
//! Everything here is pure and deterministic — same `(N, radius)` gives
//! bit-identical output on every platform (integer-driven topology,
//! fixed iteration counts, quantized hash in [`HexSphere::mesh_hash`]).

use std::collections::BTreeMap;

use glam::Vec3;

/// Default subdivision level: 10·4⁶+2 = 40,962 cells.
pub const DEFAULT_SUBDIVISIONS: u32 = 6;

/// Fixed Lloyd relaxation passes, pinned per [`GEOMETRY_VERSION`].
/// Never adaptive: convergence criteria would break cross-platform
/// determinism.
pub const RELAX_ITERATIONS: u32 = 7;

/// Geometry format version. Bumped only with an intentional mesh change;
/// stamped into [`HexSphere::mesh_hash`].
pub const GEOMETRY_VERSION: u32 = 1;

/// Sentinel marking the unused 6th slot of a pentagon's neighbor and
/// corner-ring rows. Never exposed: accessors yield valid ids only.
const MISSING: u32 = u32::MAX;

/// Fixed-point scale for the determinism hash: positions are quantized
/// to millionths of the radius before hashing, so 1-ulp float drift
/// across platforms cannot flip the hash.
const HASH_QUANTUM: f32 = 1_000_000.0;

/// Stable identity of one cell-chunk: the dual cell's index.
///
/// 1 cell = 1 chunk (see `plans/cell-chunks`): pentagons and hexagons
/// are both first-class chunks addressed by this id. The index space is
/// the deterministic cell order pinned by the committed mesh hash, so a
/// `ChunkId` is stable for a given `(subdivisions, radius,
/// GEOMETRY_VERSION)` — the key M2 streaming and per-chunk surface
/// layers will load against. Streaming groups/patches, if any, are built
/// *from* these ids; chunk identity itself never re-numbers.
///
/// Construct only through [`HexSphere::chunk_id`]: the constructor is
/// private so an id always names a real cell of its mesh.
///
/// ```
/// use game_engine::hexsphere::HexSphere;
///
/// let body = HexSphere::generate(2, 1.0);
/// let chunk = body.chunk_id(7);
/// assert_eq!(chunk.index(), 7);
/// assert!(body.chunk_id(6) < chunk);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId(u32);

impl ChunkId {
    /// Cell index of this chunk (index into the cell buffers:
    /// [`HexSphere::cell_center`], [`HexSphere::cell_neighbors`], …).
    ///
    /// ```
    /// use game_engine::hexsphere::HexSphere;
    ///
    /// let body = HexSphere::generate(1, 2.0);
    /// assert_eq!(body.chunk_id(3).index(), 3);
    /// ```
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Hex-dominant geodesic dual mesh: 10·4^N+2 cells (all hexagons except
/// exactly 12 pentagons), the base representation for a spherical body.
///
/// Cell `i`'s center is [`HexSphere::cell_center`]; its corners are the
/// centroids of the adjacent primal triangles, projected back onto the
/// sphere; its neighbors are listed counter-clockwise seen from outside.
///
/// ```
/// use game_engine::hexsphere::HexSphere;
///
/// let body = HexSphere::generate(2, 1.0);
/// assert_eq!(body.cell_count(), 10 * 4usize.pow(2) + 2);
/// assert_eq!(body.pentagon_count(), 12);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct HexSphere {
    subdivisions: u32,
    radius: f32,
    /// Primal vertices = cell centers, on the sphere.
    positions: Vec<[f32; 3]>,
    /// Cyclic neighbor ids per cell (CCW from outside), `MISSING`-padded.
    neighbors: Vec<[u32; 6]>,
    /// Dual vertices = projected primal-triangle centroids, on sphere.
    corners: Vec<[f32; 3]>,
    /// Corner ids per cell in neighbor order, `MISSING`-padded.
    rings: Vec<[u32; 6]>,
}

impl HexSphere {
    /// Generates the mesh: icosahedron → subdivide → relax → dual.
    ///
    /// Pure and deterministic: same `(subdivisions, radius)` yields
    /// bit-identical output on every platform.
    ///
    /// # Panics
    ///
    /// Panics if `radius` is not positive and finite.
    pub fn generate(subdivisions: u32, radius: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "hexsphere radius must be positive and finite, got {radius}"
        );
        let (mut verts, mut faces) = icosahedron(radius);
        subdivide(&mut verts, &mut faces, subdivisions, radius);
        let adjacency = adjacency(&faces, verts.len());
        relax(&mut verts, &adjacency, radius);
        let (neighbors, corners, rings) = build_dual(&verts, &faces, &adjacency, radius);
        Self {
            subdivisions,
            radius,
            positions: verts.iter().map(Vec3::to_array).collect(),
            neighbors,
            corners: corners.iter().map(Vec3::to_array).collect(),
            rings,
        }
    }

    /// Subdivision level this mesh was generated with.
    pub fn subdivisions(&self) -> u32 {
        self.subdivisions
    }

    /// Sphere radius this mesh was generated with.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Number of cells (= primal vertices = 10·4^N+2).
    pub fn cell_count(&self) -> usize {
        self.positions.len()
    }

    /// Typed chunk identity of cell `cell`: 1 cell = 1 chunk (see
    /// [`ChunkId`]). The returned id's [`ChunkId::index`] is `cell`
    /// itself; the type marks it as the stable M2 streaming key.
    ///
    /// ```
    /// use game_engine::hexsphere::HexSphere;
    ///
    /// let body = HexSphere::generate(2, 1.0);
    /// assert_eq!(body.chunk_id(0).index(), 0);
    /// assert_eq!(body.chunk_count(), body.cell_count());
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn chunk_id(&self, cell: u32) -> ChunkId {
        assert!(
            (cell as usize) < self.positions.len(),
            "hexsphere chunk {cell} out of range ({} cells)",
            self.positions.len()
        );
        ChunkId(cell)
    }

    /// Number of chunks (= [`HexSphere::cell_count`]).
    ///
    /// ```
    /// use game_engine::hexsphere::HexSphere;
    ///
    /// let body = HexSphere::generate(1, 1.0);
    /// assert_eq!(body.chunk_count(), 10 * 4 + 2);
    /// ```
    pub fn chunk_count(&self) -> usize {
        self.cell_count()
    }

    /// Number of corner vertices (= primal triangles = 20·4^N).
    pub fn corner_count(&self) -> usize {
        self.corners.len()
    }

    /// Center of a cell (on the sphere).
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn cell_center(&self, cell: u32) -> [f32; 3] {
        self.positions[cell as usize]
    }

    /// Neighbor cell ids in counter-clockwise order (seen from outside).
    /// Pentagons yield 5, hexagons 6. The sentinel is never yielded.
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn cell_neighbors(&self, cell: u32) -> impl ExactSizeIterator<Item = u32> + '_ {
        let row = &self.neighbors[cell as usize];
        let count = row.iter().position(|&n| n == MISSING).unwrap_or(6);
        row.iter().copied().take(count)
    }

    /// Neighbor count of a cell: 5 for pentagons, 6 for hexagons.
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn cell_neighbor_count(&self, cell: u32) -> usize {
        self.cell_neighbors(cell).len()
    }

    /// Whether a cell is one of the exactly 12 pentagons.
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn is_pentagon(&self, cell: u32) -> bool {
        self.neighbors[cell as usize][5] == MISSING
    }

    /// Number of pentagon cells (always exactly 12).
    pub fn pentagon_count(&self) -> usize {
        self.neighbors
            .iter()
            .filter(|row| row[5] == MISSING)
            .count()
    }

    /// Corner positions of a cell, in neighbor order (on the sphere).
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn cell_corners(&self, cell: u32) -> impl ExactSizeIterator<Item = [f32; 3]> + '_ {
        let row = &self.rings[cell as usize];
        let count = row.iter().position(|&c| c == MISSING).unwrap_or(6);
        row.iter()
            .copied()
            .take(count)
            .map(|corner| self.corners[corner as usize])
    }

    /// Corner ids of a cell, in neighbor order. Index into the
    /// [`HexSphere::corner_positions`] buffer — the renderer's index
    /// source (fan-triangulate center + ring per cell).
    ///
    /// # Panics
    ///
    /// Panics if `cell` is out of range.
    pub fn cell_corner_ids(&self, cell: u32) -> impl ExactSizeIterator<Item = u32> + '_ {
        let row = &self.rings[cell as usize];
        let count = row.iter().position(|&c| c == MISSING).unwrap_or(6);
        row.iter().copied().take(count)
    }

    /// Position of a corner vertex (on the sphere).
    ///
    /// # Panics
    ///
    /// Panics if `corner` is out of range.
    pub fn corner_position(&self, corner: u32) -> [f32; 3] {
        self.corners[corner as usize]
    }

    /// Flat cell-center buffer, ready for GPU upload.
    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// Flat corner buffer, ready for GPU upload.
    pub fn corner_positions(&self) -> &[[f32; 3]] {
        &self.corners
    }

    /// Deterministic FNV-1a hash over the quantized mesh (format version,
    /// level, radius bits, quantized positions/neighbors/corners/rings).
    /// Identical for identical inputs on every platform.
    pub fn mesh_hash(&self) -> u64 {
        const FNV_OFFSET: u64 = 14_695_981_039_345_656_037;
        const FNV_PRIME: u64 = 1_099_511_628_211;
        let mut hash = FNV_OFFSET;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        };
        mix(&GEOMETRY_VERSION.to_le_bytes());
        mix(&self.subdivisions.to_le_bytes());
        mix(&self.radius.to_bits().to_le_bytes());
        let scale = HASH_QUANTUM / self.radius;
        for point in self.positions.iter().chain(&self.corners) {
            for axis in quantize(*point, scale) {
                mix(&axis.to_le_bytes());
            }
        }
        for row in self.neighbors.iter().chain(&self.rings) {
            for id in row {
                mix(&id.to_le_bytes());
            }
        }
        hash
    }

    /// Documented storage footprint in bytes (length-based, not capacity).
    pub fn memory_bytes(&self) -> usize {
        self.positions.len() * 12
            + self.neighbors.len() * 24
            + self.corners.len() * 12
            + self.rings.len() * 24
            + 8
    }
}

/// Number of base icosahedron faces: every UV island map has 20 islands.
pub const BASE_FACE_COUNT: usize = 20;

/// Base icosahedron faces (CCW outward), independent of radius or
/// subdivision level. Index `k` is the island id used by
/// [`crate::render::uv`] and [`base_face_ids`].
pub fn base_faces() -> Vec<[u32; 3]> {
    icosahedron(1.0).1
}

/// Base icosahedron vertices on the unit sphere, in the same indexing as
/// [`base_faces`]. Barycentric UV lookups project relaxed mesh points
/// against these unit triangles (scale-invariant).
pub fn base_vertices() -> Vec<[f32; 3]> {
    icosahedron(1.0).0.iter().map(Vec3::to_array).collect()
}

/// Base-face id per final primal face after `subdivisions` levels:
/// `result[f]` is the island of primal face `f` (dual corner `f`).
///
/// Tracks ancestry through the same split order as [`subdivide`] (each
/// child inherits its parent id; shared-edge midpoint dedup is identical),
/// so ids stay aligned with [`HexSphere`] corner buffers. Deterministic:
/// ordered edge map, fixed child order.
pub fn base_face_ids(subdivisions: u32) -> Vec<u32> {
    let (mut verts, mut faces) = icosahedron(1.0);
    let mut ids: Vec<u32> = (0..faces.len() as u32).collect();
    for _ in 0..subdivisions {
        let mut midpoints: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut midpoint = |a: u32, b: u32, verts: &mut Vec<Vec3>| -> u32 {
            let key = (a.min(b), a.max(b));
            *midpoints.entry(key).or_insert_with(|| {
                let m = ((verts[a as usize] + verts[b as usize]) * 0.5).normalize();
                verts.push(m);
                verts.len() as u32 - 1
            })
        };
        let mut next = Vec::with_capacity(faces.len() * 4);
        let mut next_ids = Vec::with_capacity(ids.len() * 4);
        for (face, &id) in faces.iter().zip(ids.iter()) {
            let &[a, b, c] = face;
            let mab = midpoint(a, b, &mut verts);
            let mbc = midpoint(b, c, &mut verts);
            let mca = midpoint(c, a, &mut verts);
            next.push([a, mab, mca]);
            next.push([b, mbc, mab]);
            next.push([c, mca, mbc]);
            next.push([mab, mbc, mca]);
            next_ids.extend_from_slice(&[id, id, id, id]);
        }
        faces = next;
        ids = next_ids;
        for v in verts.iter_mut() {
            *v = v.normalize();
        }
    }
    ids
}

/// Quantizes a position to fixed-point millionths of the radius.
fn quantize(point: [f32; 3], scale: f32) -> [i32; 3] {
    [
        (point[0] * scale).round() as i32,
        (point[1] * scale).round() as i32,
        (point[2] * scale).round() as i32,
    ]
}

/// Canonical icosahedron: 12 vertices, 20 outward (CCW) faces.
fn icosahedron(radius: f32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0f32.sqrt()) * 0.5;
    let raw = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let verts = raw
        .iter()
        .map(|&p| Vec3::from_array(p).normalize() * radius)
        .collect();
    #[rustfmt::skip]
    let faces = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    (verts, faces)
}

/// Splits every triangle into 4 via normalized edge midpoints, `levels`
/// times; projects all vertices onto the sphere after each level.
/// Midpoints are deduplicated through an ordered edge map, so output
/// order never depends on hash iteration.
fn subdivide(verts: &mut Vec<Vec3>, faces: &mut Vec<[u32; 3]>, levels: u32, radius: f32) {
    for _ in 0..levels {
        let mut midpoints: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut midpoint = |a: u32, b: u32, verts: &mut Vec<Vec3>| -> u32 {
            let key = (a.min(b), a.max(b));
            *midpoints.entry(key).or_insert_with(|| {
                let m = ((verts[a as usize] + verts[b as usize]) * 0.5).normalize() * radius;
                verts.push(m);
                verts.len() as u32 - 1
            })
        };
        let mut next = Vec::with_capacity(faces.len() * 4);
        for &[a, b, c] in faces.iter() {
            let mab = midpoint(a, b, verts);
            let mbc = midpoint(b, c, verts);
            let mca = midpoint(c, a, verts);
            next.push([a, mab, mca]);
            next.push([b, mbc, mab]);
            next.push([c, mca, mbc]);
            next.push([mab, mbc, mca]);
        }
        *faces = next;
        for v in verts.iter_mut() {
            *v = v.normalize() * radius;
        }
    }
}

/// Sorted, deduplicated neighbor list per vertex.
fn adjacency(faces: &[[u32; 3]], vert_count: usize) -> Vec<Vec<u32>> {
    let mut adjacency = vec![Vec::new(); vert_count];
    for &[a, b, c] in faces {
        adjacency[a as usize].extend([b, c]);
        adjacency[b as usize].extend([c, a]);
        adjacency[c as usize].extend([a, b]);
    }
    for neighbors in adjacency.iter_mut() {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    adjacency
}

/// Jacobi-style Lloyd relaxation: every vertex moves to its neighbor
/// centroid, reprojected onto the sphere. Simultaneous update keeps the
/// pass independent of vertex order.
fn relax(verts: &mut Vec<Vec3>, adjacency: &[Vec<u32>], radius: f32) {
    for _ in 0..RELAX_ITERATIONS {
        let mut next = verts.clone();
        for (v, neighbors) in adjacency.iter().enumerate() {
            let mut centroid = Vec3::ZERO;
            for &n in neighbors {
                centroid += verts[n as usize];
            }
            centroid /= neighbors.len() as f32;
            next[v] = centroid.normalize() * radius;
        }
        *verts = next;
    }
}

/// Builds the dual mesh: corners (projected face centroids on the
/// sphere), per-cell corner rings, and cyclic neighbor lists (CCW from
/// outside). The ring walk is purely combinatorial — no angles, no
/// platform-dependent math — so topology is bit-identical everywhere.
fn build_dual(
    verts: &[Vec3],
    faces: &[[u32; 3]],
    adjacency: &[Vec<u32>],
    radius: f32,
) -> (Vec<[u32; 6]>, Vec<Vec3>, Vec<[u32; 6]>) {
    let mut edge_faces: BTreeMap<(u32, u32), [u32; 2]> = BTreeMap::new();
    for (f, &[a, b, c]) in faces.iter().enumerate() {
        let f = f as u32;
        for (x, y) in [(a, b), (b, c), (c, a)] {
            let key = (x.min(y), x.max(y));
            match edge_faces.get_mut(&key) {
                Some(pair) => {
                    assert!(pair[1] == u32::MAX, "non-manifold edge in hexsphere mesh");
                    pair[1] = f;
                }
                None => {
                    edge_faces.insert(key, [f, u32::MAX]);
                }
            }
        }
    }

    let corners = faces
        .iter()
        .map(|&[a, b, c]| {
            ((verts[a as usize] + verts[b as usize] + verts[c as usize]) / 3.0).normalize() * radius
        })
        .collect();

    let mut neighbors = Vec::with_capacity(verts.len());
    let mut rings = Vec::with_capacity(verts.len());
    for (v, incident) in adjacency.iter().enumerate() {
        let v = v as u32;
        let start = *incident
            .iter()
            .min()
            .expect("isolated vertex in hexsphere mesh");
        let mut order = Vec::with_capacity(incident.len());
        let mut ring = Vec::with_capacity(incident.len());
        let mut current = start;
        loop {
            order.push(current);
            let key = (v.min(current), v.max(current));
            let [f1, f2] = edge_faces[&key];
            assert!(f2 != u32::MAX, "boundary edge in hexsphere mesh");
            // The two faces sharing an edge traverse it in opposite
            // directions; exactly one follows v -> current.
            let face = if follows(faces[f1 as usize], v, current) {
                f1
            } else {
                debug_assert!(follows(faces[f2 as usize], v, current));
                f2
            };
            ring.push(face);
            current = after(faces[face as usize], current);
            if current == start {
                break;
            }
            assert!(
                order.len() <= incident.len(),
                "ring walk did not close around cell {v}"
            );
        }
        assert_eq!(order.len(), incident.len(), "cell {v} lost neighbors");
        let mut neighbor_row = [MISSING; 6];
        let mut ring_row = [MISSING; 6];
        neighbor_row[..order.len()].copy_from_slice(&order);
        ring_row[..ring.len()].copy_from_slice(&ring);
        neighbors.push(neighbor_row);
        rings.push(ring_row);
    }
    (neighbors, corners, rings)
}

/// Whether `face` contains `a` immediately followed by `b` (cyclically).
fn follows(face: [u32; 3], a: u32, b: u32) -> bool {
    (face[0] == a && face[1] == b)
        || (face[1] == a && face[2] == b)
        || (face[2] == a && face[0] == b)
}

/// The vertex cyclically following `a` in `face`.
fn after(face: [u32; 3], a: u32) -> u32 {
    if face[0] == a {
        face[1]
    } else if face[1] == a {
        face[2]
    } else {
        debug_assert_eq!(face[2], a);
        face[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Instant;

    /// Committed expected hash for the default mesh (N=6, R=1.0).
    /// Pinned 2026-09-14 on first green implementation; any change fails
    /// loudly and must be justified as an intentional geometry change
    /// (bump GEOMETRY_VERSION, never silent drift).
    const EXPECTED_HASH_N6_R1: u64 = 11_459_543_604_394_007_386;

    fn expected_counts(n: u32) -> (usize, usize) {
        let cells = 10 * 4usize.pow(n) + 2;
        let corners = 20 * 4usize.pow(n);
        (cells, corners)
    }

    #[test]
    fn counts_match_formulas() {
        for n in 0..=6 {
            let mesh = HexSphere::generate(n, 1.0);
            let (cells, corners) = expected_counts(n);
            assert_eq!(mesh.cell_count(), cells, "N={n}");
            assert_eq!(mesh.corner_count(), corners, "N={n}");
        }
    }

    #[test]
    fn chunk_identity_matches_cells() {
        for n in 0..=4 {
            let mesh = HexSphere::generate(n, 1.0);
            assert_eq!(mesh.chunk_count(), mesh.cell_count(), "N={n}");
            for cell in 0..mesh.cell_count() as u32 {
                let id = mesh.chunk_id(cell);
                assert_eq!(id.index(), cell, "N={n}");
                assert_eq!(id, mesh.chunk_id(cell), "N={n} cell {cell}");
                if cell > 0 {
                    assert!(mesh.chunk_id(cell - 1) < id, "N={n} cell {cell}");
                }
            }
        }
    }

    #[test]
    fn chunk_ids_cover_all_neighbors() {
        // Every neighbor link is addressable as a chunk id (streaming
        // walks this graph; ids must name real cells on both ends).
        let mesh = HexSphere::generate(3, 1.0);
        for cell in 0..mesh.cell_count() as u32 {
            let id = mesh.chunk_id(cell);
            assert_eq!(id.index(), cell);
            for neighbor in mesh.cell_neighbors(cell) {
                assert!((neighbor as usize) < mesh.chunk_count());
                let _ = mesh.chunk_id(neighbor);
            }
        }
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn chunk_id_rejects_unknown_cell() {
        let mesh = HexSphere::generate(1, 1.0);
        let _ = mesh.chunk_id(mesh.cell_count() as u32);
    }

    #[test]
    fn exactly_twelve_pentagons() {
        for n in 0..=6 {
            let mesh = HexSphere::generate(n, 1.0);
            assert_eq!(mesh.pentagon_count(), 12, "N={n}");
            for cell in 0..mesh.cell_count() as u32 {
                let count = mesh.cell_neighbor_count(cell);
                assert!(
                    count == 5 || count == 6,
                    "N={n} cell {cell} has {count} neighbors"
                );
                assert_eq!(mesh.is_pentagon(cell), count == 5, "N={n} cell {cell}");
                assert_eq!(mesh.cell_corners(cell).len(), count, "N={n} cell {cell}");
            }
        }
    }

    #[test]
    fn euler_characteristic_holds() {
        // The cell mesh as a polyhedron: V = corners, E = neighbor-links
        // / 2, F = cells. Closed genus-0 sphere ⟺ V − E + F = 2.
        for n in 0..=6 {
            let mesh = HexSphere::generate(n, 1.0);
            let links: usize = (0..mesh.cell_count() as u32)
                .map(|cell| mesh.cell_neighbor_count(cell))
                .sum();
            let euler = mesh.corner_count() as i64 - links as i64 / 2 + mesh.cell_count() as i64;
            assert_eq!(euler, 2, "N={n}");
            assert_eq!(links % 2, 0, "N={n}: every link counted twice");
        }
    }

    #[test]
    fn neighbor_graph_is_symmetric() {
        let mesh = HexSphere::generate(4, 2.5);
        for cell in 0..mesh.cell_count() as u32 {
            for neighbor in mesh.cell_neighbors(cell) {
                assert!(
                    mesh.cell_neighbors(neighbor).any(|back| back == cell),
                    "cell {cell} → {neighbor} is one-way"
                );
            }
        }
    }

    #[test]
    fn every_edge_has_two_faces_and_outward_normals() {
        // Rebuilds primal topology straight from the mesh buffers and
        // checks manifoldness + outward winding independently.
        let mesh = HexSphere::generate(3, 1.0);
        let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for cell in 0..mesh.cell_count() as u32 {
            let ring: Vec<u32> = mesh.cell_corner_ids(cell).collect();
            for (i, &corner) in ring.iter().enumerate() {
                let a = mesh.cell_center(cell);
                let b = mesh.corner_position(corner);
                let c = mesh.corner_position(ring[(i + 1) % ring.len()]);
                let normal = Vec3::from_array([b[0] - a[0], b[1] - a[1], b[2] - a[2]])
                    .cross(Vec3::from_array([c[0] - a[0], c[1] - a[1], c[2] - a[2]]));
                assert!(normal.length_squared() > 0.0, "degenerate fan tri");
                let outward = Vec3::from_array(a).normalize();
                assert!(normal.dot(outward) > 0.0, "inward winding at cell {cell}");
            }
            for window in ring.windows(2) {
                *edge_count
                    .entry((window[0].min(window[1]), window[0].max(window[1])))
                    .or_insert(0) += 1;
            }
            let last = (*ring.last().unwrap(), ring[0]);
            *edge_count
                .entry((last.0.min(last.1), last.0.max(last.1)))
                .or_insert(0) += 1;
        }
        assert!(edge_count.values().all(|&count| count == 2));
    }

    #[test]
    fn geometry_sits_on_sphere_without_nan() {
        let radius = 3.0;
        let mesh = HexSphere::generate(5, radius);
        for point in mesh.positions().iter().chain(mesh.corner_positions()) {
            assert!(point.iter().all(|axis| axis.is_finite()));
            let length = Vec3::from_array(*point).length();
            assert!(
                (length - radius).abs() < 1e-3,
                "off-sphere point: length {length}"
            );
        }
    }

    #[test]
    fn icosahedron_is_outward_and_unit() {
        let (verts, faces) = icosahedron(2.0);
        assert_eq!(verts.len(), 12);
        assert_eq!(faces.len(), 20);
        for &[a, b, c] in &faces {
            let normal = (verts[b as usize] - verts[a as usize])
                .cross(verts[c as usize] - verts[a as usize]);
            assert!(normal.length_squared() > 0.0);
            let centroid = (verts[a as usize] + verts[b as usize] + verts[c as usize]) / 3.0;
            assert!(normal.dot(centroid) > 0.0, "inward icosahedron face");
        }
        for v in &verts {
            assert!((v.length() - 2.0).abs() < 1e-6);
        }
    }

    #[test]
    fn deterministic_across_repeated_runs() {
        let first = HexSphere::generate(6, 1.0);
        for _ in 0..3 {
            let again = HexSphere::generate(6, 1.0);
            assert_eq!(again, first);
            assert_eq!(again.mesh_hash(), first.mesh_hash());
        }
    }

    #[test]
    fn committed_hash_matches() {
        assert_eq!(HexSphere::generate(6, 1.0).mesh_hash(), EXPECTED_HASH_N6_R1);
    }

    #[test]
    fn generation_stats_report() {
        let started = Instant::now();
        let mesh = HexSphere::generate(DEFAULT_SUBDIVISIONS, 1.0);
        let elapsed = started.elapsed();
        eprintln!(
            "hexsphere N={}: {} cells, {} corners, {} bytes ({:.2} MiB), generated in {:?}",
            DEFAULT_SUBDIVISIONS,
            mesh.cell_count(),
            mesh.corner_count(),
            mesh.memory_bytes(),
            mesh.memory_bytes() as f64 / 1_048_576.0,
            elapsed
        );
        assert!(
            mesh.memory_bytes() < 8_000_000,
            "single-digit MB budget blown: {} bytes",
            mesh.memory_bytes()
        );
    }

    #[test]
    #[should_panic(expected = "radius must be positive")]
    fn rejects_non_positive_radius() {
        HexSphere::generate(2, 0.0);
    }

    #[test]
    fn base_tables_cover_twenty_faces() {
        assert_eq!(BASE_FACE_COUNT, 20);
        let faces = base_faces();
        let verts = base_vertices();
        assert_eq!(faces.len(), 20);
        assert_eq!(verts.len(), 12);
        for face in &faces {
            for &v in face {
                assert!((v as usize) < verts.len(), "face {face:?}");
            }
        }
    }

    #[test]
    fn base_face_ids_align_with_corners() {
        for n in 0..=4 {
            let mesh = HexSphere::generate(n, 1.0);
            let ids = base_face_ids(n);
            assert_eq!(ids.len(), mesh.corner_count(), "N={n}");
            assert!(ids.iter().all(|&id| (id as usize) < BASE_FACE_COUNT));
            // Every island owns at least one corner.
            let mut seen = [false; BASE_FACE_COUNT];
            for &id in &ids {
                seen[id as usize] = true;
            }
            assert!(seen.iter().all(|&s| s), "N={n}");
        }
    }

    #[test]
    fn base_face_ids_are_deterministic() {
        assert_eq!(base_face_ids(3), base_face_ids(3));
    }
}
