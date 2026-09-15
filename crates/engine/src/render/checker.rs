//! Cube-domain checker table for the debug sphere viewer
//! (`plans/sphere-uv-debug/issue-2026-09-15-0817-3d-checker-gnomonic`,
//! round 3).
//!
//! Round 2 proved the limit of icosahedral checkers: a square grid
//! continuous across every icosa edge is mathematically impossible (5
//! faces per vertex = 60° holonomy; square grids survive only 90°
//! rotations). The user requires *only squares* — so the checker domain
//! is the **cube**, whose 90° face corners are compatible with the grid.
//! Even on a cube, perfect 2-color alternation is impossible: 3 cells
//! meet pairwise-adjacent at each of the 8 vertices (odd cycle), so each
//! vertex forces exactly one same-color grid line — the minimum defect
//! is 4 "fault edges" forming a perfect matching, with the other 8 edges
//! alternating like a true checkerboard. [`checker_table`] searches the
//! 4^6 per-face 90° rotations for an assignment with exactly that fault
//! pattern, valid at every density 2..=32 (proven by
//! `checker_alternates_consistently_across_all_cube_edges`).
//!
//! Mapping (Bourke cubemap + equiangular variant): a sphere direction
//! selects its face by max dot (= major axis), projects gnomonically
//! onto the face plane (`q = d / dot(d, N)`, apothem 1), and the
//! face-centered `[-1,1]²` coordinates are remapped by `atan`
//! (equiangular cubemap) before tiling, so squares stay evenly
//! distributed instead of inflating toward face corners. The checker is
//! a pure function of direction: it flows across the icosa seam overlay
//! untouched — mesh topology and checker domain are independent.
//!
//! [`CheckerTable`] constants are emitted by [`glsl_const_block`] and
//! embedded in the debug `FILL_FRAG` shader (the debug crate asserts
//! containment, so the two can never drift apart).
//!
//! Pure + deterministic: same code gives bit-identical output on every
//! platform (fixed face order, fixed search order, no hashing).

use glam::Vec3;

/// Number of cube faces: the checker domain.
pub const CUBE_FACE_COUNT: usize = 6;

/// Cube face normals (±X, ±Y, ±Z), fixed order. Index = face id used by
/// the shader table.
pub const CUBE_NORMALS: [[f32; 3]; CUBE_FACE_COUNT] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];

/// Per-face checker axes for the cube domain, index = face id in
/// [`CUBE_NORMALS`]. `(u, v, normal)` is right-handed orthonormal, chosen
/// by the fault-matching search (see module docs); `flip` inverts the
/// face's checker colors without moving grid lines (the phase degree of
/// freedom cross-face alternation needs). Face planes are `±x/±y/±z = 1`
/// (apothem 1) and coordinates are face-centered in `[-1,1]²`, so the
/// table carries no origins or scale constants.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckerTable {
    /// Outward unit face normals (axis-aligned).
    pub normal: [[f32; 3]; CUBE_FACE_COUNT],
    /// In-plane unit basis (grid x direction).
    pub u: [[f32; 3]; CUBE_FACE_COUNT],
    /// In-plane unit basis (grid y direction).
    pub v: [[f32; 3]; CUBE_FACE_COUNT],
    /// Color phase per face (0.0 or 1.0; added to the checker parity).
    pub flip: [f32; CUBE_FACE_COUNT],
}

/// Canonical right-handed frames before the rotation search: per face a
/// deterministic in-plane axis pair (each an axis-aligned unit pair).
fn canonical_frame(face: usize) -> ([f32; 3], [f32; 3]) {
    match face {
        0 => ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        1 => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        2 => ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        3 => ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        4 => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        _ => ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    }
}

/// All 12 cube edges as `(face_a, face_b)`: every perpendicular normal
/// pair.
fn cube_edge_faces() -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for (a, na) in CUBE_NORMALS.iter().enumerate() {
        for (b, nb) in CUBE_NORMALS.iter().enumerate().skip(a + 1) {
            if Vec3::from_array(*na).dot(Vec3::from_array(*nb)).abs() < 1e-6 {
                edges.push((a, b));
            }
        }
    }
    edges
}

/// The 8 cube vertices as `(vertex, [face_a, face_b, face_c])`: sign
/// combos with their three incident faces.
fn cube_vertex_faces() -> Vec<([f32; 3], [usize; 3])> {
    let mut out = Vec::new();
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                let vertex = Vec3::new(sx, sy, sz);
                let mut faces = [0usize; 3];
                let mut k = 0;
                for (f, n) in CUBE_NORMALS.iter().enumerate() {
                    if (vertex.dot(Vec3::from_array(*n)) - 1.0).abs() < 1e-6 {
                        faces[k] = f;
                        k += 1;
                    }
                }
                debug_assert_eq!(k, 3);
                out.push(([sx, sy, sz], faces));
            }
        }
    }
    out
}

/// One cube edge's geometry for parity sampling.
struct EdgeGeom {
    a: usize,
    b: usize,
    na: Vec3,
    nb: Vec3,
    va: Vec3,
    vb: Vec3,
}

/// Builds the [`CheckerTable`]: canonical frames, then an exhaustive
/// search over per-face 90° rotations × color flips (4^6 × 2^6) for an
/// assignment where (1) every edge has a constant parity difference
/// along its length at both density parities (grid lines always
/// coincide — no mid-edge faults) and (2) every cube vertex touches
/// exactly one same-color fault edge — the provably minimal defect (odd
/// 3-cycles at vertices), i.e. the 4 fault edges form a perfect matching
/// and the other 8 edges alternate like a true checkerboard. Edge labels
/// are required density-independent (identical at d=2 and d=3, which
/// determines all densities) so the pattern holds at every slider value.
/// Deterministic: first match in lexicographic (rotation, flip) order.
pub fn checker_table() -> CheckerTable {
    let edges: Vec<EdgeGeom> = cube_edge_faces()
        .iter()
        .map(|&(a, b)| {
            let (na, nb) = (
                Vec3::from_array(CUBE_NORMALS[a]),
                Vec3::from_array(CUBE_NORMALS[b]),
            );
            let cross = na.cross(nb);
            EdgeGeom {
                a,
                b,
                na,
                nb,
                va: na + nb + cross,
                vb: na + nb - cross,
            }
        })
        .collect();
    let vertices = cube_vertex_faces();
    let edge_index = |a: usize, b: usize| {
        let key = (a.min(b), a.max(b));
        edges
            .iter()
            .position(|e| (e.a, e.b) == key)
            .expect("incident faces share an edge")
    };
    let vertex_ok = |labels: &[bool]| {
        vertices.iter().all(|&(_, faces)| {
            let faults = [
                (faces[0], faces[1]),
                (faces[0], faces[2]),
                (faces[1], faces[2]),
            ]
            .iter()
            .filter(|&&(a, b)| !labels[edge_index(a, b)])
            .count();
            faults == 1
        })
    };
    let stations = [0.31f32, 0.47, 0.73];
    let mut relaxed: Option<([usize; CUBE_FACE_COUNT], [bool; CUBE_FACE_COUNT])> = None;
    for combo in 0..(1usize << (2 * CUBE_FACE_COUNT)) {
        let mut k = [0usize; CUBE_FACE_COUNT];
        for (f, slot) in k.iter_mut().enumerate() {
            *slot = (combo >> (2 * f)) & 3;
        }
        let base = table_for_rotations(&k, &[false; CUBE_FACE_COUNT]);
        // σ-consistency: the parity difference must be constant along
        // each edge; record it per density parity.
        let mut diff = [[false; 12]; 2];
        let mut constant = true;
        for (i, e) in edges.iter().enumerate() {
            for (di, density) in [2u32, 3].iter().enumerate() {
                let d0 = edge_difference(&base, e, stations[0], *density);
                if stations[1..]
                    .iter()
                    .any(|&t| edge_difference(&base, e, t, *density) != d0)
                {
                    constant = false;
                    break;
                }
                diff[di][i] = d0;
            }
            if !constant {
                break;
            }
        }
        if !constant {
            continue;
        }
        // Flip search: flips only toggle edge labels (color phases).
        for fl in 0..(1usize << CUBE_FACE_COUNT) {
            let mut flip = [false; CUBE_FACE_COUNT];
            for (f, slot) in flip.iter_mut().enumerate() {
                *slot = (fl >> f) & 1 == 1;
            }
            let toggle = |i: usize| flip[edges[i].a] ^ flip[edges[i].b];
            let labels_even: Vec<bool> = (0..edges.len()).map(|i| diff[0][i] ^ toggle(i)).collect();
            let labels_odd: Vec<bool> = (0..edges.len()).map(|i| diff[1][i] ^ toggle(i)).collect();
            if labels_even == labels_odd && vertex_ok(&labels_even) {
                return table_for_rotations(&k, &flip); // density-independent
            }
            if relaxed.is_none() && vertex_ok(&labels_even) && vertex_ok(&labels_odd) {
                relaxed = Some((k, flip));
            }
        }
    }
    let (k, flip) = relaxed.expect("cube checker assignment must exist (8^6 search)");
    table_for_rotations(&k, &flip)
}

/// Applies per-face 90° rotations `k[f]·90°` about the face normal to
/// the canonical frames (`u → v → −u → −v`, right-handed preserved) and
/// sets the color flips.
fn table_for_rotations(
    k: &[usize; CUBE_FACE_COUNT],
    flip: &[bool; CUBE_FACE_COUNT],
) -> CheckerTable {
    let mut table = CheckerTable {
        normal: CUBE_NORMALS,
        u: [[0.0; 3]; CUBE_FACE_COUNT],
        v: [[0.0; 3]; CUBE_FACE_COUNT],
        flip: [0.0; CUBE_FACE_COUNT],
    };
    for f in 0..CUBE_FACE_COUNT {
        let (mut u, mut v) = canonical_frame(f);
        for _ in 0..k[f] {
            let next_u = v;
            let next_v = [-u[0], -u[1], -u[2]];
            u = next_u;
            v = next_v;
        }
        table.u[f] = u;
        table.v[f] = v;
        table.flip[f] = if flip[f] { 1.0 } else { 0.0 };
    }
    table
}

/// The parity difference across `e` at station `t` and `density` (true =
/// alternates). Samples step just inside each face, off the shared grid
/// line.
fn edge_difference(table: &CheckerTable, e: &EdgeGeom, t: f32, density: u32) -> bool {
    let x = e.va + (e.vb - e.va) * t;
    let pa = checker_parity(table, e.a, local_uv(table, e.a, x - e.nb * 0.01), density);
    let pb = checker_parity(table, e.b, local_uv(table, e.b, x - e.na * 0.01), density);
    pa != pb
}

/// Checker lookup mirroring the debug fragment shader: selects the face
/// maximizing `dot(dir, normal)` (= major axis; first-max tie-break, so
/// border directions are deterministic) and returns its id plus the
/// gnomonic face-centered coordinates in `[-1,1]²` (pre-`atan`).
pub fn checker_local(table: &CheckerTable, dir: [f32; 3]) -> (u32, [f32; 2]) {
    let d = Vec3::from_array(dir).normalize();
    let mut best = 0usize;
    let mut best_dot = f32::NEG_INFINITY;
    for (f, n) in table.normal.iter().enumerate() {
        let dot = d.dot(Vec3::from_array(*n));
        if dot > best_dot {
            best_dot = dot;
            best = f;
        }
    }
    let n = Vec3::from_array(table.normal[best]);
    let q = d / d.dot(n);
    (best as u32, local_uv(table, best, q))
}

/// Face-centered coordinates of the in-plane point `q` (`[-1,1]²`).
fn local_uv(table: &CheckerTable, face: usize, q: Vec3) -> [f32; 2] {
    let rel = q - Vec3::from_array(table.normal[face]);
    [
        rel.dot(Vec3::from_array(table.u[face])),
        rel.dot(Vec3::from_array(table.v[face])),
    ]
}

/// Checker square indices for face-centered coordinates, mirroring the
/// shader: equiangular remap (`atan`, `[-1,1] → [-π/4,π/4]`) then
/// `density` squares per face edge. Negative indices floor like GLSL.
pub fn checker_square(local: [f32; 2], density: u32) -> [i32; 2] {
    let scale = density as f32 * 2.0 / std::f32::consts::PI;
    let square = |x: f32| ((x.atan() + std::f32::consts::FRAC_PI_4) * scale).floor() as i32;
    [square(local[0]), square(local[1])]
}

/// Two-tone parity of [`checker_square`] plus the face's color `flip`
/// (GLSL `mod` semantics: always 0 or 1, also for negative indices).
pub fn checker_parity(table: &CheckerTable, face: usize, local: [f32; 2], density: u32) -> u32 {
    let [a, b] = checker_square(local, density);
    (a + b + table.flip[face] as i32).rem_euclid(2) as u32
}

/// Emits the GLSL constant block embedded in the debug `FILL_FRAG` shader
/// (`GNO_N/U/V` tables + `GNO_FLIP` color phases + the equiangular
/// constants `GNO_PI4` = π/4 and `GNO_TWO_OVER_PI` = 2/π). Nine-digit
/// scientific literals round-trip through GLSL `float` parsing
/// bit-exactly; the debug crate asserts this block is contained verbatim
/// in the shader source.
pub fn glsl_const_block() -> String {
    let table = checker_table();
    let mut out = String::new();
    for (name, rows) in [
        ("GNO_N", table.normal),
        ("GNO_U", table.u),
        ("GNO_V", table.v),
    ] {
        out.push_str(&format!(
            "const vec3 {name}[{CUBE_FACE_COUNT}] = vec3[{CUBE_FACE_COUNT}]("
        ));
        for (i, r) in rows.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("vec3({:.9e}, {:.9e}, {:.9e})", r[0], r[1], r[2]));
        }
        out.push_str(");\n");
    }
    out.push_str(&format!(
        "const float GNO_FLIP[{CUBE_FACE_COUNT}] = float[{CUBE_FACE_COUNT}]("
    ));
    for (i, f) in table.flip.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{:.9e}", f));
    }
    out.push_str(");\n");
    out.push_str(&format!(
        "const float GNO_PI4 = {:.9e};\n",
        std::f32::consts::FRAC_PI_4
    ));
    out.push_str(&format!(
        "const float GNO_TWO_OVER_PI = {:.9e};\n",
        2.0f32 / std::f32::consts::PI
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hexsphere::HexSphere;

    /// All 12 cube edges as `(face_a, face_b, [vertex_a, vertex_b])`.
    fn cube_edges() -> Vec<EdgeGeom> {
        cube_edge_faces()
            .iter()
            .map(|&(a, b)| {
                let (na, nb) = (
                    Vec3::from_array(CUBE_NORMALS[a]),
                    Vec3::from_array(CUBE_NORMALS[b]),
                );
                let cross = na.cross(nb);
                EdgeGeom {
                    a,
                    b,
                    na,
                    nb,
                    va: na + nb + cross,
                    vb: na + nb - cross,
                }
            })
            .collect()
    }

    #[test]
    fn table_frames_are_orthonormal_and_consistent() {
        let table = checker_table();
        const EPS: f32 = 1e-6;
        for f in 0..CUBE_FACE_COUNT {
            let n = Vec3::from_array(table.normal[f]);
            let (uu, vv) = (Vec3::from_array(table.u[f]), Vec3::from_array(table.v[f]));
            assert!((uu.length() - 1.0).abs() < EPS, "face {f}");
            assert!((vv.length() - 1.0).abs() < EPS, "face {f}");
            assert!(n.dot(uu).abs() < EPS, "face {f}");
            assert!(n.dot(vv).abs() < EPS, "face {f}");
            assert!(uu.dot(vv).abs() < EPS, "face {f}");
            assert!((uu.cross(vv) - n).length() < EPS, "face {f}");
            // Cube vertices sit at (±1, ±1) local in every incident face
            // (axis-aligned frames): vertices are grid corners.
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    for sz in [-1.0f32, 1.0] {
                        let vertex = Vec3::new(sx, sy, sz);
                        if (vertex.dot(n) - 1.0).abs() > 1e-5 {
                            continue;
                        }
                        let local = local_uv(&table, f, vertex);
                        assert!(
                            (local[0].abs() - 1.0).abs() < EPS
                                && (local[1].abs() - 1.0).abs() < EPS,
                            "vertex {vertex:?} face {f} local {local:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn checker_alternates_consistently_across_all_cube_edges() {
        // THE round-3 proof, at every slider density: (a) the parity
        // difference across every edge is constant at all stations
        // (grid lines always coincide — no mid-edge faults);
        // (b) every cube vertex touches exactly one same-color fault
        // edge (the provably minimal defect: odd 3-cycles at vertices),
        // so the 4 fault edges form a perfect matching and the other 8
        // edges alternate like a true checkerboard.
        let table = checker_table();
        let edges = cube_edges();
        assert_eq!(edges.len(), 12);
        let vertices = cube_vertex_faces();
        for density in 2..=32u32 {
            let alternates: Vec<bool> = edges
                .iter()
                .map(|e| {
                    let mut difference = None;
                    // Stations never hit grid lines: along-edge
                    // coordinate 2t−1 is rational while line stations
                    // sit at tangents.
                    for t in [0.37f32, 0.61] {
                        let d = edge_difference(&table, e, t, density);
                        match difference {
                            None => difference = Some(d),
                            Some(prev) => assert_eq!(
                                prev, d,
                                "edge {}-{} difference flips at t={t} density={density}",
                                e.a, e.b
                            ),
                        }
                    }
                    difference.expect("sampled")
                })
                .collect();
            for &(_, faces) in &vertices {
                let key = |a: usize, b: usize| (a.min(b), a.max(b));
                let faults = [
                    (faces[0], faces[1]),
                    (faces[0], faces[2]),
                    (faces[1], faces[2]),
                ]
                .iter()
                .filter(|&&(a, b)| {
                    !alternates[edges
                        .iter()
                        .position(|e| (e.a, e.b) == key(a, b))
                        .expect("edge")]
                })
                .count();
                assert_eq!(faults, 1, "density {density} vertex faces {faces:?}");
            }
            let fault_count = alternates.iter().filter(|&&a| !a).count();
            assert_eq!(fault_count, 4, "density {density} perfect matching");
        }
    }

    #[test]
    fn every_mesh_vertex_projects_inside_its_cube_face() {
        // Every mesh vertex direction selects a cube face (major axis)
        // and projects inside its square (frame-independent check).
        const EPS: f32 = 1e-4;
        let table = checker_table();
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let cells = mesh.cell_count();
            let total = cells + mesh.corner_count();
            for i in 0..total {
                let pos = if i < cells {
                    mesh.cell_center(i as u32)
                } else {
                    mesh.corner_position((i - cells) as u32)
                };
                let (face, _) = checker_local(&table, pos);
                let d = Vec3::from_array(pos).normalize();
                // Selection = a major axis of the direction (border ties
                // break first-max, like the shader — any argmax is a
                // valid major axis).
                let max_dot = (0..CUBE_FACE_COUNT)
                    .map(|f| d.dot(Vec3::from_array(CUBE_NORMALS[f])))
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    (d.dot(Vec3::from_array(table.normal[face as usize])) - max_dot).abs() < 1e-5,
                    "N={n} v{i} face {face}"
                );
                let nrm = Vec3::from_array(table.normal[face as usize]);
                let q = d / d.dot(nrm);
                assert!((q.dot(nrm) - 1.0).abs() < 1e-5, "N={n} v{i}");
                for e in axes {
                    if e.dot(nrm).abs() < 0.5 {
                        assert!(q.dot(e).abs() <= 1.0 + EPS, "N={n} v{i} q {q:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn parity_matches_manual_floor_mod() {
        // The shader formula, recomputed independently (flip included).
        let table = checker_table();
        let local = [0.374, -0.882];
        for face in 0..CUBE_FACE_COUNT {
            for density in [2u32, 7, 8, 31, 32] {
                let scale = density as f32 * 2.0 / std::f32::consts::PI;
                let manual = |x: f32| {
                    let g = (x.atan() + std::f32::consts::FRAC_PI_4) * scale;
                    (g.floor() as i64).rem_euclid(2)
                };
                let expect =
                    ((manual(local[0]) + manual(local[1]) + table.flip[face] as i64) % 2) as u32;
                assert_eq!(
                    checker_parity(&table, face, local, density),
                    expect,
                    "face {face} density {density}"
                );
            }
        }
    }

    #[test]
    fn glsl_block_covers_the_whole_table() {
        let table = checker_table();
        let block = glsl_const_block();
        for (name, rows) in [
            ("GNO_N", table.normal),
            ("GNO_U", table.u),
            ("GNO_V", table.v),
        ] {
            assert!(
                block.contains(&format!("const vec3 {name}[{CUBE_FACE_COUNT}]")),
                "{name} header"
            );
            for r in rows {
                let lit = format!("vec3({:.9e}, {:.9e}, {:.9e})", r[0], r[1], r[2]);
                assert!(block.contains(&lit), "{name} {r:?}");
            }
        }
        assert!(block.contains("const float GNO_PI4 = "));
        assert!(block.contains("const float GNO_TWO_OVER_PI = "));
    }

    #[test]
    fn builds_are_deterministic() {
        assert_eq!(checker_table(), checker_table());
        assert_eq!(glsl_const_block(), glsl_const_block());
    }
}
