# Plan — Debug Player View

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — [ARCHITECT] `game` lib (DPV-001)

Add `[lib]` to `crates/game/Cargo.toml`; move `pub mod` declarations
to `src/lib.rs`; `main.rs` uses `game::…`. Register `game` in
`[workspace.dependencies]`; `game_debug` depends on it. No cycles
(`game` → `game_engine` + `glam` only).

### Phase 2 — [DEV] `PlayerViewState` (DPV-002)

New `crates/debug/src/player_view.rs` (pure): `Player` + `PlayerCamera`
+ `ChunkStreamer` + `MoveKeys` + tick + `active` flag. Methods: `new`,
`reset(radius)` (keeps `active`), `set_keys`, `input`, `update(dt,
desired) -> StreamDelta`, `toggle`, `cycle_camera`, `mode`,
`position`, `eye`, `view_matrix`, `projection_matrix`,
`lon_lat_deg`, `loaded_count`, `tick`. Speed default `0.5·R`.
Unit tests: key→input map, walk changes lon/lat, toggle/cycle,
streaming loads then evicts, reset keeps active, determinism.

### Phase 3 — [DEV] Viewer state (DPV-003)

`SphereViewerState.player: PlayerViewState`; `regenerate()` resets it
to the applied radius. `thumb_label` + swap-button copy switch `U` →
`V` (U now toggles player). `toggle_focus` 3-cycle untouched.
Update `focus_toggles_both_ways_with_labels`.

### Phase 4 — [DEV] Binary input + render (DPV-004)

- Keys: `U` toggle player, `V` focus cycle, `P` cycle player camera
  (active only), `WASD`+arrows drive `MoveKeys` (press + release;
  fields focused → text as today), `G/T/B/R` global presets
  (inactive-player only… presets always allowed; they retarget the
  global orbit camera).
- Drag/wheel route to player camera when active.
- `update_hover` uses the active view matrix.
- `draw()`: frame `dt` → `player.update`; throttled (10 Hz) flat
  follow + `refresh_chunk_flat`; sphere marker (world→pixels) and flat
  marker (chunk-center→pixels) pushed as UI solids.
- Pure helpers (`world_to_pixels`, `flat_uv_to_pixels`,
  `snap_global_camera`, `GlobalPreset`) with binary unit tests.
- Headless: player self-test line.

### Phase 5 — [DEV] Panel sections (DPV-005)

`PLAYER` header + 3 lines (pos / cam / chunks); `VIEW` header + one
row split into 4 preset buttons (`ui::split_row_4` + test). Click
routing in the press handler. Extend `panel_plan` ordering test +
`viewer_ui_contains_panel_content` needles.

### Phase 6 — [TECH] Gates + docs (DPV-006)

`fmt --check`, `clippy -D warnings`, `test --workspace --all-targets`,
`test --doc`, `run --bin game`, debug/tools headless. Update
`docs/game/controls.md` (viewer keys), techstack `0.6.5`.

## Todo

| ID | Role | Status | Task | Ref notion § |
|----|------|--------|------|--------------|
| DPV-001 | ARCHITECT | done | `game` lib + debug dep | FR 1 |
| DPV-002 | DEV | done | `player_view.rs` + tests | FR 2 |
| DPV-003 | DEV | done | viewer state + regen + labels | FR 3 |
| DPV-004 | DEV | done | input/render/markers/headless | FR 3–6, 8 |
| DPV-005 | DEV | done | panel sections + buttons | FR 7 |
| DPV-006 | TECH | done | gates + docs | DoD |

## DoD verification

| DoD # | Criterion | Status | Evidence |
|-------|-----------|--------|----------|
| 1 | game lib reuse | done | `game` lib+bin build; demo output unchanged |
| 2 | U/P/WASD walk + readout | done | `player_view` tests; `viewer_ui_shows_player_readout_when_active` |
| 3 | dots on sphere + flat | done | `world_to_pixels` / `flat_uv_to_pixels` tests; marker push in `draw()` |
| 4 | flat follows, streams | done | headless `player_selftest=lon28.64 lat0.00 loaded21873 ok` |
| 5 | presets clickable + keys | done | `global_presets_frame_the_planet`; `split_row_4` test |
| 6 | tests green | done | 20 + 85 + 14 + 61 unit, 10 doc — 0 failed |
| 7 | docs updated | done | controls.md viewer keys; techstack 0.6.5 |

## Acceptance criteria

- All gates in `docs/techstack/quality.md` green.
- Interactive check: U → dot appears, WASD walks it, P cycles cams,
  flat follows, G/T/B/R snap global view.

## Risks & Next steps

- Per-frame flat rebuild throttled to 10 Hz; raise N slowly if it
  hitches (debug tool only).
- Interactive `winit` binding for release `game` still open (M3).
