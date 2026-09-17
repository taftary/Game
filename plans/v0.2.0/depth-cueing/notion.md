# Notion — depth-cueing

## Status

`done` (all DoD criteria checked in `plan.md`, ANALYST + SECURITY signed 2026-09-17)

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§9.1. Decided by ADR-021 in [`../../../docs/decisions/`](../../../docs/decisions/).
Rayleigh/Mie haze is physically valid only inside an atmosphere
(waypoints 7–10 and descent into 7); every vacuum regime needs a
physically valid substitute.

## Problem & Needs

- Using atmospheric haze in vacuum is physically wrong and visually
  samey; each regime needs its own real cue.
- Cosmological redshift is **not** a monotonic cue at local
  intergalactic range (Andromeda is blueshifted).

## Goals

Implement the spec §9.1 regime table:

1. Interplanetary (waypoint 6): faint real zodiacal dust glow (source
   term from `zodiacal-light`).
2. Interstellar (5–4): dust extinction + reddening — color excess
   `E(B−V)`, extinction `Aᵥ` from actual dust column density.
3. Local intergalactic (4–3): extinction/reddening + peculiar-velocity
   Doppler tinting (not cosmological redshift).
4. Cosmic web (2–1): cosmological redshift once Hubble flow dominates +
   raymarched seeded volumetric density fields (filament luminosity
   falloff, dust-node glow).
5. Atmosphere (7–10): real Rayleigh blue sky/horizon haze + Mie
   haze/pollution scattering.

## Non-goals

- Exposure/tone mapping (owned by `exposure-tone-mapping`).
- Waypoint-to-waypoint transition scripting (owned by
  `waypoint-transitions`) — this feature supplies the per-regime
  primitives.

## Users / Stakeholders

- Players: legible depth at every scale with physical honesty.
- Developers: one documented cue model per regime.

UX notes (2026-09-17): depth cueing is a passive visual layer and adds no
controls or affordances. Perceptual exaggeration is limited to tunable
calibration factors with readable defaults; style must not invert the physical
direction of a cue. Reference captures verify readability for every regime.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): yes —
passive visual layer and perceptual calibration notes under `Users /
Stakeholders` (2026-09-17). ARCHITECT consulted (required if cross-module):
yes — cue models belong in `engine::render`, dust column density uses
domain-separated `engine::seeding` sub-seeds per ADR-019, volumetric raymarch
stays within tier budgets, and rendering invariants are untouched (2026-09-17).
ADR-021 covers the regime table; binding ADR-003 records the atmosphere model
and fallback; no new ADR is needed for this feature.

PO sign-off for `draft → planned` (2026-09-17): checklist green — problem
stated without prescribing implementation; non-goals explicit; DoD criteria
verifiable. DoD-2 uses an approved active-frame switch harness asserting cue
model replacement with no leftover state, since waypoint runtime belongs to
`waypoint-transitions`. ADR-021 fixes the physical basis; visual style may
only tune parameters, so the spec §10 open item remains open. Volumetric
density resolution is resolved by the tier budget in `plan.md` and
`docs/techstack/quality.md`.

## Functional requirements

- Per-regime cue modules keyed to the active frame/waypoint.
- Dust column density source (seeded, per ADR-019) driving
  `E(B−V)`/`Aᵥ`.
- Peculiar-velocity Doppler tint + Hubble-flow redshift switches per
  regime.
- Raymarched volumetric density fields for cosmic-web scales.

## Non-functional requirements

- Volumetric raymarch within tier perf budgets
  (`docs/techstack/quality.md`).
- Quality gates stay green.

## Definition of Done

- [ ] Each regime table row has an implemented cue + reference capture.
- [ ] Regime switch test: crossing waypoint boundary swaps cue models
  with no leftover state.
- [ ] ADR-003 (atmosphere/sky model) updated/closed by the atmosphere
  rows here.

## Constraints & Assumptions

- ADR-021 draft becomes binding here.
- Perceptual exaggeration for readability is tunable but the physical
  basis per regime is fixed.

## Open questions

- Volumetric density resolution per tier (perf data needed).
