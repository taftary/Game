# Notion — free-flight-navigation

## Status

`draft`

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§2. Default play state is free flight with real physical momentum; the
automated mode is select-to-focus. Depends on `frame-hierarchy` (state
vectors live in the active frame) and `scale-physics` (thrust integrates
against the per-scale model).

## Problem & Needs

- The player needs direct control: thrust, orientation, inertia —
  piloted craft with true physical dynamics, not scripted movement.
- The player needs a low-effort alternative: select a region/object and
  let the engine fly there.
- Scripted pacing must never leak into free flight.

## Goals

1. Free flight: player-controlled thrust + orientation with real
   momentum in the active frame.
2. Select-to-focus: select target → engine computes automated fly-to
   trajectory with "constant time per decade" easing/duration applied
   **only** to the transition.
3. Both modes share one ship state vector (6D position/velocity +
   orientation quaternion) resumable by `autosave-persistence`.

## Non-goals

- Craft definition numbers (mass range, thrust model, propellant, max
  acceleration, assists, collision, landing/atmospheric scope) — open
  product item, spec §10; this feature codes against tunable parameters.
- HUD reticle/ETA display (owned by `navigation-hud`).
- SOI blending (owned by `soi-handoff`).

## Users / Stakeholders

- Players (primary): both piloting and being flown.
- UX: transition easing and target selection are player-facing.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): pending
— required before `draft → planned` (flight feel + selection UX).
ARCHITECT consulted (required if cross-module): pending (touches
simulation + input + camera).

## Functional requirements

- Thrust/orientation input mapped to craft acceleration in the active
  frame; momentum conserved across frame transitions.
- Target selection API (region or object) → trajectory plan → automated
  execution with cancellable commit (consistent with `gameplay.md`
  transit rules).
- Fly-to duration = constant time per decade of distance (spec §1
  decade-gap table), as easing + duration function.
- Mode transitions (free ⇄ automated) explicit and interruptible.

## Non-functional requirements

- No fixed pacing in free flight (spec §2).
- Deterministic resume: active transition plan persisted (ADR-004).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Free flight with momentum in at least the solar-system and
  planetary frames (integration evidence).
- [ ] Select-to-focus completes a multi-decade fly-to with documented
  easing; cancellation restores free flight cleanly.
- [ ] Ship state survives save/load mid-transition (with
  `autosave-persistence`).

## Constraints & Assumptions

- Physics model per scale follows ADR-018; symplectic integrator for
  integrated regimes.
- Camera behavior during transitions must respect the rendering
  invariants (`docs/techstack/rendering.md`).

## Open questions

- Craft parameter ranges (spec §10 open item).
- Navigation assists scope (open item).
