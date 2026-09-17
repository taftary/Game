# Notion — exposure-tone-mapping

## Status

`draft`

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§9.2. Decided by ADR-021 in [`../../docs/decisions/`](../../../docs/decisions/).
Sun vs background starlight spans ~10 orders of magnitude; surface
daylight vs deep space ~30 stops. No fixed exposure covers both.

## Problem & Needs

- A fixed exposure either blinds the player near the Sun or erases
  stars in deep space.
- Stars must not pop at a fixed altitude during twilight transitions.

## Goals

1. Physically driven auto-exposure keyed to the dominant light source
   in view: Sun, planet albedo, or ambient starlight when nothing
   brighter dominates.
2. Filmic tone mapping (ACES or comparable), never hard clipping.
3. Gradual dark adaptation: stars fade in as sky luminance falls across
   the effective scotopic threshold — civil → nautical → astronomical
   twilight behavior.

## Non-goals

- The physical depth-cue models themselves (owned by `depth-cueing`).
- Zodiacal light source term (owned by `zodiacal-light`) — but it must
  feed this exposure path.
- Waypoint transition scripting (owned by `waypoint-transitions`).

## Users / Stakeholders

- Players: readable scene at every scale without manual exposure.
- UX: adaptation pacing is a feel-critical player-facing behavior.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): pending
— required before `draft → planned` (adaptation pacing/feel). ARCHITECT
consulted (required if cross-module): pending (render pipeline + light
sources).

## Functional requirements

- Dominant-source selection (Sun / albedo / starlight) with hysteresis.
- Auto-exposure curve + ACES-class tone mapper in the post chain.
- Dark-adaptation model: star visibility as a function of sky luminance
  with twilight-stage behavior.

## Non-functional requirements

- No visible exposure pops at dominant-source switches (the 6 → 5
  solar-system → neighborhood transition is the hardest case, spec
  §9.3).
- Quality gates + `--headless` smoke stay green.

## Definition of Done

- [ ] Sun-disk to Sun-as-point transition (waypoint 6 → 5) with smooth
  exposure handoff (capture evidence).
- [ ] Twilight sequence shows progressive star fade-in, no altitude
  pop.
- [ ] Tone mapper never clips test HDR scenes (validation captures).

## Constraints & Assumptions

- ADR-021 draft becomes binding here.
- All environment source terms (zodiacal, extinction, scattering) must
  convert to linear radiance **before** this stage (spec §9.4 rule).

## Open questions

- Exact ACES variant vs mobile cost (ties to ADR-007 tiers).
