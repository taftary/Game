# Notion — scale-debug-screens

## Status

`draft`

## Context

Milestone v0.1.0 (navigation core, dev tooling), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§1 (10 waypoint dimensions) + §9.3 (waypoint-to-waypoint transitions).
PO requirement (2026-09-17): **one debug screen per dimension, plus one
global screen grouping all dimensions and transitions.** Extends the
`game_debug` tab model from
[`../debug-screens/`](../../v0.0.1/debug-screens/) (done) and
[`../debug-ui-reorganize/`](../../v0.0.1/debug-ui-reorganize/) (in-review: main
window F1–F3 tabs Galaxy Map / System Map / Planet View, tools window on
F4).

## Problem & Needs

- The navigation core (`frame-hierarchy`, `soi-handoff`,
  `time-compression`, `scale-physics`) spans 10 dimensions; debugging
  any one of them needs a dedicated live view of that scale's state —
  today no per-dimension visibility exists.
- Cross-dimension behavior (frame transitions, SOI handoffs, fly-to
  transitions) needs one global overview: which dimension is active,
  what transition is in flight, what happened last.
- "Transactions" in the PO request is read as **transitions** (frame /
  SOI / waypoint transitions, spec §9.3) — confirm at plan time.

## Goals

1. **Per-dimension screens (10)** — one per spec §1 waypoint: Cosmic
   Web, Galactic Supercluster, Local Group, Milky Way, Solar
   Neighborhood, Solar System, Planetary Surface/Earth, Regional
   Aerial, Facility Exterior, Facility Interior. Each shows that
   dimension's live state: active frame + units, camera/ship position
   in-frame, local physics model, content stats (tiles/objects), and
   the waypoint ⇄ journey-level mapping (L1–L8).
2. **Global screen** — all 10 dimensions at a glance (which is active,
   camera decade position within it) **plus a transitions panel**:
   current/pending transitions (frame switch, SOI handoff blend weight,
   select-to-focus fly-to progress, time-compression mode) and a
   scrolling transition-event log.
3. Screens live in `crates/debug` (`game_debug`), wired into the
   existing tab/nav model; never player-facing.

## Non-goals

- No gameplay HUD (player-facing overlay is `navigation-hud`).
- No new rendering pipelines per dimension beyond debug overlays;
  dimension content itself lands with the v0.1.0/v0.2.0 features.
- No multi-window expansion beyond the existing main + tools model
  (desktop-only dev tool).

## Users / Stakeholders

- Developers (only) building and validating the navigation core; strip
  or gate before any release.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): n-a
(dev tool, same precedent as `debug-ui-reorganize`). ARCHITECT consulted
(required if cross-module): pending — consumes frame/SOI/time APIs across
modules.

## Functional requirements

- Dimension screen ×10, addressable from the debug nav; each renders
  even when its dimension is inactive (shows "inactive" + last state).
- Global screen: dimension matrix (10 rows: waypoint, scale, active
  flag, camera position in-frame) + transitions section (live blend
  weights, fly-to ETA, time-compression mode) + event log (timestamped
  frame/SOI/fly-to/time events).
- Data sourced from `frame-hierarchy`, `soi-handoff`,
  `time-compression`, `free-flight-navigation` public APIs — read-only,
  decoupled (no simulation mutation from debug UI).
- Headless smoke: `game_debug --headless` boots with the new screens
  registered.

## Non-functional requirements

- Debug overlay cost within existing debug budgets; screens render at
  interactive rates on the dev floor device.
- Quality gates in `docs/techstack/quality.md` stay green
  (`cargo test --workspace --all-targets`, clippy, fmt, headless).

## Definition of Done

- [ ] 10 per-dimension screens registered and reachable in `game_debug`,
  each showing live in-frame state (screenshot/headless evidence).
- [ ] Global screen shows all dimensions + live transitions + event
  log; a scripted frame transition and SOI handoff appear in the log.
- [ ] No simulation state mutated from debug UI (audit evidence).
- [ ] Docs updated: `docs/techstack/architecture.md` (debug crate),
  `docs/game/controls.md` (new debug nav), techstack version bump.

## Constraints & Assumptions

- Depends on v0.1.0 core APIs shipping first (frame/SOI/time state must
  exist to display); UI skeleton can land earlier behind stubs.
- Follows `game_debug` conventions: contextual panels, headless gate,
  no new third-party dependencies.

## Open questions

- Confirm "transactions" = transitions (not, e.g., resource/save
  transactions) — if save transactions are also wanted, the autosave
  event feed joins the log.
- Tab budget: 11 new screens exceed the F-key model — nav scheme
  (sub-tabs vs list) decided at plan time with ARCHITECT.
