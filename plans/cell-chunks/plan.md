# Plan — cell-chunks

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Chunk identity (engine + ADR)

`ChunkId` newtype in `engine::hexsphere` (additive API, no geometry
touch): struct + `index()`, `HexSphere::chunk_id(cell)` +
`HexSphere::chunk_count()`, unit tests (roundtrip, ordering, Display/
Debug, out-of-range panic parity with siblings), doc example on
`chunk_id`. ADR-010 (`docs/decisions/ADR-010.md`) records identity =
cell index + grouping deferred to M2; link it in
`docs/decisions/README.md`.

### Phase 2 — Picking (debug lib, pure)

New `game_debug::picking` module wired in `lib.rs`: `Ray`,
`ray_from_cursor`, `intersect_sphere`, `pick_cell` (max-dot argmax +
neighbor-graph hill-climb from `hint`, ties → lowest index). Tests:
ray through mesh center hits both faces; cursor corners miss; pick at
cell-center direction → that cell for sampled N; corner pick →
deterministic lowest adjacent id; antipodal direction → far cell;
hill-climb from wrong hints ≡ full scan on a sample ring; N=0
dodecahedron self-pick.

### Phase 3 — Viewer UX (state + shaders + binary)

- `SphereViewerState`: `hovered`/`pinned: Option<u32>`,
  `toggle_pin(cell)`; `regenerate()` clears hover and drops
  out-of-range pins; state tests.
- `panel_plan` + `build_viewer_ui`: `CHUNK` section rows above
  `STATS` (chunk/type/neighbors/state); update the
  `panel_plan_stays_inside_and_ordered` assertions + the
  `viewer_ui_contains_panel_content` needles.
- `FillPush` gains `hover_cell`/`pin_cell` (`-1.0` = none; 88 B total,
  assert ≤ 128 B in a test); `FILL_VERT` sets per-cell `flat` hover/pin
  flags via `float(gl_VertexIndex)` comparison; `FILL_FRAG` mixes toward
  cyan-white 45%/75% after the mode switch. `viewer_shaders_compile`
  keeps passing.
- `main.rs`: `CursorMoved` computes hover from the sphere-rendered rect
  (main viewport / thumb by focus); press/release ≤ 4 px on a hovered
  chunk toggles the pin (main viewport only; thumb keeps swap); draw
  passes both ids to both sphere draws; `--headless` prints the pick
  self-test line.

### Phase 4 — Docs + gates

`architecture.md` hexsphere row (ChunkId) + debug row
(hover/pin/picking); `universe.md` chunk-terminology clarification;
`techstack/README.md` version bump 0.5.4 → 0.6.0; full `quality.md`
gate list green; DoD evidence filled below.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CHK-001 | done | notion.md + plan.md written per plans/README.md | Constraints |
| CHK-002 | done | `ChunkId` + `chunk_id`/`chunk_count` + tests + doc example in `engine::hexsphere` | Functional: Engine |
| CHK-003 | done | ADR-010 + decisions/README link | Goals |
| CHK-004 | done | `game_debug::picking` + unit tests | Functional: Viewer picking |
| CHK-005 | done | viewer state (`hovered`/`pinned`/`toggle_pin`) + regenerate rules + tests | Functional: Viewer UX |
| CHK-006 | done | CHUNK panel section + UI/plan tests updated | Functional: Viewer UX |
| CHK-007 | done | FillPush + FILL_VERT/FILL_FRAG highlight (≤128 B test) + shader compile test | Functional: Viewer UX |
| CHK-008 | done | binary hover/pin wiring + headless self-test line | Functional: Viewer UX |
| CHK-009 | done | docs: architecture.md, universe.md, techstack README 0.6.0 bump | Constraints |
| CHK-010 | done | quality.md gates green (fmt/clippy/build/test/doc/game+debug+tools headless) | Non-functional |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | `ChunkId` implemented, tested, doc-referenced; hash + topology tests pass | done | `crates/engine/src/hexsphere.rs`: `ChunkId` (Copy/Eq/Hash/Ord) + `index()` + `HexSphere::chunk_id`/`chunk_count`, 5 doc examples, 3 unit tests (`chunk_identity_matches_cells`, `chunk_ids_cover_all_neighbors`, `chunk_id_rejects_unknown_cell`). All 17 hexsphere tests pass incl. unchanged `committed_hash_matches` (11459543604394007386) and topology invariants. |
| 2 | Hover highlights + correct id, all 6 modes, main + thumb, N 0–4 | done | Correctness: `pick_cell` proven by 11 picking tests (centers→self N0–3, corner/antipode, miss→None, hill-climb≡full-scan on dense N2–3 samples from antipodal hints, stale-hint fallback). Highlight: `v_hover`/`v_pin` mixed 45%/75% toward cyan-white AFTER the mode switch in `FILL_FRAG` (all modes by construction); both sphere rects fed via `update_hover` (main viewport + thumb); `fill_centers_match_mesh_order` pins the `gl_VertexIndex`==chunk-id contract; `viewer_shaders_compile` + `fill_push_constants_fit_vulkan_floor` (88 B ≤ 128 B) pass. Eyeball check on first windowed run recommended (structural evidence only — no display in CI). |
| 3 | Click pin/unpin per spec; drags never pin; thumb still swaps | done | `pin_toggles_with_hover_precedence` + `regenerate_clears_hover_and_stale_pin` pass. Wiring: `press_cursor` + `CLICK_MAX_DRAG_PX` (4 px) travel gate, release-in-viewport + `SphereMain`-only guard; thumb path still calls only `toggle_focus` (no pin code on that path). |
| 4 | `picking` unit tests pass (centers/corners/antipode/miss/climb/N=0) | done | 11/11 pass: `pick_at_cell_centers_returns_self` (N0–3), `corner_pick_finds_global_maximizer` (maximizer-equality + repeat determinism), `antipodal_pick_finds_far_side`, `center_ray_hits_origin_sphere`, `grazing_ray_misses`, `degenerate_rays_hit_nothing`, `unproject_roundtrips_through_view_proj`, `hill_climb_matches_full_scan_on_dense_samples`, `stale_hint_from_another_mesh_still_picks`, `zero_mesh_viewport_rejects`, `layout_viewport_type_flows`. |
| 5 | ADR-010 + architecture/universe/techstack docs updated, links resolve | done | `docs/decisions/ADR-010.md` + README link (`ADR-010.md` resolves); `architecture.md` hexsphere row (`ChunkId`), debug row (picking/hover/pin), purity rule extended; `universe.md` surface-chunk vs cell-chunk line with `../decisions/` link (resolves to `docs/decisions/`); `techstack/README.md` bumped 0.5.4 → 0.6.0. |
| 6 | quality.md gates green | done | `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo build --workspace`, `cargo test --workspace --all-targets` (132 tests: 68 debug-lib + 10 debug-bin + 54 engine), `cargo test --doc --workspace` (5 engine doctests), `cargo run --bin game`, `cargo run -p game_debug -- --headless` (`pick_selftest=chunk0 ok`), `cargo run -p game_tools -- --headless --tier low` — all exit 0. |

## Acceptance criteria

- `cargo test -p game_engine hexsphere` green incl. new chunk tests and
  the unchanged committed-hash test; `cargo test -p game_debug picking`
  green; windowed viewer manually hovered across modes (screenshot in
  update if visual doubt).
- `git status` shows only: `plans/cell-chunks/`, `crates/engine/src/hexsphere.rs`
  (+ tests), `crates/debug/src/{lib,picking,sphere_viewer,main}.rs`,
  `docs/{decisions,techstack,game}` touched lines.

## Risks & Next steps

- Risk: `gl_VertexIndex` relies on the provoking-vertex + upload-order
  contract — mitigated: same contract already backs tint/seam/island
  (commented in-shader), plus a defensive test asserting center-vertex
  ordering (`fill_vertices[cell].position == mesh.cell_center(cell)`).
- Risk: thumb hover rect is small (244×144) — acceptable d
...[truncated 447 chars]