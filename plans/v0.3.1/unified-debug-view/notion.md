# Notion — unified-debug-view

## Status

`in-review` (DEV gates green + evidence in `plan.md`; awaiting
ANALYST DoD verification + SECURITY review per `plans/README.md`
role gates)

## Context

`game_debug` grew two OS windows sharing one Vulkan device
(`plans/v0.0.1/debug-ui-reorganize`): a viewer window (Galaxy Map /
System Map / Planet View on `F1`–`F3`) and a tools window (FPS /
Console / Inspector / Transitions / Scale / Dimensions on `1`–`6`).
The tools tabs duplicate each other and the viewer: the Scale tab
lists the ten waypoint dimensions with active flags plus a
transition-event log, the Dimensions tab re-lists L1–L5 dimension
views, the Transitions tab repeats the current transition + history,
and Galaxy/System map data renders in both windows. PO decision
(2026-09-17): rebuild the debug view from scratch as v0.3.1 — one
window, grouped dimension navigation, a single overlay widget for
dev tools.

## Problem & Needs

- Two windows split focus ("focus the tools window, then press a
  key") and the duplicated tabs force the developer to reconcile
  three versions of the same state (Scale vs Dimensions vs
  Transitions; viewer maps vs Dimensions L2/L3).
- There is no player-facing reference inside the debug shell: work
  on the navigation core cannot be eyeballed against the shipped
  experience without leaving the tool.
- Every key needs a discoverable button and every button a key
  (PO rule); today bindings are hardcoded across `main.rs` with no
  single listing.

## Goals

1. **One window.** The tools window is deleted; one `winit`
   window, one event loop, one nav model.
2. **Three top-level items:** `GAME DEMO` (default landing tab) ·
   `DIMENSIONS: <active layer>` (dropdown of the 10 waypoints, live
   breadcrumb) · `SETTINGS`.
3. **Interactive Game Demo tab:** renders the current journey layer
   presentation-accurately with the shipping HUD
   (frame/time/SOI/target), zero debug data, chrome hidden by
   default; WASD walk, camera modes, E/T/Q travel drive the shared
   `Journey` the debug tabs inspect.
4. **Absorbed views:** Galaxy Map → Milky Way tab, System Map →
   Solar System tab, Planet View → Earth tab, with their existing
   docks (260/300 px, padded) and content keys unchanged.
5. **Single dev widget:** one fixed bottom-right overlay, three pure
   sub-tabs (FPS / Console / Inspector), `` ` `` toggles, `F6`/`F7`/
   `F8` jump to a sub-tab; Console is fed the transition-event log;
   transition pill floats bottom-center while a transition is in
   flight.
6. **Always-visible top bar + two independent dock toggles**
   (`F9`/`F10`); the bar hosts three toggle buttons at its right end
   (left dock, right dock, dev widget with FPS value).
7. **Key↔button parity for all actions** via one `Action` registry;
   Settings → Controls renders it grouped and read-only; every
   button label shows its key. Chrome keys never shadow game keys.
8. **No blank pages:** inactive dimensions render last state +
   `INACTIVE` badge; contentless dimensions show an explicit
   placeholder.

## Non-goals

- Remappable key bindings (fixed bindings for v0.3.1; registry is
  built remap-ready).
- Pixel-parity with the final renderer (demo is
  presentation-accurate on current content; v0.2.0 visuals land as
  they ship).
- Draggable/resizable widget (fixed corner for v0.3.1).
- 3D content for the 7 non-absorbed dimension tabs (placeholders;
  per-dimension content is future per-feature work).
- Player-facing release shell (demo tab is presentation-accurate
  but still dev-gated inside `game_debug`).

## Users / Stakeholders

- Developers (only) building and validating the navigation core;
  strip or gate before any release.
- Players (indirect): the Game Demo tab is presentation-accurate,
  so it previews the shipped experience — UX consulted for that
  surface (see Roles).

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing):
yes (2026-09-17 design review: S1 live breadcrumb, S2 transition
strip + console log, S3 FPS on toggle button, S4+S5 key rules,
S7 no blank pages adopted; S6 reset-layout rejected — recorded in
`plan.md` acceptance criteria). ARCHITECT consulted (required if
cross-module): yes — debug-crate restructure, `game::hud` and
engine waypoint APIs consumed read-only, no simulation mutation.

## Functional requirements

- Single window; `MainScreen`/`ToolsScreen` two-window model gone;
  `game_debug --headless` boots the new shell.
- Top bar `F1`/`F2`/`F3`; dropdown captures digits `1`–`0`;
  breadcrumb always shows the active journey layer (`active_waypoint`).
- Dev widget `` ` `` toggle, `F6`/`F7`/`F8` sub-tabs; console log
  fed from `TransitionPanel` history; strip visible only in flight.
- `F9`/`F10`/`F11` dock toggles; corner strip always visible with
  4 toggle buttons; `Esc` unwinds (dropdown → widget focus), no
  longer quits the app.
- Game Demo tab: journey-layer 3D + HUD lines, interactive, chrome
  hidden by default; drives shared `Journey`.
- Milky Way / Solar System / Earth tabs mount the absorbed views
  with existing docks/keys; other 7 tabs show placeholders with
  `INACTIVE` badge when not the active layer.
- Settings → Controls: grouped read-only registry listing, every
  row clickable; `Action` parity test (key + label + group).

## Non-functional requirements

- Quality gates in `docs/techstack/quality.md` stay green
  (`cargo test --workspace --all-targets`, clippy, fmt, headless).
- Debug overlay cost within existing debug budgets; no new
  third-party dependencies.
- Rendering invariants in `docs/techstack/rendering.md`
  (camera/screen-space contract) preserved verbatim.

## Definition of Done

- [ ] Single window; tools window + old screen enums gone; headless
  smoke passes on the new shell.
- [ ] 3 top-level items; dropdown lists 10 waypoints with active
  marker + text; digits select while open.
- [ ] Game Demo tab interactive, presentation-accurate, HUD on,
  chrome hidden by default; travel there reflects in debug tabs.
- [ ] MW/SS/Earth absorbed with docks/keys; inactive dims show
  badge; empty dims show placeholder (no blank pages).
- [ ] Widget with 3 sub-tabs in fixed corner; console shows
  transition log; pill shows in-flight transition only, bottom-center.
- [ ] Top bar always visible with 3 in-bar toggle buttons; `F9`/`F10`
  dock toggles; FPS on widget button; solid-black bordered dropdown
  drawn on top of all chrome (shadow, hover lift, separators).
- [ ] Registry parity test green; Controls section lists every
  action grouped with keys; chrome keys audited against game keys.
- [ ] Docs sweep done (`controls.md`, `architecture.md`,
  `rendering.md`, `journey.md`, `roles/ux.md`, `roles/security.md`,
  techstack version `0.32.0`, milestones v0.3.1 section, ADR-022);
  every touched link resolves.

## Constraints & Assumptions

- Depends on `game::hud` (v0.3.0 `navigation-hud`), `Journey`
  public APIs, and waypoint transition events — all shipped;
  read-only consumption, no simulation mutation from debug UI.
- Follows `game_debug` conventions: contextual panels, headless
  gate, no new third-party dependencies.
- `Esc` no longer quits; exit is the window close button
  (documented in `controls.md`).
- Each feature lands as exactly one commit on branch `v0.3.1`
  (`plans/README.md` §7); docs-only commits (`docs:`) separate.

## Open questions

None — all design questions resolved in the 2026-09-17 PO/UX
session (see `plan.md` resolved design notes). Remaining key
detail: `F5` stays the Earth-tab twilight cycle (collision audit),
so widget sub-tabs use `F6`–`F8`.
