# Plan — scale-debug-screens

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Read-only scale model

ARCHITECT: keep dimension registration and snapshot formatting in
`crates/debug`; consume `game::journey` and `engine::waypoints` by read-only
access. No simulation, renderer, or release-game boundary changes.

### Phase 2 — Navigation and screen surfaces

TECHLEAD: extend the existing tools-window tab model with a scale overview and
ten selectable dimension surfaces. Preserve the existing F-key and digit
routes, using an in-panel two-column selector for the ten dimensions.

### Phase 3 — Global transition evidence

Expose the active dimension matrix, inactive/last-state labels, current
transition state, and bounded transition/SOI event evidence in the global
surface. Keep all values derived from caller-owned state and bounded display
data.

### Phase 4 — Verification and documentation

Run the headless debug path and unit tests, verify the read-only boundary, and
document the screen registry and controls without adding debug dependencies to
the release binary.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| SDS-20260917-001 | done | Register ten waypoint dimensions plus global scale selection in the debug model. | Goals 1-2 |
| SDS-20260917-002 | done | Add tools-window navigation and clickable per-dimension screen selection. | Functional requirements |
| SDS-20260917-003 | done | Render active/inactive matrix, selected dimension state, and bounded transition/SOI log evidence. | Goals 1-2; DoD 1-2 |
| SDS-20260917-004 | done | Add headless/model tests proving ten registration, one active row, and UI-only selection. | Functional requirements; DoD 3 |
| SDS-20260917-005 | done | Update architecture, controls, milestone, and techstack version documentation. | DoD 4 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-17) · Todos approved by: TECHLEAD
(2026-09-17) · UX acceptance rows: n-a (developer-only tool) · DoD verified
by: ANALYST (2026-09-17) · Security reviewed by: SECURITY (2026-09-17).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | 10 per-dimension screens registered and reachable in `game_debug`, each showing live in-frame state. | verified | `ScaleDebugState::rows`, Scale tab, model tests, and the explicit tools-window 900x720 launch size with digit-5 headless routing check. | ANALYST 2026-09-17 |
| 2 | Global screen shows all dimensions, live transitions, and event log. | verified | `build_scale_tab`, bounded transition/SOI evidence, headless build. | ANALYST 2026-09-17 |
| 3 | No simulation state mutated from debug UI. | verified | Scale UI only mutates `ScaleDebugState::selected`; journey is read-only. | ANALYST 2026-09-17; SECURITY 2026-09-17 |
| 4 | Docs updated: architecture, controls, and techstack version. | verified | Documentation changes in this feature commit. | ANALYST 2026-09-17 |

## Acceptance criteria

- The tools window exposes `Scale` through its nav and digit 5.
- The Scale surface exposes all ten waypoint dimensions and selecting one
  changes only debug selection state.
- The global surface identifies the active journey dimension and labels the
  other dimensions as inactive with last state.
- The displayed transition log is bounded, read-only, and does not affect the
  journey state.
- `game_debug --headless` remains GPU-free and passes the scale model tests.

## Risks & Next steps

- The current journey API exposes map/orbit layers rather than all ten
  physical frames; inactive dimensions therefore show an explicit last-state
  label until frame hierarchy read APIs are exposed to debug.
- The transition/SOI evidence is intentionally display data until the runtime
  event bus connects real handoff events; the bounded display contract is ready
  for that integration.
- ANALYST reproduced the workspace gates and headless evidence. SECURITY found
  no new dependency, unsafe code, file-I/O, serialization, or unvalidated
  input surface.
