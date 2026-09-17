# Notion — time-compression

## Status

`draft`

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§2. Decided by ADR-015 in [`../../docs/decisions/`](../../../docs/decisions/).
Physics must run true real-time near bodies (precision piloting,
~90-minute low orbit) but compress when far from all bodies (galactic
rotation ≈ 225 Myr is meaningless to a player).

## Problem & Needs

- A global time multiplier desynchronizes at sphere-of-influence
  handoffs (spec §2).
- The player needs legible time state at all times.

## Goals

1. Explicit time-compression **state machine** tied to reference-frame
   occupancy (`frame-hierarchy`), never a global multiplier.
2. True real-time inside any body's gravitationally significant radius;
   smooth compression outside.
3. Time state (mode + ratio) exposed to HUD (`navigation-hud`) and
   persisted (`autosave-persistence`, ADR-004).

## Non-goals

- No SOI blending itself (owned by `soi-handoff`).
- No HUD rendering (owned by `navigation-hud`).

## Users / Stakeholders

- Players: legible, non-surprising time behavior.
- Developers: deterministic simulation across compression transitions.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): pending
— required before `draft → planned` (time-state display legibility).
ARCHITECT consulted (required if cross-module): pending (simulation +
frames + persistence).

## Functional requirements

- State machine: named modes (minimum: real-time, compressed) with
  transitions driven by frame occupancy + proximity rules per spec §2.
- Smooth (continuous) compression ramp, no discrete jumps in physics dt.
- Compression mode/multiplier queryable by HUD and included in save
  state.

## Non-functional requirements

- Symplectic integrator stability preserved across compression
  transitions (ADR-018).
- Handoff between compression states must not desynchronize SOI blending
  (ADR-014).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] State machine implemented with unit tests: occupancy change →
  correct mode; no global multiplier path exists.
- [ ] Orbital period in real-time mode matches analytic value within
  tolerance (validation-plan number TBD, spec §10).
- [ ] Compression state round-trips through save/load.

## Constraints & Assumptions

- ADR-015 draft becomes binding here.
- Depends on `frame-hierarchy` frame occupancy API.

## Open questions

- Resolved (PO 2026-09-17): shared constant confirmed — the ADR-014
  10⁻³ acceleration rule is the gravitationally-significant radius for
  both handoff eligibility and compression transitions. See spec §10
  addendum *Resolved for v0.1.0*.
