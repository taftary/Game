# Issue report — 3D checker squares distorted on the sphere

Parent specs: [`specs.md`](specs.md). Status `draft → in-progress` here;
fix plan in [`plan.md`](plan.md).

## Investigation

`crates/debug/src/main.rs` `FILL_FRAG` mode 3 (`m == 3`) is
`floor(v_uv * pc.density)` parity over the icosa-net UVs. In the 3D view
`v_uv` is the `PlanetVertex.uv` from `engine::render::uv::build_debug_uv`:
each direction is barycentric-projected onto its base face's **chord**
triangle and carried affinely into the net slot. On the sphere that mapping
is not angle- or area-preserving — grid lines that are straight in the net
bend/shear on the curved face, each island samples a differently-rotated
piece of the global `[0,1]²` grid, and seam cells contribute a single UV
per center so their fans stretch one checker patch across the cut. All
three effects are inherent to a net-space checker; no net tweak removes
them.

Reading http://www.paulbourke.net/panorama/icosahedral/ gives the fix used
by the reference itself: each icosahedron face is a gnomonic (perspective)
projection plane, and the per-face pattern is evaluated in that plane.
For a convex origin-centered polyhedron the containing face of a direction
`d` is `argmax_f dot(d, N_f)` (supporting plane), and the gnomonic point
is `q = d · (H / dot(d, N_f))` with apothem `H`. That yields
equal-spacing-along-rows/columns within each face and identical scale on
all 20 faces (same world units) — minimal distortion for a sphere checker
(Bourke: less stretching than cube-map corners, 60° per edge vs 90°).

## Fix (to implement)

- New pure engine module `render::checker`: `GnomonicTable` derived from
  `hexsphere::{base_faces, base_vertices}` (normals, in-plane centroids,
  `U = normalize(v_b−v_a)` / `V = cross(N, U)` bases, apothem, inv edge
  length), `gnomonic_uv(dir)` mirror of the shader math, and
  `glsl_const_block()` emitting the exact literal block pasted into
  `FILL_FRAG` (single source of truth, drift-guard test).
- `FILL_FRAG` mode 3: per-fragment face select → gnomonic project →
  face-local checker with `density` ≈ squares per face edge. Both fill and
  flat pipelines share the shader, so the flat view shows the same pattern
  (Bourke unwrapped map) with zero extra plumbing; push constants (80 B),
  vertex layouts, and all other modes untouched; seams overlay unchanged.
- Tests: table orthonormality/apothem consistency; every corner direction
  at N=0..=3 selects its `base_face_ids` ancestry face; every vertex
  projects inside its selected face; determinism; GLSL block == engine
  table (debug crate).

## Verification

- `cargo test -p game_engine --lib` + `cargo test -p game_debug`
  (compile test catches naga rejection of the const-table form; fallback:
  switch-chain accessors, same math).
- Full gates in `plan.md` DoD table.
- IMPORTANT rebuild note (from the previous issue): `game_debug.exe`
  cannot relink while the viewer runs (Windows os error 5) — close the
  viewer first, then rebuild and press `3`.

## Refinement round 2 (red-area discontinuity)

User visual after round 1: squares are uniform per face, but the grid
visibly jumps at the red seam bands. Diagnosis: round 1 gave each face an
independent frame (origin at its own centroid, U along its own first
edge), so grids are rotated + phase-shifted at every shared edge — the
dog-leg Bourke notes for icosahedral maps.

A globally continuous square grid on an icosahedron is mathematically
impossible: 5 faces meet at each vertex (60° holonomy deficit) while a
square grid is invariant only under 90° rotations. The best achievable:
make the grid continuous across the 19 edges of a spanning tree of
faces and concentrate the unavoidable mismatch on the remaining 11 edges.
We reuse the UV net's own walk tree (`render::uv::walk_tree_edges` —
newly exported), so the 11 fault edges are exactly the flat net's cuts:
3D fault lines and island boundaries coincide, and the flat view shows
one continuous grid across all islands.

Implementation: each child face's frame `(origin, U, V)` is the parent
frame rotated about the shared edge line (the face planes' intersection)
into the child's plane — signed dihedral angle via
`atan2(axis·(N_p×N_c), N_p·N_c)`, Rodrigues via `glam::Mat3::from_axis_angle`.
Root face 0 keeps centroid + first-edge basis. Continuity proven by
`walk_tree_frames_are_continuous_across_shared_edges` (identical local
coordinates from both sides at 3 sample points per tree edge). Propagation
chains up to depth 9 keep f32 error ~1e-5 — below the 1e-4 test
tolerance, invisible at any density ≤ 32.

Independent recheck (per user request, plan reviewed externally): round-1
math verified correct (argmax face = supporting plane of a convex solid;
`q = d·H/dot(d,N)` exact gnomonic; frame orthonormal; density ≈ squares
per edge). No defect found in the projection itself — the remaining
artifact was the frame discontinuity addressed above.

## Refinement round 3 (only squares — cube domain)

User visual after round 2: much better, but the icosa vertices (north /
south) still show triangular pinwheels and the 11 cut edges still show
mismatched squares. Both are the round-2 report's proven impossibility —
no per-face icosahedral assignment can remove them. The user requires
*only squares* (foundation for later game logic), so the checker domain
moved to the **cube**: its 90° face corners are the one case compatible
with square-grid symmetry (Bourke cubemap; equiangular/EAC remap for
even distribution).

Even on a cube, perfect 2-color alternation is impossible: 3 cells meet
pairwise-adjacent at each of the 8 vertices (odd cycle), so each vertex
forces ≥1 same-color grid line. The minimum defect — 4 fault edges in a
perfect matching, other 8 edges alternating — is achieved by searching
the 4^6 per-face 90° rotations × 2^6 color flips (the flip is the phase
degree of freedom cross-face alternation needs; without it the strict
search is unsatisfiable — found by failing tests, not guesses).
Criterion: constant parity difference along each edge at both density
parities (grid lines always coincide) + density-independent edge labels
+ exactly-one-fault per vertex. Proven by
`checker_alternates_consistently_across_all_cube_edges` (constant
difference at all stations, vertex condition, matching count — at every
density 2..=32).

Mapping: direction → major-axis cube face → gnomonic `q = d/dot(d,N)`
(apothem 1) → face-centered `[-1,1]²` → `atan` (EAC) → `density`
squares per edge. The checker is a pure function of direction: it flows
across the icosa seam overlay (red bands) untouched — mesh topology and
checker domain are independent, and both views share the one shader.

Also fixed en route (test-driven): propagated-origin frames dropped
(origins wander off-face; only centered axes matter), and a first-max /
last-max tie divergence between the shader and the Rust `max_by`
expectation on face-border directions.
