# Notion — cell-chunks

## Status

`done` (plan: [`plan.md`](plan.md) — all DoD criteria checked)

## Context

`hex-sphere` is done: `engine::hexsphere::HexSphere` produces the pure,
deterministic base mesh for every spherical body — 10·4^N+2 dual cells
(40,950 hexagons + exactly 12 pentagons at N=6) with a stable,
committed-hash-pinned cell index space. `debug-sphere-viewer` is done:
the `game_debug` Sphere Viewer renders that mesh (fill + wireframe,
6 debug-shader modes) with subdivisions/radius inputs and read-only
stats — but it shows only aggregate numbers. No developer can point at
an individual cell and learn its identity.

Meanwhile the M2 milestone ("Descent slice: … chunk streaming stub",
`docs/milestones/README.md`) will need chunk identities to load against,
and `docs/game/universe.md` already promises lazy generation "per
chunk" while `docs/techstack/quality.md` budgets "Planet chunk stream
(descent)". No plan has defined what a chunk *is*. This feature is that
definition — plus the debugger UX that makes it visible and checkable.

## Problem & Needs

- Developers cannot identify a single mesh cell: pentagon sites, relax
  artifacts, and future per-cell data layers (elevation, biomes) have no
  visual anchor to their stable cell index.
- M2 streaming needs a chunk identity to key loads/unloads on, decided
  now — not ad hoc when the streaming stub lands. The id used by the
  debugger must be the id streaming consumes; no throwaway debug-only
  identity.
- The word "chunk" is already overloaded: *terrain streaming chunks*
  (`quality.md`, near-field heightfield) vs. the hexsphere *cell* this
  feature names. The terminology must be disambiguated in docs.

## Goals

- `engine::hexsphere` gains a `ChunkId` newtype: **1 cell = 1 chunk**,
  `ChunkId` = the cell's deterministic index (`0..cell_count`, stable
  per `(N, R, GEOMETRY_VERSION)`). Streaming groups/patches (M2) will be
  built *from* these ids — chunk identity itself stays stable either way.
- Sphere Viewer hover inspection (CPU ray + neighbor-graph pick, pure
  math in the `game_debug` lib): hovering any rendered 3D sphere cell
  (main viewport *and* the sphere thumb) highlights it and reports its
  chunk id in a new read-only panel `CHUNK` section; click pins the
  selection (sticky highlight + info).
- ADR-010 records "chunk identity = cell index, grouping deferred to M2".

## Non-goals

- No chunk streaming / loading / eviction (M2). No per-chunk data
  layers (elevation, biomes) — identity only.
- No UV flat-net hover (v1: 3D sphere only, main + thumb). No floating
  cursor tooltip (panel section only).
- No migration of existing `u32` cell accessors to `ChunkId`
  (additive API only; `render`/`uv` internals untouched).
- No changes to the release `game` binary; no mobile/touch UX (desktop
  dev tool, same rule as `debug-sphere-viewer`).

## Users / Stakeholders

- Developers (only): visual mesh-data correlation, hash debugging,
  future surface-layer work keyed to chunk ids.
- M2 streaming stub (consumer): chunk ids as load keys.
- Later world-gen layers: per-chunk surface data addressed by `ChunkId`.

## Functional requirements

### Engine (`engine::hexsphere`)

- `pub struct ChunkId(u32)` — `Copy + Clone + Debug + PartialEq + Eq
  + Hash + PartialOrd + Ord`; `ChunkId::index(self) -> u32`; doc comment
  states 1 cell = 1 chunk and that M2 groups build from these ids.
- `HexSphere::chunk_id(cell: u32) -> ChunkId` (panics out-of-range, like
  its sibling accessors); `HexSphere::chunk_count() -> usize`
  (`== cell_count()`).
- Committed mesh hash and all topology invariants unchanged (the new API
  is additive; no geometry touched).

### Viewer picking (`game_debug::picking`, pure, window/GPU-free)

- `ray_from_cursor(cursor, viewport_rect, view_proj) -> Ray`: inverse-VP
  unproject, Vulkan NDC (z ∈ [0,1], y-down projection).
- `intersect_sphere(ray, radius) -> Option<Vec3>`: analytic; miss →
  no hover.
- `pick_cell(mesh, point, hint: Option<ChunkId>) -> ChunkId`:
  nearest cell center by max dot product; neighbor-graph hill-climb
  from `hint`; cold start = full argmax scan. Cold scans keep the
  lowest id on exact ties; hinted climbs keep the first-reached tied
  cell — both attain the global f32 max score (ties live at symmetry
  points below f32 resolution; either answer borders the query).

### Viewer UX (Sphere Viewer screen)

- State: `hovered: Option<u32>`, `pinned: Option<u32>`;
  `toggle_pin(cell)` (click on pinned cell unpins; click elsewhere pins).
- Hover computed on cursor move when the cursor is inside a
  sphere-rendered rect (main viewport with `SphereMain` focus, or the
  panel thumb with `UvMain` focus); cleared otherwise.
- Click = press+release with ≤ 4 px movement on a hovered chunk →
  pin; thumb clicks keep the established swap-view behavior (no pinning
  from the thumb). Orbit drags never pin.
- Fill highlight via two new push constants (`hover_cell`, `pin_cell`,
  −1 = none): vertex shader sets per-cell `flat` flags from
  `gl_VertexIndex` (== cell id on fan-center vertices, the existing
  tint/seam/island provoking-vertex trick); fragment shader mixes toward
  cyan-white `(0.6, 0.95, 1.0)` 45% for hover / 75% for pin, *after* the
  debug-mode switch so it reads in all 6 modes. No buffer/layout
  changes; `FillPush` stays ≤ 128 B (Vulkan 1.1 floor).
- Panel `CHUNK` section (above `STATS`): `chunk: 12,345 | —`,
  `type: hexagon | pentagon | —`, `neighbors: 6 | —`,
  `state: hover | pinned | —`.
- Regeneration: a pin whose id is out of range on the new mesh is
  cleared; hover is cleared on regenerate.
- `--headless` prints a pick self-test line (pick at cell 0's center
  direction → `0`) so CI covers picking GPU-free.

## Non-functional requirements

- All `docs/techstack/quality.md` gates green (`fmt`, `clippy -D
  warnings`, `build`, `test --workspace --all-targets`, `test --doc`,
  `game` + `game_debug --headless` + `game_tools --headless` runs).
- Zero determinism/perf impact on the engine: additive API only,
  committed N=6/R=1.0 hash unchanged; picking is O(1) amortized per
  cursor move after one O(n) cold scan.
- Hover readout updates within one frame of cursor movement (desktop
  dev tool; no mobile perf target).

## Definition of Done

- [ ] `ChunkId` implemented, tested, and doc-referenced in
  `engine::hexsphere`; committed hash and topology tests still pass.
- [ ] Hovering any 3D sphere cell (main viewport and sphere thumb)
  highlights it and shows the correct chunk id in the CHUNK section,
  in all 6 debug modes, across tested N (0–4 windowed range).
- [ ] Click pins/unpins the chunk per spec; drags never pin; thumb
  clicks still swap views.
- [ ] `picking` unit tests pass: centers → self, corner ties →
  deterministic lowest id, antipodal ray, miss → None, hill-climb ≡
  full scan on samples, N=0 mesh.
- [ ] ADR-010 written and linked; `architecture.md` (hexsphere +
  debug rows), `universe.md` (chunk terminology line), and
  `techstack/README.md` (version bump) updated; all touched links
  resolve.
- [ ] All quality.md gates green; plan.md DoD verification records
  evidence per criterion.

## Constraints & Assumptions

- Follows `plans/README.md` lifecycle (notion first, then plan) and
  AGENTS.md docs-maintenance rules (update every doc the change
  touches, bump the techstack Version line, verify links).
- Hard rule carried over from `debug-sphere-viewer`: developer screens
  never leak into the release `game` binary; engine stays pure +
  headless-testable (the `ChunkId` addition is data + docs, no GPU).
- `FillPush` must remain ≤ 128 B (Vulkan 1.1 `maxPushConstantsSize`
  floor); GLSL push block and Rust repr must match in field order.
- GLSL must avoid `uint(negative)` UB: hover/pin comparisons go through
  `float(gl_VertexIndex)` vs the float id (indices < 2M stay exact in
  f32).
- Windowed viewer default is N=4 (2,562 chunks); N=6 hover works via
  zoom (subpixel cells at default framing are a known viewer trait).

## Open questions

- UV flat-net hover (v2): needs island-barycentric inverse mapping —
  deferred, not in DoD.
- Color-blind-safe highlight palette review (UX follow-up).
- Existing `u32` cell accessors → `ChunkId` migration (deferred; would
  touch `render`/`uv` internals).
- `ChunkId` serialization shape for saves (`universe_version`
  interplay) — decided when the first per-chunk data layer lands.
