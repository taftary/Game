# Notion — zodiacal-light

## Status

`draft`

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§9.4. Decided by ADR-021 in [`../../docs/decisions/`](../../../docs/decisions/).
The solar-system frame needs its real faint dust glow — reflected/
scattered sunlight from interplanetary dust, **not** atmospheric haze.

## Problem & Needs

- Interplanetary space has no atmospheric depth cue; the zodiacal glow
  is the real one (spec §9.1).
- The model must be replaceable by a map-based asset later without
  interface churn.

## Goals

1. Analytic zodiacal-light model in the solar-system frame, evaluated
   per pixel from camera ray direction in ecliptic coordinates.
2. Base V-band surface brightness μ₀ = 23.0 mag/arcsec² at the ecliptic
   poles; magnitude-offset form with latitude falloff (Aβ = 1.5 mag,
   β₀ = 20°) and forward-scattering term (Aθ = 0.5 mag, k = 0.7) per
   the spec §9.4 formula.
3. Clamp all terms to measured-plausible ranges; expose calibration
   parameters for photographic comparison.
4. Convert magnitude surface brightness to linear radiance **before**
   exposure/tone mapping (`exposure-tone-mapping`).
5. Interface stable for a future map-based HEALPix replacement.

## Non-goals

- The exposure stage itself (owned by `exposure-tone-mapping`).
- The interplanetary depth-cue composition (owned by `depth-cueing`).

## Users / Stakeholders

- Players: physically real interplanetary sky.
- Developers/validators: calibration knobs + reference validation.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): n-a at
draft (subtle visual, no interaction). ARCHITECT consulted (required if
cross-module): pending (render interface contract).

## Functional requirements

- Ecliptic-frame ray transform + per-pixel evaluation of the spec §9.4
  magnitude-offset formula.
- Calibration parameter set exposed (μ₀, Aβ, β₀, Aθ, k) with clamps.
- Linear-radiance output plumbed into the exposure path.

## Non-functional requirements

- Full-sky evaluation cost within tier budgets (cheap analytic form is
  the point).
- Quality gates stay green.

## Definition of Done

- [ ] Model implemented with spec initial parameters; ecliptic-pole and
  low-elongation captures match expected falloff.
- [ ] Validation against reference observations (COBE/DIRBE or HST
  background models) documented before release (spec §9.4).
- [ ] Swap test: interface accepts a stub map-based asset unchanged.

## Constraints & Assumptions

- ADR-021 draft becomes binding here.
- Magnitude → linear conversion must precede filmic tone mapping
  (spec §9.4).

## Open questions

- Final calibration values after photographic comparison.
