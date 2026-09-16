# Plan — chunk-flat-view

Parent notion: [`notion.md`](notion.md)

> Revision 2026-09-15: the first implementation unfolded the whole
> sphere (global BFS graph unfold). Rejected — a sphere cannot be
> flattened without tearing. Revised strategy: tangent-plane
> hemisphere projection with arrow-key orbit + per-step unload/load.
>
> Follow-up 2026-09-15: rim chunks rendered half-cut and stretched —
> `visible_hemisphere` now requires **all** corners inside (partial
> rim cells dropped) and `project_to_tangent` uses the Lambert
> azimuthal equal-area map (equal chunk sizes, bounded disk, clamp
> removed).

## Feature breakdown

### Phase 1 — Engine projection (`engine::render::chunk_flat`)

`visible_hemisphere` + `tangent_basis` + `project_to_tangent` +
`orbit_viewpoint`. Unit tests: half coverage, fully-inside filter,
bounded disk, per-class equal area, orthonormal basis,
origin mapping, radius-preserving orbit, determinism.

### Phase 2 — Debug lib (`game_debug`)

Hemisphere builders in `mesh.rs` (projected true-corner fans +
deduped wireframe + picking sidecars); viewpoint state +
`rebuild_chunk_flat()` + `orbit_chunk_flat()` in `sphere_viewer.rs`;
`pick_flat_visible` + `flat_point_from_cursor` in `picking.rs`.

### Phase 3 — Viewer binary (`game_debug` bin)

`CHUNK_FLAT_VERT` shader, chunk-flat pipeline reusing `FILL_FRAG` +
`FillPush`, upload helpers, 3-way render path, arrow-key orbit with
buffer reload, flat hover + click-pin, `--headless` self-test line.

### Phase 4 — Docs + gates

`architecture.md`, `rendering.md`, `techstack/README.md` v0.6.2, DoD
evidence, full `quality.md` gate run.

### Phase 5 — Amendment 2026-09-16: continuous player marker

`build_chunk_flat` returns `ChunkFlatNorm` (`chunk_flat_normalize`
helper); `SphereViewerState` stores it + exposes `player_flat_uv()`;
binary replaces the cell-center marker lookup with the exact
projection, replaces the 100 ms viewpoint follow with the rim-only
re-anchor (`FLAT_RECENTER_DOT`), and decouples the streaming
desired-set onto the player's own hemisphere (`STREAM_SYNC_MS`
throttle). Tests: norm round-trip in `mesh`, rewritten
`player_view` key tests, headless turn-then-thrust walk.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CF-001 | done | Engine projection helpers + exports | Engine |
| CF-002 | done | Hemisphere builders in `game_debug::mesh` | Debug mesh builder |
| CF-003 | done | Viewpoint state, rebuild, orbit | Viewer state |
| CF-004 | done | `pick_flat_visible` + `flat_point_from_cursor` | Viewer state |
| CF-005 | done | Binary: shader, pipeline, upload, render path | Viewer binary |
| CF-006 | done | Binary: arrows + flat hover/pin + headless | Viewer binary |
| CF-007 | done | Docs updates + quality gates green | NFR / DoD |
| CF-008 | done | Drop partial rim chunks; Lambert equal-area projection | Engine + Debug mesh builder |
| CF-009 | done | Exact marker projection: `ChunkFlatNorm`, `player_flat_uv()` | Amendment 2026-09-16 |
| CF-010 | done | Rim-only re-anchor + streaming decouple | Amendment 2026-09-16 |
| CF-011 | done | Amendment docs (`controls.md`, techstack 0.6.7) + gates | NFR / DoD |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Projection helpers implemented, tested | done | `cargo test -p game_engine render::chunk_flat`: 7/7 pass |
| 2 | Hemisphere buffers valid; orbit loads disjoint half | done | lib `mesh` + `sphere_viewer` orbit tests pass |
| 9 | No partial rim polygons; equal chunk sizes | done | engine `visible_cells_are_fully_inside` + `projection_stays_inside_bounded_disk` + `projected_chunks_keep_equal_size` pass; lib `chunk_flat_drops_partial_rim_cells` proves straddling cells never reach buffers |
| 3 | ChunkFlat renders fill + wireframe main, 3D thumb | done | `check --all-targets` clean; render path binds `chunk_flat_pipeline` + hemisphere buffers main, 3D sphere thumb |
| 4 | Arrow keys orbit + reload buffers | done | handler + `refresh_chunk_flat()`; lib orbit test proves disjoint reload |
| 5 | Hover/pin on flat map | done | `pick_flat_visible` lib tests pass; click-pin allows `ChunkFlat` focus |
| 6 | Headless self-test | done | `chunk_flat_cells=20269 chunk_flat_verts=141879 chunk_flat_tris=121610 chunk_flat_pick=chunk0 ok` at N=6 (193 partial rim cells dropped vs center-only filter) |
| 7 | Docs updated, links resolve | done | `architecture.md`, `rendering.md`, `techstack/README.md` v0.6.2 |
| 8 | Quality gates green | done | `fmt --check`, `clippy -D warnings`, workspace `--all-targets` (engine 61, debug lib 77, debug bin 10), `--doc`, `game` + `game_debug --headless` + `game_tools --headless --tier low` all green |
| 9 | Amendment: exact marker projection | done | `chunk_flat_normalize` round-trips raw cell centers onto buffer centers (`chunk_flat_hemisphere_covers_visible_cells`); `player_flat_uv()` replaces the `pick_cell → chunk_flat_centers` lookup |
| 10 | Amendment: rim-only re-anchor; streaming follows player | done | 100 ms `moved && due` follow deleted; re-anchor only below `FLAT_RECENTER_DOT` (0.35 ≈ 70°); desired set = `visible_hemisphere(mesh, player.position())` on `STREAM_SYNC_MS` throttle; headless `player_selftest=lon28.64 lat-0.00 loaded21873 ok` |
| 11 | Amendment docs + gates | done | `controls.md` Surface-walk rewritten; `techstack/README.md` 0.6.7 → 0.6.8; full gate re-run green (24 game + 89 debug lib + 19 debug bin + 61 engine, doc 4 + 6, all three binary runs) |
| 12 | Follow-up 2026-09-16: flat map opens on the player | done | `regenerate` sets `chunk_flat_viewpoint = player.position()` (was north pole): the exact marker spawns strictly inside the map with no first-frame re-anchor jump; pinned by `flat_marker_spawns_centered` and the updated `defaults_build_high_tier_stats` viewpoint assertion |

## Acceptance criteria

- Windowed viewer cycles Sphere → UV → ChunkFlat; flat map shows the
  north-pole hemisphere as true-shape polygons.
- Arrow keys orbit the viewpoint; the loaded set visibly changes
  (rim chunks unload, new chunks load).
- Hover/pin highlight + CHUNK panel work in flat.
- `cargo test` + headless runs pass.

## Risks & Next steps

- Risk: gnomonic rim stretch → resolved by Lambert equal-area map
  (bounded disk, clamp removed); per-class area spread < 1.2 pinned by
  test. Verify visually at N=4.
- Risk: orbit-step rebuild cost at N=6 → one half ≈ 20k fans;
  acceptable for a debug tool, measure if slow.
- Amendment note: rim re-anchors re-orient the map around the walking
  player (view change only); an orientation-preserving basis at
  re-anchor is optional engine-side polish.
- Next: configurable radius, zoom/pan, chunk id labels.
