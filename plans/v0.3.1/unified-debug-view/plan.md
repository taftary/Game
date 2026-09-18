# Plan — unified-debug-view

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 0 — branch + docs-first (docs-only commits)

Cut `v0.3.1`, write notion/plan, ADR-022, milestones entry.

### Phase 1 — Action registry + shell state (lib, GPU-free)

`actions.rs` registry; `app.rs` one-`Screen` shell; retire the old
screen modules; console log feed; headless asserts rewritten.

### Phase 2 — Window shell (main.rs)

Delete the tools window (creation, routing, handlers, frame
render); single event loop; new chrome builders (top bar,
dropdown, transition strip, corner strip, dev widget); key routing
via the registry.

### Phase 3 — Tab content

Game Demo tab (journey-layer 3D + HUD, interactive); absorbed
MW/SS/Earth views; 7 placeholders with badges; Settings Controls
section rendering the registry.

### Phase 4 — Tests + gates

Unit + parity + headless; `cargo test --workspace --all-targets`,
clippy, fmt, `game_debug --headless`.

### Phase 5 — Docs sweep + commit

Rewrite affected docs, bump techstack to `0.32.0`, verify links,
land as exactly one commit on `v0.3.1`.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UDV-001 | done | Cut branch `v0.3.1` from `main` | Constraints |
| UDV-002 | done | Write `notion.md` (this feature) | — |
| UDV-003 | done | Write `plan.md` (this file) | — |
| UDV-004 | done | ADR-022 + milestones v0.3.1 section | DoD |
| UDV-010 | done | `actions.rs`: `Action` registry (key+label+group), parity test | Functional req. |
| UDV-011 | done | `app.rs`: `Screen`, `ChromeState`, widget/dropdown state, Esc unwind | Functional req. |
| UDV-012 | done | Retire `dimensions.rs`/`scale_debug.rs` screens; console log feed | Goals 5 |
| UDV-020 | done | `main.rs`: delete tools window + handlers + frame render | Goals 1 |
| UDV-021 | done | `main.rs`: chrome builders + registry key routing | Goals 2,5,6 |
| UDV-030 | done | Game Demo tab: layer 3D + HUD + input on shared `Journey` | Goals 3 |
| UDV-031 | done | MW/SS/Earth mount absorbed views with docks/keys | Goals 4 |
| UDV-032 | done | 7 placeholders + `INACTIVE` badges (S7) | Goals 8 |
| UDV-033 | done | Settings Controls section renders registry | Goals 7 |
| UDV-040 | done | Rewrite headless asserts to new shell | DoD |
| UDV-041 | done | Unit tests: nav, dropdown capture, toggles, unwind, defaults | DoD |
| UDV-042 | done | Gates green: test/clippy/fmt/headless | Non-functional |
| UDV-050 | done | Docs sweep + `0.32.0` bump, links resolve | DoD |
| UDV-051 | in-progress | DoD verification + one commit on `v0.3.1` | Constraints |

## Role sign-off

Breakdown approved by: ARCHITECT _(pending)_ · Todos approved by: TECHLEAD
_(pending)_ · UX acceptance rows: **UX consulted 2026-09-17** (S1, S2,
S3, S4+S5, S7 adopted; S6 rejected — see Acceptance criteria) ·
DoD verified by: ANALYST _(pending)_ · Security reviewed by: SECURITY
_(pending)_.

DEV evidence (2026-09-17, branch `v0.3.1`, uncommitted working tree):
`cargo fmt --check` clean; `cargo clippy --workspace --all-targets
--all-features -- -D warnings` clean; `cargo test --workspace
--all-targets` green (game_debug: 130 lib + 18 bin); `cargo run -p
game_debug -- --headless` prints all self-test `ok` lines.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Single window; old enums gone; headless passes | done | `MainScreen`/`ToolsScreen`/`tools_window_event`/`draw_tools`/`WindowKind::Tools` deleted (repo grep clean); `--headless` prints all `ok` lines | DEV |
| 2 | 3 top-level items; dropdown 10 waypoints + active marker; digits select | done | `unified_chrome_shows_topbar_dropdown_widget_and_settings` (topbar + dropdown needles); `app::digit_routing…`, `actions::digits_round_trip…` | DEV |
| 3 | Demo tab interactive + HUD, chrome hidden by default | done | `build_demo_ui` + demo needles; `demo_tab_defaults_to_hidden_chrome`; content keys gated on `screen_content()` so demo inherits galaxy/system/planet behavior | DEV |
| 4 | MW/SS/Earth absorbed; badges; placeholders | done | `screen_content_mapping`; builders reuse `build_galaxy_ui`/`build_system_ui`/`build_planet_ui`; `build_placeholder_ui` + `INACTIVE` needle | DEV |
| 5 | Widget 3 sub-tabs fixed corner; console log; pill in-flight only | done | widget needles (FPS/Console/Inspector bodies); `console::feed_drains_only_unseen_events`; pill preview-leg + silent-when-`None` asserts; `ui::transition_pill_floats_bottom_center` | DEV |
| 6 | Bar always visible; 2 dock toggles + in-bar strip; FPS on widget button | done | `app::toggles_are_independent` (no BAR toggle); corner-strip needles (`DOCK-L [F9]`, `DOCK-R [F10]`, `DEV 60 [\`]`) + `ui::widget_and_corner_rects_stay_on_screen` (strip inside bar) | DEV |
| 7 | Parity test; Controls lists all; key audit | done | `actions::registry_size_is_pinned` (44) + `every_action_has_key_label_title_and_group`; `controls_plan` renders `all_actions()`; audit table in Acceptance criteria; F5 kept for twilight, sub-tabs on F6–F8, docks on F9–F10 | DEV |
| 8 | Docs sweep; links resolve; 0.32.0 | done | `controls.md`, `architecture.md`, `rendering.md` (invariants untouched), `journey.md`, `roles/ux.md`, `roles/security.md`, techstack `0.32.0`, milestones v0.3.1, ADR-022; stale-reference grep clean; `Test-Path` on all touched links | DEV |
| 1 | Single window; old enums gone; headless passes | pending | | |
| 2 | 3 top-level items; dropdown 10 waypoints + active marker; digits select | pending | | |
| 3 | Demo tab interactive + HUD, chrome hidden by default | pending | | |
| 4 | MW/SS/Earth absorbed; badges; placeholders | pending | | |
| 5 | Widget 3 sub-tabs fixed corner; console log; strip in-flight only | pending | | |
| 6 | 4 toggles + corner strip; FPS on widget button | pending | | |
| 7 | Parity test; Controls lists all; key audit | pending | | |
| 8 | Docs sweep; links resolve; 0.32.0 | pending | | |

## Acceptance criteria

- UX rows (2026-09-17 review): top bar always shows the active
  journey layer (S1); transition strip only in flight + log in
  console (S2); FPS on widget toggle button (S3); chrome keys never
  shadow game keys + buttons show keys (S4+S5); no blank pages —
  badges/placeholders with text, not color alone (S7). S6
  (reset-layout) explicitly rejected.
- Chrome audit table (final, collision-checked): F1/F2/F3 nav ·
  `1`–`0` dropdown-captured · `` ` `` widget · F6/F7/F8 sub-tabs ·
  F9/F10 dock toggles (bar always visible, no BAR toggle) · Esc unwind. Content keys unchanged
  (WASD/arrows, U, P, E, T, Q, F, R, G/B, Home, `1`–`6` shader,
  F5 twilight on Earth tab).
- Polish acceptance (2026-09-17 screenshot review): dropdown is a
  solid-black bordered menu; transition pill
  floats bottom-center, in-flight only; toggle buttons live inside
  the always-visible top bar (DOCK-L [F9], DOCK-R [F10], DEV fps [`]);
  docks 260/300 px with 12 px padding; no chrome overlaps the INPUTS
  dock or dropdown rows.
- Dropdown fix + enhancement (2026-09-18): the menu bled under dock /
  content because it was emitted with the top bar (solids draw in push
  order before texts). It is now built dead last of the chrome in
  `draw_main` — topmost surface, matching the click handler — with a
  solid-black background, drop shadow, hover lift from the
  live cursor, hairline row separators, and digit / name /
  right-aligned-status columns.
- Follow-up (2026-09-18): the overlay tail of `draw_main` is now the
  `compose_overlay_ui` helper so the order is machine-checked —
  `overlay_compose_draws_dropdown_on_top_of_settings` asserts the
  panel solid closes the solid list and the 30 dropdown texts close
  the text list on the Settings screen. Verified the shipped binary
  was stale (exe mtime predated the fix with no second copy on disk);
  rebuilt so `target/debug/game_debug.exe` is newer than
  `crates/debug/src/main.rs`.
- Structural fix (2026-09-18): the menu no longer shares the UI
  vertex buffer at all — `compose_overlay_ui` composes it into a
  separate `UiItems`, uploaded to its own buffer and drawn after every
  other UI surface with the same pipeline (topmost by GPU command
  order, not just push order). Test renamed to
  `overlay_compose_isolates_dropdown_in_own_buffer`: asserts zero menu
  content leaks into the main buffer and the menu buffer holds exactly
  shadow + panel + border + separators and the 30 row texts.
- ARCHITECT rows: read-only aggregation in `crates/debug`; no
  simulation mutation; `game`/`engine` untouched except docs.

## Risks & Next steps

- `main.rs` is ~6k lines; tools-window deletion is mechanical but
  large — delete handlers + frame render first, then rewire
  dispatch (UDV-020 before UDV-021).
- `dimensions.rs` L1–L5 3D views are removed with the old shell
  per the supersede decision; re-adding per-dimension 3D content
  is future per-feature work (placeholders make this explicit).
- Execute UDV-004 → UDV-010 → … in order; compile after each lib
  change (`cargo check -p game_debug --lib`).
