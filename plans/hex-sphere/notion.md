# Notion — hex-sphere

## Status

`done` (all DoD criteria checked — see `plan.md` DoD verification)

## Context

World generation is layered (docs/game/universe.md); this feature is the
geometry base layer for every spherical body in the game — planets,
stars, moons, anything spheric. Target platform is mobile-first
(limited CPU/RAM/battery, budgets in docs/techstack/quality.md). For
planets specifically, ADR-002 (planet representation) is still open and
docs/techstack/rendering.md lists the far-field planet as "sphere LOD /
quadtree chunked sphere" — this feature supersedes that line: a
hex-dominant geodesic dual mesh (icosahedron → subdivide → project →
relax → dual) becomes the single sphere representation for BOTH the
gameplay data grid and the renderer's source mesh. Geometry is
seed-independent (only radius R varies per body), so one mesh per (N, R)
is shareable across all bodies; per-seed variety arrives in later
layers. The module is named `engine::hexsphere` — body-agnostic;
`engine::universe` (galaxy/system generation) is a separate concern,
defined later.

## Problem & Needs

- Every spherical body needs a closed, manifold mesh of near-uniform
  polygon cells supporting O(n) passes; uniform hexagons on a sphere are
  impossible (Euler), so the topology must be hex-dominant with exactly
  12 pentagons — the icosahedral Goldberg dual provides this.
- Mobile budgets demand minimal stored data per cell, no heavy physics
  (no fluid sim, no complex erosion), and algorithms that stay O(n).
- Universe rules (universe.md) require deterministic, reproducible
  output across platforms: fixed iteration counts, quantized floats,
  pure functions.
- ADR-002 is unresolved; the renderer has no committed sphere topology.

## Goals

- Pure, deterministic pipeline in a new `engine::hexsphere` module:
  icosahedron (12 canonical vertices, 20 faces) → N subdivisions (each
  triangle → 4, midpoint edge-dedup) → spherical projection
  v = R·normalize(v) → fixed Lloyd relaxation (5–10 iterations,
  reproject each pass) → dual mesh (triangle centroids → polygon cells
  with neighbor graph).
- One mesh type as the base for any spherical body (planets, stars, …):
  gameplay cells AND renderer source data (positions + indices,
  render-ready; no GPU code in this feature).
- N is a parameter, default 6 → 10·4⁶+2 = 40,962 cells
  (40,950 hexagons + 12 pentagons).
- ADR-002 written: hex-dominant geodesic dual chosen as planet
  representation; rendering.md far-field line updated.

## Non-goals

- No surface layers (elevation, biomes, water table, POIs) — separate
  features stacked on this one.
- No renderer pipeline code (vulkano passes land in M1); mesh data only.
- No chunk streaming / LOD / descent fades (M2).
- No heavy physics: no fluid sim, no erosion sim.
- No adaptive/non-deterministic relaxation (CVT/quasi-Newton variants
  are Open questions only).
- No body-specific semantics (planet/star types, orbits) — those live in
  later layers (`engine::universe` and friends).

## Users / Stakeholders

- Later world-gen layers for every spherical body (cells as data substrate).
- Renderer (M1 far-field planet mesh source; any body mesh source).
- Gameplay (colony placement, resources keyed to cells).
- Developers (headless tests, tools preview).

## Functional requirements

All of the below lives in `engine::hexsphere`.

- Icosahedron builder: 12 canonical vertices, 20 triangular faces,
  normalized to radius R.
- Subdivision: triangle → 4 children via edge midpoints (shared-edge
  dedup), repeated N times; N parameterized, default 6.
- Spherical projection of every vertex: v = R · normalize(v).
- Lloyd relaxation: move each vertex toward neighbor centroid, reproject
  to sphere; iteration count K fixed (5–10, pinned per universe_version).
- Dual mesh: cells from adjacent-triangle centroids; each cell exposes
  center, polygon vertices, and 5/6 neighbor indices; stable cell index
  space.
- Topology invariants hold and are tested: exactly 12 pentagons, closed
  manifold, Euler V−E+F = 2, every cell has 5 or 6 neighbors.

## Non-functional requirements

- Determinism: same (N, R, K, universe_version) → identical mesh hash
  across runs and platforms; quantized floats and purity per the
  universe.md determinism rules, applied to `engine::hexsphere`; no
  wall-clock or hash-iteration-order dependence in output.
- O(n) passes over cells; per-cell storage documented and minimal
  (centers + neighbor indices; quantization allowed); N=6 mesh fits in
  single-digit MB within the Low-tier <1 GB app budget.
- Generation is one-time and cacheable (seed-independent); target <1 s
  for N=6 on mid-tier phone class (informational until the ADR-007 perf
  harness gates).
- All quality.md gates green, incl. mobile compile-guard and
  determinism-test policy (adapted to `hexsphere`).

## Definition of Done

- [ ] Pipeline implemented in `engine::hexsphere` (icosahedron →
  subdivide → project → relax → dual), pure + deterministic; N default 6.
- [ ] Topology tests pass: 12 pentagons, Euler = 2, closed manifold,
  all cells 5/6-neighbor, across tested N range.
- [ ] Determinism test: identical mesh hash across repeated runs with a
  committed expected hash; float quantization per universe.md.
- [ ] Per-cell storage documented + within budget; O(n) passes; no heavy
  physics anywhere in the feature.
- [ ] ADR-002 written and linked in docs/decisions/README.md;
  rendering.md and architecture.md (new `hexsphere/` module row)
  updated; techstack version bumped; all touched links resolve.
  (universe.md untouched — the universe layer is a later discussion.)
- [ ] All quality.md gates green; plan.md DoD verification records
  evidence per criterion.

## Constraints & Assumptions

- Fixed iteration counts everywhere (K pinned per universe_version); no
  adaptive convergence — platform float drift would break determinism.
- New module `engine::hexsphere` stays pure + headless-testable: no GPU
  handle, no window (architecture.md module rules); `engine::universe`
  (galaxy/system layer) is out of scope here and defined separately
  later; architecture.md is updated to document the new module.
- 12 pentagons are mathematically unavoidable; they are first-class
  cells, never special-cased hacks in gameplay or rendering.
- Mesh is seed-independent: shared across bodies of equal (N, R);
  per-seed data layers come later.
- Follows AGENTS.md docs-maintenance rules for every touched doc.

## Open questions

- Relaxation optimization: spherical Laplacian smoothing / CVT /
  quasi-Newton hybrids could cut iteration count — evaluate at plan time
  only if fixed Lloyd misses quality or perf targets; any adoption must
  keep fixed iteration counts.
- Neighbor storage: explicit per-cell indices vs implicit icosahedral
  addressing — memory vs code complexity, decided in plan.
- Normalize per subdivision level vs single final projection — measure
  at plan time.
- Render LOD/decimation for far-field: derived from this mesh in M1 or
  deferred to M2 streaming?
