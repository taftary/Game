# Notion — exposure-tone-mapping

## Status

`done` (all DoD criteria checked in `plan.md`, ANALYST + SECURITY signed 2026-09-17)

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

UX notes (2026-09-17): dark adaptation must be slower than light
adaptation (mirroring scotopic behavior); star fade-in is gradual
across civil → nautical → astronomical twilight — never a pop at a
fixed altitude; dominant-source switches (hardest: waypoint 6 → 5)
must show no visible exposure step; all pacing constants ship as
tunable calibration, UX acceptance rows land in `plan.md`.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): yes —
adaptation pacing is the player-facing surface here; UX notes under
`Users / Stakeholders` (2026-09-17). ARCHITECT consulted (required if
cross-module): yes — render-pipeline placement confirmed (HDR scene
target + tonemap resolve pass; UI stays LDR post-resolve; `game` never
touches vulkano); ADR-021 already covers the decision, no new ADR
(2026-09-17).
PO sign-off for `draft → planned` (2026-09-17): checklist green —
problem stated without prescribing implementation; non-goals explicit;
DoD criteria verifiable (DoD-1 via the analytic Sun angular-diameter
sweep harness — approved evidence form 2026-09-17, since no waypoint
runtime exists yet (`waypoint-transitions` owns it); DoD-2 via twilight
captures; DoD-3 via never-clip unit tests on synthetic HDR scenes);
open question (ACES variant vs mobile cost) stays open, resolved in
`plan.md` via runtime HDR-format query + tier fallback (ADR-007). The
spec §10 visual/rendering-style open item does not block this feature:
ADR-021 fixes the physical basis, style tunes parameters only.

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
  exposure handoff — evidence: analytic Sun angular-diameter sweep
  harness (headless self-test asserts continuity, no pops) + debug
  captures; approved evidence form 2026-09-17 (no waypoint runtime
  exists yet — `waypoint-transitions` owns it).
- [ ] Twilight sequence shows progressive star fade-in, no altitude
  pop.
- [ ] Tone mapper never clips test HDR scenes (validation captures).

## Constraints & Assumptions

- ADR-021 is binding (2026-09-17): the physical basis (auto-exposure
  keyed to dominant source, filmic mapping, twilight dark adaptation)
  is fixed; PBR-vs-stylized style choices tune parameters only.
- All environment source terms (zodiacal, extinction, scattering) must
  convert to linear radiance **before** this stage (spec §9.4 rule).

## Open questions

- Exact ACES variant vs mobile cost (ties to ADR-007 tiers).
