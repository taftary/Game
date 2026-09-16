# Notion — debug-ui-reorganize

## Status

`in-review` (plan: [`plan.md`](plan.md) — all REORG todos implemented,
local gates green; needs ANALYST DoD audit + SECURITY review, plus a
manual windowed run for DoD 2–3)

## Context

The `game_debug` viewer grew three views onto one screen — a `ViewFocus`
cycle (sphere → UV net → flat chunk map) with a preview thumb — plus four
nav entries (Sphere Viewer, FPS, Console, Inspector) where the last three
are stubs. Every key hint is always visible, even when its mode is off.
Related done features: [`../debug-screens/`](../debug-screens/),
[`../debug-sphere-viewer/`](../debug-sphere-viewer/),
[`../sphere-uv-debug/`](../sphere-uv-debug/),
[`../debug-player-view/`](../debug-player-view/). The flat chunk map
itself ([`../chunk-flat-view/`](../chunk-flat-view/)) is cancelled by this
feature (visual-debug only; the engine-side `render::chunk_flat`
projection stays for player streaming).

## Problem & Needs

- The flat view is unused visual-debug weight: own shader, pipelines,
  buffers, arrow-key orbit, picking branch, headless asserts.
- The UV net deserves its own screen instead of sharing the sphere
  screen via the thumb/swap cycle (`ViewFocus`, `V` key).
- Inputs must appear only when needed: player hints only with player
  mode on, camera presets only on the sphere screen, density slider only
  in Checker mode, UV controls only on the UV screen.
- FPS / Console / Inspector belong in a separate OS window (own nav),
  not as tabs of the main viewer window.

## Goals

1. Flat view removed from the viewer (state, rendering, input, tests,
   docs); engine `render::chunk_flat` untouched.
2. UV net is its own main-window screen (`F2`); sphere screen is pure
   3D (`F1`); no thumb, no `ViewFocus`, no `V` key.
3. Panel inputs are contextual per the rules above.
4. Second OS window hosts FPS (real frame-time counter) + Console /
   Inspector (stubs for now) with window-local nav (`1/2/3`).

## Non-goals

- Implementing the log console (ADR-009 tracing layer) or the state
  inspector beyond placeholders.
- Touching `game_engine`, `game`, or player-streaming behavior.
- Multi-window for the game itself (desktop-only dev tool).

## Users / Stakeholders

Developers inspecting the planet mesh (only user of `game_debug`).

## Roles

Author: PO. UX consulted (required if player-facing): n-a (dev tool).
ARCHITECT consulted (required if cross-module): yes (two-window
`main.rs` shell).

## Functional requirements

- `MainScreen { SphereViewer, UvNet }` (`F1`/`F2`); `ToolsScreen { Fps,
  Console, Inspector }` (clicks + `1/2/3`, tools window only).
- Sphere screen: left VIEW (camera presets) + SHADER + OVERLAYS
  (wireframe/pentagons/seams); right INPUTS/SELECTION/STATS.
- UV screen: left SHADER + OVERLAYS (UV wire/seams); right dock shared.
- Density slider row only in Checker mode; player rows only when the
  player is active; preset keys only on the sphere screen.
- Tools window close hides it (`F3` on the main window reopens);
  main-window close / `Esc` exits.
- FPS tab shows live fps + avg/max ms + sparkline.

## Non-functional requirements

- No new dependencies (winit 0.30 + vulkano 0.35 already cover
  multi-window); pipelines/allocators/atlas shared, per-window
  surface/swapchain/framebuffers/descriptor sets.
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

1. No `ChunkFlat` / `chunk_flat` / `ViewFocus` / thumb references remain
   in `crates/debug` (engine module excluded).
2. `F1` = sphere screen, `F2` = UV screen; each screen shows only its
   own controls per the contextual rules.
3. Second window opens with the app, hosts the three tools tabs, FPS
   tab shows live numbers; closing it hides, `F3` reopens.
4. `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace
   --all-targets`, `game_debug --headless` all pass.
5. Docs updated: `rendering.md`, `architecture.md`, `controls.md`,
   version bump, AGENTS.md invariant line.

## Constraints & Assumptions

- `game_debug` is desktop-only; multi-window code stays in
  `crates/debug/src/main.rs`.
- Player streaming (`visible_hemisphere`) is decoupled from the removed
  flat view and keeps working unchanged.

## Open questions

None (resolved with the requester before implementation: single tools
window with 3 tabs, thumb removed entirely, contextual hints, F1/F2 +
window-local keys).
