# Notion — soi-handoff

## Status

`done`

(2026-09-17: ANALYST DoD-verified + SECURITY reviewed; single commit
on branch `v0.1.0` per `plans/README.md` § *7. Version branch*.)

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§3. Decided by ADR-014 in [`../../docs/decisions/`](../../../docs/decisions/).
Ships crossing between gravitational domains must switch dominant frames
(`frame-hierarchy`) with no player-observable discontinuity.

## Problem & Needs

- Hard SOI switches cause velocity kicks, camera pops, and HUD flicker.
- Stability analysis (moon orbits, rings) needs a different radius than
  patched-conic handoffs — the two are commonly confused.

## Goals

1. Soft patched-conic handoff: position C⁰ continuous, velocity
   approximately C¹ continuous.
2. Eligibility: candidate secondary acceleration ≥ 10⁻³ of primary's.
3. Blend band: 1.05 × r_soi → 0.95 × r_soi, smoothstep weighting.
4. Laplace SOI `a·(m/M)^(2/5)` for handoffs; Hill sphere
   `a·(m/3M)^(1/3)` for stability analysis — both in code, documented.

## Non-goals

- Time-compression behavior during handoff (owned by
  `time-compression`, must not desync).
- HUD indicator rendering (owned by `navigation-hud`; this feature
  exposes blend weight + hysteresis events).

## Users / Stakeholders

- Players: invisible transitions.
- Developers: one tested boundary-crossing path for all bodies.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): yes
(2026-09-17) — indicator subtlety is player-facing, but rendering
belongs to `navigation-hud` (v0.3.0, explicit non-goal here). UX
acceptance for THIS feature = event-payload completeness for the
future HUD: blend weight + 0.2 threshold + hysteresis + body identity
must all be observable from the API (spec §10). Confirmed in
`plan.md`. ARCHITECT consulted (required if cross-module): yes
(2026-09-17) — breakdown in `plan.md` (physics + frames + HUD events).

## Functional requirements

- State-vector transform (position + velocity) before the physics step
  crossing the boundary.
- Acceleration blending across the band with smoothstep weight (no
  linear ramp).
- Blend-weight + enter/exit events with hysteresis (HUD consumes;
  "Approaching / Entering [body] SOI" above weight 0.2, spec §10).
- Completed-handoff event feeds autosave triggers (ADR-004).

## Non-functional requirements

- No position jump, velocity kick, camera discontinuity, or HUD flicker
  (spec §3 acceptance).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Handoff integration test: crossing ship shows C⁰ position /
  ~C¹ velocity continuity within documented tolerance.
- [ ] Laplace vs Hill helpers with regression values (spec §3 note,
  corrected 2026-09-17: Laplace ≈ 0.5–0.7 × Hill for planet–Sun).
- [ ] Hysteresis event log demonstrably free of noisy repeats.

## Constraints & Assumptions

- ADR-014 draft becomes binding here.
- Depends on `frame-hierarchy` active-frame transition API.

## Open questions

- Resolved at plan time (ARCHITECT 2026-09-17, see `plan.md` HOF-007):
  band-edge behavior during an active fly-to — blending always governs
  acceleration; the fly-to plan subscribes to `Entered` and replans.
