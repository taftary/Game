# Notion — navigation-hud

## Status

`done`

## Context

Milestone v0.3.0 (player-facing layer), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§10 (HUD and navigation overlay — resolved for v0.4 scope). The player
needs persistent orientation across frames, time states, SOI handoffs,
and fly-to targets. Distinct from `scale-debug-screens` (developer-only);
this is the shipping overlay.

## Problem & Needs

- Without frame/time/target readouts the player is lost across 26
  decades — the spec resolves the full minimal overlay for v0.4 scope.

## Goals

Ship the spec §10 minimal overlay:

1. **Active-frame indicator:** frame name, local unit system, primary
   reference body where applicable, camera/ship distance or altitude in
   that frame.
2. **Time state:** real-time or current compression ratio +
   state-machine mode (`time-compression`).
3. **SOI handoff indication:** subtle indicator whose strength follows
   blend weight; "Approaching / Entering [body] SOI" above weight 0.2
   with hysteresis against noisy repeats (`soi-handoff` events).
4. **Select-to-focus target:** reticle/directional marker, resolved
   target name/identifier, distance, ETA throughout an automated fly-to
   (`free-flight-navigation`).

## Non-goals

- Debug screens (owned by `scale-debug-screens`).
- UI framework decision (ADR-005 open; spec §10 allows a fast overlay
  implementation).
- Map screens (v0.0.1 `universe-maps` features own those).

## Users / Stakeholders

- Players (primary): every navigation action reads through this overlay.
- UX: the overlay is the player-facing face of the whole navigation core.

## Roles

Author: PO (2026-09-17). UX consulted (2026-09-17): approved layout zones,
legibility, frame-local units, hidden absent targets, and approach/enter/exit
SOI messaging. ARCHITECT consulted (2026-09-17): approved a pure
`game::hud` read model consuming only public engine APIs; no simulation or
rendering boundary changes.

## Functional requirements

- Four overlay elements above, fed by public read APIs of the v0.1.0
  features.
- **Decoupled from simulation state** — must not change physics,
  timing, or frame-selection behavior (spec §10 hard rule).
- Hysteresis on the SOI message (threshold 0.2 blend weight).
- `game::hud` is a headless-testable view model; pixels bind when the
  windowed release shell lands. The current DoD evidence is a scripted trace.

## Non-functional requirements

- Overlay cost within UI budget in `docs/techstack/quality.md`.
- No HUD flicker during SOI handoffs (joint acceptance with
  `soi-handoff`, spec §3).
- Quality gates stay green.

## Definition of Done

- [x] All four elements live with simulated frame/time/SOI/target
  changes (capture evidence).
- [x] Decoupling audit: HUD disabled ⇒ identical simulation traces.
- [x] SOI message hysteresis demonstrably free of repeat spam.

## Constraints & Assumptions

- Consumes only public read APIs; write access to simulation is a
  SECURITY/ARCHITECT finding.
- UX layout: frame indicator top-left, time state top-right, SOI indicator
  center-top, and target reticle/readout bottom-center or edge-directed.
- Presentation is monochrome with one accent, readable at phone distance, and
  never communicates state by color alone. An absent target hides its element.
- Target and body labels are caller-supplied when available; otherwise target
  coordinates and `body-{id}` are displayed.

## Resolved design notes

- ADR-005 remains open. This feature supplies the decoupled overlay model and
  headless evidence without selecting a UI framework.
- SOI text shows on `Approaching`, changes to `Entering` on `Entered`, holds
  during the handoff, and clears on `Exited`; strength follows blend weight.
