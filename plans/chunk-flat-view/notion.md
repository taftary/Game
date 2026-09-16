# Notion — chunk-flat-view

## Status

`in-progress` (plan: [`plan.md`](plan.md) — base feature implemented;
reopened 2026-09-16 for the continuous-player-marker amendment:
exact-projection marker, rim-only re-anchor, streaming decouple;
see Phase 5. Awaiting windowed visual confirmation in the viewer.)

Parent updates driving this amendment:
[`../player-sphere-movement/update-2026-09-16-0736/notion.md`](../player-sphere-movement/update-2026-09-16-0736/notion.md),
[`../debug-player-view/update-2026-09-16-0736/notion.md`](../debug-player-view/update-2026-09-16-0736/notion.md).

## Context

`cell-chunks` is done: every dual cell has a stable `ChunkId`, the
Sphere Viewer highlights hovered/pinned chunks in the 3D view, and the
panel shows chunk id/type/neighbors. `sphere-uv-debug` is done: the UV
net view unfolds the 20 icosahedron base faces as a triangle strip with
full seam handling.

A whole sphere cannot be flattened without tearing it apart — so the
flat plan shows **one hemisphere at a time**: a Lambert azimuthal
equal-area projection of the fully-inside chunks onto the tangent
plane at the viewpoint, so every chunk keeps its size and no partial
rim polygons appear. Arrow keys orbit the
viewpoint across the surface; chunks leaving the half unload, entering
chunks load (GPU buffers rebuilt per step). This is the visual
reference M2 patch construction will build from, and a first exercise
of load/unload behavior.

## Problem & Needs

- The UV net shows chunks inside 20 separate triangular regions, not
  as a navigable surface map.
- No flat view shows true chunk shapes with neighbor-shared edges.
- No tool exercises chunk load/unload while navigating the surface.
- M2 patch construction needs a visual adjacency + loading reference.

## Goals

- `engine::render::chunk_flat` gains pure + deterministic projection
  helpers: `visible_hemisphere`, `tangent_basis`,
  `project_to_tangent`, `orbit_viewpoint`.
- `game_debug::mesh` gains hemisphere builders: `build_chunk_flat`
  (filled fans of projected true corners) + wireframe, normalized to
  `[0, 1]²`, with visible cell/center sidecars for picking.
- Sphere Viewer gains a third `ViewFocus::ChunkFlat` mode: hemisphere
  flat map main, 3D sphere thumb. Arrow keys orbit the viewpoint 5°
  per step and reload the flat GPU buffers (unload + load).
- Hover/pin + CHUNK panel work on the flat map via
  `pick_flat_visible` over the loaded set.

## Non-goals

- No chunk id text labels (panel section only, same as 3D view).
- No zoom/pan or configurable view radius (full hemisphere, fixed).
- No texture/biome visualization (debug geometry).
- No changes to the release `game` binary; no mobile/touch UX.
- No persistent streaming lifecycle — load/unload is GPU-buffer
  rebuild per orbit step (CPU data always resident).

## Users / Stakeholders

- Developers (only): chunk-graph verification, load/unload behavior
  while navigating, pentagon-distribution inspection.
- M2 streaming stub (consumer): visual reference for patch building.
- Later world-gen layers: per-chunk adjacency checks.

## Functional requirements

### Engine (`engine::render::chunk_flat`)

- `visible_hemisphere(mesh, viewpoint) -> Vec<u32>`: cells whose
  every corner satisfies `dot(corner, viewpoint) > 0`, cell-id order
  (partial rim chunks are dropped, never drawn half-cut).
- `tangent_basis(viewpoint) -> ([f32; 3], [f32; 3])`: orthonormal
  `(right, up)` of the tangent plane.
- `project_to_tangent(point, viewpoint) -> [f32; 2]`: Lambert
  azimuthal equal-area map; viewpoint maps to `(0, 0)`, the rim maps
  to radius `√2·R` (finite — no clamping).
- `orbit_viewpoint(viewpoint, yaw, pitch) -> [f32; 3]`: yaw around
  world Y, then pitch around local tangent-right; stays on sphere.

### Debug mesh builder (`game_debug::mesh`)

- `build_chunk_flat(mesh, viewpoint) -> (vertices, indices, cells,
  centers)`: filled fans of projected true corners, normalized to
  `[0, 1]²`; `cells`/`centers` aligned visible sidecars.
- `build_chunk_flat_wireframe(mesh, viewpoint) -> Vec<[f32; 2]>`:
  deduplicated projected boundary edges.

### Viewer state (`game_debug`)

- `ViewFocus::ChunkFlat` third variant; `toggle()` cycles
  `SphereMain → UvMain → ChunkFlat → SphereMain`.
- `SphereViewerState` gains `chunk_flat_viewpoint` (reset to north
  pole on `regenerate()`), `chunk_flat_cells/centers/vertices/
  indices/wire`; `rebuild_chunk_flat()` reloads the half;
  `orbit_chunk_flat(yaw, pitch)` orbits + reloads.

### Viewer binary (`game_debug` bin)

- `CHUNK_FLAT_VERT` shader (per-vertex chunk id → hover/pin),
  shared `FILL_FRAG`; `FillPush` stays ≤ 128 B.
- Render: `ChunkFlat` focus draws the hemisphere main + 3D sphere
  thumb.
- Arrow keys (Left/Right = yaw, Up/Down = pitch, 5°/step) reload
  buffers via `refresh_chunk_flat()`; hover re-resolves after orbit.
- `--headless` prints `chunk_flat_*` self-test line (coverage +
  antipodal reload + flat pick check).

### Amendment 2026-09-16 — continuous player marker (Phase 5)

The flat player marker snapped cell-center to cell-center and the
viewpoint re-centered every 100 ms, teleporting the marker to map
center each sync. New behavior:

- `build_chunk_flat` also returns its `(lo, span)` normalization
  (`ChunkFlatNorm` + `chunk_flat_normalize`); `SphereViewerState`
  stores it at every rebuild and exposes `player_flat_uv()` — the
  player's exact position in buffer UV space (no cell snap, clamped
  at the rim).
- The flat viewpoint stays fixed while the player walks inside its
  hemisphere and re-anchors on the player only when the viewpoint
  cosine drops below `FLAT_RECENTER_DOT` (~70°): movement is never
  interrupted (re-anchor is a view change; the sim is untouched), and
  the marker glides continuously between re-anchors.
- Streaming desired-set decouples from the flat viewpoint: the binary
  refreshes the player's own hemisphere on a 100 ms throttle
  (`STREAM_SYNC_MS`) instead of cloning the flat cells.

## Non-functional requirements

- All `docs/techstack/quality.md` gates green.
- Engine stays pure + deterministic.
- Hemisphere rebuild is O(visible cells); ~20k chunks at N=6 per
  half, acceptable for the debug tool.
- Hover readout updates within one frame of cursor movement.

## Definition of Done

- [ ] Projection helpers implemented, tested, documented.
- [ ] Hemisphere builders emit valid buffers in `[0, 1]²`; orbit
  loads a disjoint half.
- [ ] No partial rim polygons on the flat map; projected chunks keep
  equal size (equal-area map, per-class spread pinned by test).
- [ ] `ViewFocus::ChunkFlat` renders fill + wireframe main, 3D thumb.
- [ ] Arrow keys orbit + reload buffers (unload/load).
- [ ] Hover/pin works on the flat map.
- [ ] `--headless` self-test covers hemisphere + reload + pick.
- [ ] Amendment: marker uses exact projection (norm round-trip pinned
  by test); viewpoint re-anchors at the rim only; streaming follows
  the player, not the viewpoint.
- [ ] Docs updated; all touched links resolve.
- [ ] All quality.md gates green; plan.md DoD verification records
  evidence per criterion.

## Constraints & Assumptions

- Follows `plans/README.md` lifecycle and AGENTS.md docs-maintenance
  rules.
- Developer screen only: never leaks into the release `game` binary.
- `FillPush` stays ≤ 128 B; GLSL avoids `uint(negative)` UB.
- Lambert maps the open hemisphere into a finite disk, so no rim
  clamping exists anywhere in the pipeline.
- Layout computed on CPU per orbit step; no GPU compute.

## Open questions

- View-radius configurability (v2); zoom/pan (v2).
- Chunk id text overlay (later update).
