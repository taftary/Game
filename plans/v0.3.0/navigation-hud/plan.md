# Plan — navigation-hud

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Pure HUD view model

ARCHITECT: add `game::hud` as a pure presentation module. It reads
`ShipState`, `CompressionClock`, fly-to accessors, and handoff events without
mutating simulation state or introducing a renderer/UI dependency.

### Phase 2 — Hysteresis and readouts

TECHLEAD: build the four readouts risk-first: SOI event lifecycle and target
absence/label fallback first, then frame and time formatting. Keep transient
SOI state owned by the HUD model, not the engine simulation.

### Phase 3 — Scripted headless evidence

Extend the headless `game` demo with a deterministic HUD trace covering frame,
time, SOI, and fly-to changes. Add an on/off simulation-hash audit proving
that collecting HUD frames does not alter the simulation trace.

### Phase 4 — Verification and documentation

Run all workspace gates, record UX/ANALYST/SECURITY evidence, and document the
shipping model, controls/readouts, and deferred pixel binding.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| NH-20260917-001 | done | Add pure `Hud`, `HudInputs`, and `HudFrame` types with frame/time/target formatting. | Goals 1, 2, 4 |
| NH-20260917-002 | done | Implement SOI approach/enter/exit messaging and no-repeat hysteresis behavior. | Goal 3; Functional requirements |
| NH-20260917-003 | done | Add deterministic HUD trace and simulation hash on/off audit to the headless game demo/tests. | DoD 1-2 |
| NH-20260917-004 | done | Update architecture, controls, techstack version, milestones, and lifecycle evidence. | DoD 1-3 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-17) · Todos approved by: TECHLEAD
(2026-09-17) · UX acceptance rows: UX (2026-09-17) · DoD verified by: ANALYST
(2026-09-17) · Security reviewed by: SECURITY (2026-09-17).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | All four elements live with simulated frame/time/SOI/target changes. | verified | `hud-trace` from `cargo run --bin game` plus `game::hud` tests. | ANALYST 2026-09-17 |
| 2 | HUD disabled produces identical simulation traces. | verified | `free_flight_hides_target_and_hud_is_read_only` and pure-input audit. | ANALYST 2026-09-17 |
| 3 | SOI message hysteresis is free of repeat spam. | verified | `soi_message_holds_and_clears_without_repeats` plus engine monitor contract. | ANALYST 2026-09-17 |

## Acceptance criteria

- Frame and time readouts are always available and use the active frame's
  display name and unit system.
- SOI indicator strength follows current blend weight; messages show once per
  crossing and clear only after the engine's exit threshold.
- Target readout is hidden without an active target and otherwise displays the
  caller label or canonical coordinates, remaining distance, and ETA.
- HUD collection cannot mutate ship, clock, handoff, or fly-to state.
- The headless trace is deterministic and contains all four overlay elements.

## Risks & Next steps

- The release `game` binary has no windowed shell yet, so this feature ships a
  view model and evidence trace; pixel rendering binds when that shell lands.
- `Target` and `BodyId` have no names in the engine API. Caller labels and
  canonical/numeric fallbacks prevent an engine API expansion in this feature.
- ADR-005 remains open and is not silently resolved here.
- SECURITY review (2026-09-17): no new dependency, unsafe code, file I/O,
  serialization, network surface, or mutable simulation input was introduced.
