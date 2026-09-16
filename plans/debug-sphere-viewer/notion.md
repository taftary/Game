# Notion — debug-sphere-viewer

## Status

`done` (plan: [`plan.md`](plan.md) — all DoD criteria checked)

## Context

`hex-sphere` is done: `engine::hexsphere::HexSphere::generate(subdivisions,
radius)` produces the pure, deterministic base mesh for every spherical body.
It is verified only by numeric topology invariants (Euler = 2, 12 pentagons,
committed mesh hash) — no developer can actually *see* the mesh, judge
relaxation quality, or feel the cost curve (cells = 10·4^N+2, ×4 per level).
`debug-screens` is done: the `game_debug` crate (lib with `fps` / `console` /
`inspector` stubs + runnable binary) exists, but it explicitly deferred all
screen rendering as an M1+ decision — this notion is that decision. M1
(`vulkano` boot + orbit camera, see `docs/milestones/`) has not started and
is a hard prerequisite: nothing in the workspace renders yet.

## Problem & Needs

- Developers need visual verification of the generated sphere (relax
  artifacts, pentagon distribution, cell uniformity) instead of trusting
  numeric invariants alone.
- Developers need to tweak generation parameters (subdivisions, radius) at
  runtime and regenerate without recompiling, to explore the quality/cost
  tradeoff interactively.
- The debug tool will host several screens (fps/console/inspector stubs
  already exist); it needs a screen-switching UX now so screens don't pile
  up ad hoc later.

## Goals

- `Sphere Viewer` debug screen in the `game_debug` lib: renders the current
  `HexSphere` (dual-cell polygons), orbit camera, wireframe + pentagon
  highlight display toggles.
- Inputs panel: edit subdivisions and radius, explicit Regenerate, read-only
  stats (cells, corners, pentagons, short mesh hash, generation time).
- Navigation menu: `Sphere Viewer` (functional) + `FPS` / `Console` /
  `Inspector` (placeholder pages); click + hotkey switching; viewer state
  preserved across switches.
- UX fully specified in this notion (layout wireframe under Functional
  requirements) so `plan.md` only schedules tasks.

## Non-goals

- No renderer boot: `vulkano` device/swapchain/pipeline work is M1's
  `engine::render`, not this feature.
- No changes to `engine::hexsphere` (stays pure; viewer consumes the public
  API only).
- No real fps/console/inspector implementations — placeholders only.
- No elevation/surface layers, LOD, or chunk streaming.
- Nothing in the release `game` binary; no mobile/touch UX (desktop dev
  tool).

## Users / Stakeholders

- Developers (only). Never player-facing; same rule as `debug-screens`:
  strip or gate before any release.

## Functional requirements

All of the below lives in `crates/debug` (`game_debug` lib + binary).

- Navigation menu: persistent top bar with entries `Sphere Viewer`
  (functional), `FPS`, `Console`, `Inspector` (placeholders). Mouse click
  and hotkeys (e.g. F1–F4) switch screens; exactly one screen visible at a
  time; switching away and back preserves Sphere Viewer state (mesh +
  camera).
- Placeholder screens: centered title + "not implemented yet" text.
- Sphere Viewer layout: left view dock (VIEW / UV DEBUG / OVERLAYS) +
  center 3D viewport (majority of the window) + right data dock
  (INPUTS / SELECTION / STATS). Non-viewer screens reclaim the full
  width (no docks) so viewer inputs stay attached to the viewer
  screen only:

  ```text
  +----------------------------------------------------------------------+
  | [Sphere Viewer] [FPS] [Console] [Inspector]          <- nav menu     |
  +--------- +-----------------------------------------------+-----------+
  | VIEW     |                                               | INPUTS    |
  | [thumb]  |                                               | Subdiv    |
  | Swap (V) |                 3D viewport                     | [ 6   ]   |
  | main: 3D |             orbit camera + current              | -> 40,962 |
  | presets  |              HexSphere mesh                     | Radius    |
  | [Top][Bot]                                             | [ 1.0 ]   |
  | [Right][Persp]                                           | [Regenerate] |
  |--------- |                                               |-----------|
  | UV DEBUG |                                               | SELECTION |
  | Shader.. |                                               | chunk: .. |
  | density  |                                               | player:.. |
  |--------- |                                               |-----------|
  | OVERLAYS |                                               | STATS     |
  | [x] Wire |                                               | cells: .. |
  | [x] Pent |                                               | hash: ..  |
  +----------+-----------------------------------------------+-----------+
  ```

- Viewport: orbit camera (drag to rotate, scroll to zoom); renders the
  current mesh as dual-cell polygons; wireframe overlay toggle (default
  on); pentagon highlight toggle (default on, pentagons tinted).
- Inputs panel:
  - Subdivisions: integer input (slider + field), clamped to 0–8; live cost
    hint showing resulting cell count (10·4^N+2); warning styling above the
    default N=6.
  - Radius: float input, validated `> 0` and finite. Invalid input is
    rejected with an inline hint and never reaches `HexSphere::generate`
    (which panics on invalid radius).
  - `Regenerate` button: explicit apply — no per-keystroke regeneration;
    disabled while any input is invalid.
  - Display toggles: wireframe on/off, pentagon highlight on/off.
  - Stats (read-only, refreshed after each regeneration): cell count,
    corner count, pentagon count (invariant = 12), short mesh hash (8-hex
    prefix of `HexSphere::mesh_hash`), last generation time in ms.

## Non-functional requirements

- All `docs/techstack/quality.md` gates green, including
  `cargo run -p game_debug --bin game_debug` opening the viewer (after M1).
- Zero changes to `game_engine`: no determinism or perf impact; the viewer
  reads only the public `HexSphere` API.
- Regeneration at N ≤ 6 stays interactive (< 1 s per the hex-sphere
  budget); generation time is displayed after each rebuild.
- UI uses the custom swapchain UI per `docs/techstack/stack.md` (`vulkano`
  + `fontdue`); no third-party UI framework without an ADR.
- Desktop-first (Windows/Linux); the debug tool has no mobile target.

## Definition of Done

- [ ] Sphere Viewer renders the current `HexSphere` in `game_debug` with an
  orbit camera; wireframe + pentagon highlight toggles functional.
- [ ] Inputs panel edits subdivisions (clamped 0–8, live cell-count hint)
  and radius (validated `> 0`, finite); invalid input shows an inline hint
  and never panics.
- [ ] Regenerate rebuilds the mesh; stats (cells, corners, pentagons = 12,
  short hash, gen ms) refresh.
- [ ] Nav menu switches between Sphere Viewer and the FPS/Console/Inspector
  placeholder screens; viewer state survives switching.
- [ ] `engine::hexsphere` and the release `game` binary untouched; both
  binaries run.
- [ ] `docs/techstack/architecture.md` debug-crate row updated, techstack
  version bumped; all `quality.md` gates green with evidence recorded in
  `plan.md` DoD verification.

## Constraints & Assumptions

- Hard dependency: M1 (`engine::render` `vulkano` boot + orbit camera, see
  `docs/milestones/`) must land before implementation starts; this notion
  is written now, code comes later.
- Lives only in `crates/debug` per `docs/techstack/architecture.md`:
  developer screens never leak into the release binary.
- `HexSphere::generate` panics on invalid radius — panel validation is the
  guard; the generator is never called with unvalidated input.
- Only the public `HexSphere` API is used (`generate`, cell/corner
  accessors, `mesh_hash`); engine purity is preserved.

## Open questions

- Blocking vs async regeneration at high N (UI freeze vs progress
  indicator) — decide in plan; synchronous is acceptable if the < 1 s
  budget holds.
- Camera mapping: reuse M1's orbit camera controls for consistency? Decide
  in plan.
- Default display mode is dual-cell wireframe (hexagons — the inspection
  target); a primal-triangle view as an extra toggle is a plan-time
  decision.
