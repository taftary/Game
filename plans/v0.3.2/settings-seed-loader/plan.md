# Plan — settings-seed-loader

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Single editable seed home (debug crate, `app` + binary UI)

Move the `TextField` out of `GalaxyMapView` into a new
`app::UniverseSettings` owned by `App` (single source of truth; the
map keeps read-only `seed`). Settings screen gains a UNIVERSE section
(field + `Load [Enter]` + active-seed hint) above the untouched
Controls grid via a shared pure `settings_plan(area, lh, seed)` used
by both draw and click routing (hit-rects-match-by-construction rule).
Milky Way dock loses the SEED editor (read-only `seed N` row stays).
Typing/Enter/backspace/shortcut-gate routing follows the field to its
new scope. No engine/`game` boundary crossed; rendering invariants
untouched (UI pass only).

### Phase 2 — Staged loader (debug crate, new `loader` module + binary)

Pure `loader::{LoadStep × 8, LoadSource, LoadPlan}` (order, labels,
progress, done-semantics; unit-tested): galaxy → journey → system →
cosmic → 3× GPU uploads → finalize. `ViewerApp` executes one step per
`draw_main`, blocks all mouse/keyboard input while `App::loading` is
`Some`, and draws the modal in `compose_overlay_ui`. All entries
(Settings Load/Enter, Milky Way `R`, `--seed` boot) call
`begin_load`; `load_galaxy_seed` is deleted. `--seed` boot defers the
CPU regen to the first frames (sky keeps its boot seed at
construction — unchanged). Demo-tab `R` untouched (in-place reseed).

### Phase 3 — Gates + docs

Loader unit tests + settings-plan geometry test + updated galaxy-map
test; full `quality.md` gate list green; docs sweep (`controls.md`,
`milestones/README.md` v0.3.2 row, `fx.rs` module docs reversing the
"no spinner" note for staged loads, techstack version `0.33.3`).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| SSL-001 | completed | `app::UniverseSettings` (seed field, mirror helper) owned by `App`; drop `GalaxyMapView::seed_field`; update galaxy-map test | Goals §1, FR |
| SSL-002 | completed | `settings_left_plan` + UNIVERSE section in `build_settings_ui`; click/focus/typing/Enter/backspace routing; shortcut gates follow the field | Goals §1, FR |
| SSL-003 | completed | Milky Way dock SEED editor removed (read-only row stays) | Goals §1, FR |
| SSL-004 | completed | New `debug::loader` module (steps, source, plan, progress) + unit tests | Goals §2/3, FR |
| SSL-005 | completed | `begin_load` + one-step-per-frame in `draw_main`; input blocked while loading; delete `load_galaxy_seed`; route Settings/Enter/`R`/boot | Goals §2/3, FR |
| SSL-006 | completed | Modal loader overlay in `compose_overlay_ui` (UX spec) | Goals §2, FR, UX |
| SSL-007 | completed | Quality gates green (fmt, clippy `-D warnings`, build, workspace tests, doc tests, `game` / `game_debug --headless` / `game_tools --headless --tier low`) | NFR, DoD 5 |
| SSL-008 | completed | Docs sweep: `controls.md`, `milestones/README.md`, `fx.rs` docs, techstack `0.33.3`; link check; AGENTS.md accuracy | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-18 — debug-crate-internal
only: `debug::{app,loader}` + `game_debug` binary; `game`/`engine`
untouched, dependency direction legal, no locked choice changed, no
ADR; rendering invariants not touched — UI-pass overlay only; phase
order dependency-safe: state move → surfaces → machine → gates/docs)
· Todos approved by: TECHLEAD (2026-09-18 — risk-first: SSL-001 state
move before surfaces; every todo independently verifiable with one
DoD row; no budget risk — loader adds dozens of UI verts under the
existing `MAX_UI_VERTS` assert; gate list from `quality.md` in
SSL-007) · UX acceptance rows: UX-1 modal determinate bar + % + step
label + seed (text + bar, never color-only); UX-2 input blocked, no
Cancel, no strobe; UX-3 lands on Milky Way with fade + notify;
UX-4 invalid seed surfaces (`Load` dimmed, `Invalid seed '…'` notify)
· DoD verified by: ANALYST (2026-09-19 — per-row audit below, all
evidence reproduced: full gate suite green on this tree, new tests
named per row; one gap noted: windowed modal animation is
headless-untestable — recommended manual smoke in AC-6, no issue
filed since the draw path is unit-covered) · Security reviewed by:
SECURITY (2026-09-19 — pass, no findings: seed input is
`u64`-parse-gated with a notify fallback, no new deps, no new
`unsafe`, no save/IO/network touch; debug-binary surface only).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | One editable seed field (Settings); Milky Way read-only | done | `rg seed_field` → only `app::UniverseSettings` + `main.rs` settings scope; `settings_seed_editor_lives_in_left_dock` asserts UNIVERSE/`Load [Enter]` present, dock `seed 1234` kept, no `Load` in dock | ANALYST |
| 2 | Staged load + modal determinate progress; input blocked | done | `loader::tests` (order, 1/8…8/8 fractions, done-semantics); `loader_modal_overlays_seed_bar_and_step` asserts `LOADING UNIVERSE · seed 7` + `13%` + step label; mouse/key/wheel gates in event arms | ANALYST |
| 3 | Lands on Milky Way + fade + notify; `R`/`--seed` same path | done | `exec_load_step(Finalize)` + `finish_load` code; both `R` arms + Enter + Settings Load + boot call `begin_load` (`rg` zero `load_galaxy_seed`); headless `--seed 7` → `galaxy_seed=7` ok | ANALYST |
| 4 | Invalid seed: dimmed Load, Enter notify | done | `settings_load_button_dims_on_invalid_seed` (C_BTN_OFF on `abc`); Enter-invalid → `Invalid seed '…'` notify path (code-reviewed, same parse gate) | ANALYST |
| 5 | Quality gates green | done | `cargo fmt --check` ok; `clippy --workspace --all-targets --all-features -D warnings` ok; `cargo test --workspace --all-targets` ok (289 engine + 23→26 debug incl. 3 new + tools/tests suites); `cargo test --doc --workspace` ok; `game`, `game_debug --headless` (±`--seed 7`), `game_tools --headless --tier low` ok | ANALYST |
| 6 | Docs sweep; links resolve | done | `controls.md` (Settings UNIVERSE + Milky Way seed lines), `milestones/README.md` v0.3.2 row, `fx.rs` staged-loader note, techstack `0.33.3`; new milestones link resolves to this folder; AGENTS.md unchanged (invariants untouched) | ANALYST |

## Acceptance criteria

- AC-1 (UX-1…UX-4): loader matches the UX decision in notion Roles.
- AC-2: `R` on Game Demo still reseeds in place (no tab switch).
- AC-3: `--seed N` boot shows the loader on first frames; headless
  output unchanged.
- AC-4: no hunk of the
  `issue-2026-09-18-2122-target-highlight-missing` work is modified
  (verified: that work committed as `16a0a43` mid-flight; this
  feature's `git diff` hunks do not intersect its four `main.rs`
  hunks nor its docs hunks).
- AC-5: `App::with_viewer` default still opens seed 1234 everywhere
  (field mirrors it — `settings_seed_field_mirrors_default_seed`,
  headless `galaxy_seed=1234`).
- AC-6 (ANALYST note): windowed smoke recommended — open the viewer,
  `F3`, type a seed, Load → modal with % + step → lands Milky Way;
  `R` on Milky Way; invalid text dims Load. Headless CI cannot
  animate the modal; the draw path is unit-covered instead.

## Risks & Next steps

- Risk: staged steps could interleave with transit/fly-to ticks
  mid-load (journey/system replaced under a live transit) — mitigated:
  input blocked, `transit_acc` reset in the system-upload step, and
  tick order (loader advances before UI build in `draw_main`).
- Risk: `--seed` boot double-generates (default seed buffers built,
  then replaced) — accepted: one wasted gen at startup, single code
  path wins.
- Next: implement SSL-001…SSL-008 in order; ANALYST audit +
  SECURITY review; single commit on `v0.3.2` at `done`.
