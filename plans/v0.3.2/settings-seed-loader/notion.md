# Notion — settings-seed-loader

## Status

`done` (all gates green, ANALYST DoD verified + SECURITY reviewed
2026-09-19 per `plans/README.md` role gates; uncommitted on branch
`v0.3.2` — lands as exactly one commit per the version branch rule)

## Context

`game_debug` (single-window debug shell, ADR-022 as extended by ADR-023)
regenerates the whole procedural universe from one master seed
(`engine::seeding`, hierarchical seeding v0.1.0). The only editable seed
field lives in the Milky Way dimension dock (`GalaxyMapView::seed_field` +
Load button, Enter/Apply); the Cosmic Web and Solar System docks show
read-only seed readouts. The Settings screen (`F3` / top bar, reachable
from every tab) holds only the read-only Controls registry.

Loading a universe (`ViewerApp::load_galaxy_seed`) is fully synchronous on
the UI thread — galaxy regen, journey reset, system load, cosmic reseed,
three GPU re-uploads — followed by a fade + notify banner. There is no
progress surface: `crates/debug/src/fx.rs` explicitly documents "no
literal spinner" for universe loads.

## Problem & Needs

- The seed is editable in a dimension dock instead of a single,
  always-reachable place: a developer comparing universes must navigate
  to the Milky Way tab to type a seed, while read-only copies sit in
  other docks.
- Clicking Load freezes the shell with zero feedback until the fade —
  on slower tiers the stall looks like a hang.
- The `fx.rs` "no spinner" note is now a stale decision: the shell
  needs a loader, without pre-empting the M2 async-streaming design
  (worker threads stay out of scope).

## Goals

1. **Single editable seed home:** the seed text field + Load live only
   in Settings (reachable from every screen via `F3`/top bar); the
   Milky Way dock keeps its read-only `seed N` row like the other docks.
2. **Loader progress on every universe load:** a staged load (fixed
   ordered steps, one per frame, UI stays alive) drives a determinate
   modal progress bar; input is blocked while loading.
3. **One load path:** Settings Load, Enter-on-field, `R` re-roll on the
   Milky Way tab, and the `--seed` boot flag all run the same staged
   machine and land on the Milky Way tab with fade + notify.

## Non-goals

- Async worker threads for generation (still M2 descent streaming).
- Cancel button (loads are short; no async infra for safe cancel).
- Re-seeding `CatalogSky` on load (sky keeps its boot seed — existing
  behavior, unchanged).
- Release-binary settings UI (debug shell only; strip-or-gate rule stands).
- Remappable keys, new registry actions.

## Users / Stakeholders

- Developers (only) building and validating the navigation core; the
  shell is dev-gated and never ships.
- Players (indirect): none — no player-facing surface changes. UX is
  consulted anyway at the user's explicit request, scoped to the loader.

## Roles

Author: PO (user need 2026-09-18; notion written by agent as PO).
UX consulted (required if player-facing): yes — user delegated the
loader design 2026-09-18; UX decision recorded: **determinate modal
progress bar, center-viewport dimmed panel, `LOADING UNIVERSE · seed N`
+ bar + % + live step label, input blocked while loading, no Cancel,
no strobe, text + bar (never color-only)**; completion keeps the
existing fade + notify and lands on the Milky Way tab.
ARCHITECT consulted (required if cross-module): yes — debug-crate-
internal restructure only (`debug::{app,loader}`, `game_debug`
binary); no engine/game boundary crossed, no locked choice changed,
no ADR required.

## Functional requirements

- Settings screen gains a UNIVERSE section above Controls: seed field
  (62% width, same widget as today) + Load button labeled
  `Load [Enter]` (S5 self-documenting rule), dimmed while the text is
  not a valid `u64`; hint row shows the active universe seed.
- The field always mirrors the loaded universe seed after
  construction / load / re-roll.
- Milky Way dock: SEED editor section removed; read-only `seed N` row
  stays; `R: re-roll seed` hint stays.
- Load machine steps (fixed order, one per frame): galaxy regen →
  journey reset → system load (star 0) → cosmic reseed → map upload →
  system upload → cosmic upload → land on Milky Way + fade + notify
  (`Seed N · M stars`; `--seed` boot appends `· --seed flag`).
- While a load is in flight: modal overlay (dim + panel + bar + %
  + step label); all mouse/keyboard input ignored (no cancel).
- Enter with the Settings field focused starts the staged load;
  invalid text notifies `Invalid seed '…'` (existing failure surface).
- `R` on the Milky Way tab runs the staged loader (`seed + 1`);
  `R` on the Game Demo tab is unchanged (in-place cosmic reseed, no
  tab switch — the loader always lands on Milky Way, which would be
  the wrong destination mid-demo).
- `--seed N` boot shows the loader on the first frames instead of
  regenerating inline; headless path unchanged (no frames, no loader).

## Non-functional requirements

- Quality gates in `docs/techstack/quality.md` stay green
  (`cargo fmt --check`, `clippy --workspace --all-targets
  --all-features -D warnings`, `cargo build --workspace`,
  `cargo test --workspace --all-targets`, `cargo test --doc
  --workspace`, `game` / `game_debug --headless` /
  `game_tools --headless --tier low`).
- No new third-party dependencies; no new `unsafe`.
- Rendering invariants in `docs/techstack/rendering.md` untouched
  (UI-pass overlay only; no camera/projection/picking/marker change).
- Debug overlay cost within existing budgets (loader = a few dozen
  UI verts, asserted by the existing vertex-budget assert).

## Definition of Done

- [ ] Exactly one editable seed field exists in the shell, on the
      Settings screen; Milky Way dock shows the seed read-only.
- [ ] Settings Load and Enter start a staged load; the modal shows
      determinate progress (0→100%), the seed, and the live step;
      input is blocked until it completes.
- [ ] Every load lands on the Milky Way tab with fade + notify;
      `R` (Milky Way) and `--seed` use the same machine.
- [ ] Invalid seed: Load dimmed, Enter notifies (failure surface).
- [ ] All quality gates green (evidence: command outputs).
- [ ] Docs sweep done (`controls.md`, `milestones/README.md`,
      `fx.rs` module docs, techstack version bump); every touched
      link resolves; AGENTS.md still accurate.

## Constraints & Assumptions

- Loads stay synchronous-but-staged (one step per frame); true async
  remains M2. Assumes the event loop keeps pumping
  `RedrawRequested` during load (it does — fade/FPS need frames).
- `CatalogSky` keeps its boot seed (unchanged).
- Each feature lands as exactly one commit on branch `v0.3.2`
  (`plans/README.md` §7); docs-only commits (`docs:`) separate.
  Working tree already holds the uncommitted
  `issue-2026-09-18-2122-target-highlight-missing` work — this
  feature does not touch its hunks (draw_player_marker,
  build_cosmic_web_ui, overlay target ring, tests).

## Open questions

None — scope, loader design, landing tab, and `R`/boot behavior
decided with the user 2026-09-18 (see Roles + requirement deltas
above).
