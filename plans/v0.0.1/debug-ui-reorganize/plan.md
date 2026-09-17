# Plan — debug-ui-reorganize

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Remove the flat view

Delete `ViewFocus::ChunkFlat`, flat-viewer state
(`chunk_flat_*`, `rebuild_chunk_flat`, `orbit_chunk_flat`,
`player_flat_uv`), `CHUNK_FLAT_VERT` + `chunk_flat_pipeline` +
`upload_chunk_flat*`, the ChunkFlat draw branch, arrow-key orbit input,
flat picking (`pick_flat_visible`, `flat_point_from_cursor`), the flat
player marker + `FLAT_RECENTER_DOT` rim re-anchor, flat mesh builders,
flat headless asserts. Keep `engine::render::chunk_flat`.

### Phase 2 — UV screen + kill `ViewFocus`

Two screen enums in `app.rs` (`MainScreen`, `ToolsScreen`); delete
`ViewFocus`/`toggle_focus`/`thumb_label`; `draw()` dispatches on
`MainScreen` (sphere branch vs full-viewport UV branch); thumb rects,
swap button, `V` key gone; nav is F1/F2 on the main window.

### Phase 3 — Contextual inputs

Per-screen left docks (sphere: presets + shader + 3 overlays; UV:
shader + UV-wire/seams); density row only in Checker mode; player rows
only when active; preset keys only on the sphere screen; shared right
dock on both screens.

### Phase 4 — Tools window + FPS

`WindowContext` per OS window (own surface/swapchain/framebuffers/
pipelines/descriptor set), shared device/allocators/atlas/shader
modules; `WindowId` event routing; per-window redraw; `FpsOverlay`
ring-buffer implementation; tools nav (clicks + `1/2/3`); `F3`
reopens; FPS tab with live numbers + sparkline.

### Phase 5 — Tests + headless

Update `app` / `sphere_viewer` / `mesh` / `picking` / `ui` / `main`
tests for the new structure; headless drops flat asserts.

### Phase 6 — Docs

`rendering.md`, `architecture.md`, `controls.md`, version bump,
AGENTS.md invariant line; `chunk-flat-view` cancelled.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| REORG-001 | done | plans docs + cancel chunk-flat-view | — |
| REORG-002 | done | lib: remove flat state/focus (sphere_viewer, mesh, picking, ui, app, fps) | Goals 1–2 |
| REORG-003 | done | main.rs: remove flat rendering/input/headless | Goals 1–2 |
| REORG-004 | done | main.rs: UV screen, per-screen docks, contextual hints | Goals 2–3 |
| REORG-005 | done | main.rs: two-window shell + tools UI + FPS wiring | Goal 4 |
| REORG-006 | done | tests + headless green | DoD 4 |
| REORG-007 | done | docs + AGENTS.md + version | DoD 5 |

## Role sign-off

Breakdown approved by: ARCHITECT _(implementing agent)_ · Todos
approved by: TECHLEAD _(implementing agent)_ · UX acceptance rows (if
player-facing): n-a · DoD verified by: ANALYST _(pending)_ · Security
reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | No flat/focus/thumb refs in `crates/debug` | done | grep: zero matches for `ViewFocus\|ChunkFlat\|chunk_flat\|thumb\|CHUNK_FLAT_VERT\|pick_flat_visible\|flat_point_from_cursor\|player_flat_uv\|orbit_chunk_flat\|FLAT_RECENTER` in `crates/debug` (engine `render::chunk_flat` kept) | implementing agent |
| 2 | F1/F2 screens with contextual controls | code-done, windowed run pending | unit tests (`viewer_ui_contains_panel_content`, `panel_plans_stay_inside_and_ordered`); manual windowed confirmation still needed | — |
| 3 | Tools window with live FPS; close/F3 | code-done, windowed run pending | unit test (`tools_ui_shows_fps_numbers_and_placeholders`); manual windowed confirmation still needed | — |
| 4 | Gates green | done | `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace --all-targets`, `game_debug --headless` pass (see Acceptance criteria) | implementing agent |
| 5 | Docs updated | done | rendering.md, architecture.md, controls.md, techstack version 0.8.0, AGENTS.md invariants; links resolve | implementing agent |

## Acceptance criteria

- `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo test --workspace
  --all-targets`, `cargo run -p game_debug -- --headless` pass.
- Manual windowed run: both windows render, nav works, close/reopen
  works.

## Risks & Next steps

- `main.rs` (~4k lines) carries most of the diff; phases land in
  order so the multi-window diff stays reviewable.
- Console (ADR-009 tracing layer) and full inspector are follow-up
  features, not this plan.
