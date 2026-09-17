# Plan — sphere-uv-debug

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Pure logic (headless, no window/GPU)

- Engine UV builder: net table + barycentric map primal→dual (`uv`,
  `island`, `seam`), `UvScheme` enum reserving `Equirect/Cube`.
- Extend `PlanetVertex` with `uv: [f32; 2]`; update `SeededPlanet`,
  debug `build_fill`, `game_tools` layout users.
- Debug state: `ViewFocus`, `DebugMode` (6), checker density, seam/wire-UV
  toggles, `uv_thumb_rect()` + hit-test in `ui.rs`.

### Phase 2 — Rendering + interaction (`crates/debug/src/main.rs` only)

- `FILL_VERT` passes `uv/seam/island`; unified `FILL_FRAG` with
  `mode + density` push constants; flat ortho UV pipeline (no depth).
- Dual-viewport `draw()`: focused view in `layout.viewport`, other in widget
  rect (scissor); widget click + `U` + `[Swap]` wiring; UV pan/zoom.
- Panel `UV DEBUG` rows in `panel_plan` + `build_viewer_ui`.

### Phase 3 — Polish + gates + docs

- Hover highlight, labels, `MAX_UI_VERTS` re-check, headless UV stats line.
- Docs (`rendering.md`, `architecture.md`, techstack version), link check,
  full `quality.md` gates with evidence below.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UVD-001 | done | Engine icosa-net builder + tests (20 islands, uv range, seam-only-on-border, determinism) | Functional requirements |
| UVD-002 | done | `PlanetVertex.uv` + `SeededPlanet`/`build_fill`/`game_tools` updates + buffer-equality tests | Functional requirements |
| UVD-003 | done | Debug state (`ViewFocus`, `DebugMode`×6, density, toggles) + `ui.rs` thumb rect/hit-test + tests | Functional requirements |
| UVD-004 | done | Shaders (uv/seam/island varyings, 6-mode frag) + flat ortho pipeline + compile tests | Functional requirements |
| UVD-005 | done | Dual-viewport draw + swap wiring (click/`U`/button) + panel rows (flat view fixed aspect-fit, no pan/zoom in v1) | Functional requirements |
| UVD-006 | done | Headless stats, docs updates, quality gates + evidence | Non-functional requirements |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Icosa-net UVs on every vertex; N=0 20 islands, uv∈[0,1], seams on borders | done | `render/uv.rs` tests `net_has_twenty_unit_range_triangles`, `debug_uv_covers_all_islands_in_unit_range`, `seams_only_on_island_borders`; headless `uv_islands=20` |
| 2 | Thumb visible; click/`U`/`Swap` toggle both directions; cameras preserved | done | `sphere_viewer.rs` `focus_toggles_both_ways_with_labels`; `main.rs` thumb/caption/swap click + `KeyU` (field-focus guard); `panel_plan` thumb≡`uv_thumb_rect` assert; orbit camera untouched by swap |
| 3 | 6 shader modes live on both views; density works; seams match flags | done | `debug_mode_cycles_all_six_with_stable_ids`; `viewer_shaders_compile` (8 shaders incl. flat); `flat_mvp_centers_unit_square`; both pipelines share `FILL_FRAG` with mode push const |
| 4 | Engine + debug buffers agree; hash story resolved | done | `mesh.rs` `matches_engine_indexed_mesh_at_high_tier` (equality incl. `uv`); `planet.rs` `uvs_cover_all_islands_in_unit_range`; `mesh_hash` untouched (uv excluded), `committed_hash_matches` still green |
| 5 | Both binaries run; headless prints UV stats; gates green | done | `game`, `game_debug --headless` (`uv_islands=20 uv_seam_verts=1902`), `game_tools --headless --tier low` all exit 0; `fmt --check`, `clippy -D warnings`, `build`, `test --workspace --all-targets` (53+8+36), `test --doc` green |
| 6 | Docs updated + links verified; lifecycle followed | done | `rendering.md` UV section, `architecture.md` debug row, techstack `README.md` → 0.5.0; `notion.md`→`plan.md`→implement order kept |

## Acceptance criteria

- Reviewer opens viewer at N=4, sees 3D sphere + UV net thumb; presses `U`,
  views swap; orbit intact both ways.
- Cycling 6 shader modes visibly changes both views; checker density slider
  re-tiles live.
- `cargo test --workspace --all-targets` + headless runs pass; DoD table fully
  checked with file:line evidence.

## Risks & Next steps

- Icosa-net complexity (chosen over equirect) may blow Phase 1 — mitigate by
  pinning the 20-face table early and testing N=0/1 first.
- `PlanetVertex` layout change ripples to `game_tools` + pipeline vertex
  definitions — update atomically.
- Thumb aspect vs. net packing — pick strip layout fitting ~244px width.
- Next: flip notion `Status: planned`, implement UVD-001 → UVD-006 in order.
