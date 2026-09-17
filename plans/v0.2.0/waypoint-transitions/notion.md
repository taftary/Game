# Notion — waypoint-transitions

## Status

`done`

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§9.3. The journey between the 10 waypoint dimensions must feel
continuous and physically motivated. Composes `depth-cueing`,
`exposure-tone-mapping`, `zodiacal-light`, and the
`free-flight-navigation` fly-to machinery. Debug visibility via
`scale-debug-screens` (global transitions panel).

## Problem & Needs

- Each waypoint-to-waypoint leg has distinct physical behavior (spec
  §9.3 lists all nine); a generic zoom looks wrong.
- The 6 → 5 leg (Sun-as-disk → Sun-as-point) is the exposure system's
  hardest transition.

## Goals

Implement the spec §9.3 leg behaviors:

1. 10 → 9 interior → exterior: exposure-adaptation cut (artificial →
   ambient/daylight).
2. 9 → 8 exterior → aerial: aerial-photography haze; contrast/saturation
   falloff with distance + altitude.
3. 8 → 7 aerial → full Earth: blue → indigo → black sky, thin blue limb
   glow, curvature near the Kármán range (100 km FAI or ~80 km
   reanalysis — neither represented as uncontested).
4. 7 → 6 Earth → solar system: Earth recedes to a point; Sun disk
   shrinks inverse-square; planets brighten as points; zodiacal glow
   along the ecliptic.
5. 6 → 5 solar system → neighborhood: Sun becomes one point among
   stars (exposure hero-source handoff).
6. 5 → 4 neighborhood → Milky Way: stars resolve into spiral structure;
   dust-lane extinction reddens/dims near the plane.
7. 4 → 3 Milky Way → Local Group: Milky Way one galaxy among members;
   extinction + peculiar-velocity Doppler depth.
8. 3 → 2 Local Group → supercluster: clusters become blobs; same cue
   family.
9. 2 → 1 supercluster → cosmic web: Hubble-flow redshift becomes
   monotonic + raymarched volumetric filaments.

The feature also owns the runtime coordination required to make those legs
observable: a `WaypointId` layer over `FrameChain`, deterministic per-leg
transition descriptors, an analytic GPU atmosphere shell for the atmospheric
legs, and a debug transitions panel with an event queue shaped for the future
ADR-004 autosave consumer.

## Non-goals

- The underlying cue/exposure/zodiacal primitives (owned by their
  features).
- The fly-to trajectory math (owned by `free-flight-navigation`).
- Facility interior rendering. Leg 10 → 9 is demonstrated at descriptor and
  exposure-key level until a facility feature supplies interior geometry.
- Autosave persistence and file I/O. This feature emits drainable events;
  `autosave-persistence` owns the eventual consumer.
- The final global visual policy (PBR versus stylized); parameters remain
  tunable while that product decision stays open.

## Users / Stakeholders

- Players: the signature "powers of ten" journey, seamless end to end.
- UX: pacing/feel of every leg is player-facing.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): yes —
transition pacing, manual/select-to-focus reachability, and visible debug
feedback (2026-09-17). ARCHITECT consulted (required if cross-module): yes —
the waypoint layer coordinates render, flight, frames, and debug events
(2026-09-17).

## Functional requirements

- Per-leg transition descriptors driven by active-frame/waypoint
  changes (manual flight and select-to-focus both).
- Ten `WaypointId` values map onto the seven existing `FrameId` values and
  altitude bands without changing frame-coordinate conventions.
- Kármán-line representation with the dual 100 km / ~80 km framing
  (spec §9.3).
- An analytic GPU atmosphere shell applies Rayleigh/Mie scattering, the
  altitude sky ramp, and limb glow with a low-tier approximation.
- Transition events are visible in the feature-owned `game_debug` transitions
  panel and emitted through a queue compatible with autosave triggers
  (ADR-004).

## Non-functional requirements

- No exposure pops, no cue-model leftovers across legs.
- Quality gates stay green.

## Definition of Done

- [ ] All nine legs demonstrable with per-leg capture evidence against
  the spec §9.3 descriptions.
- [ ] 6 → 5 exposure handoff smooth (joint evidence with
  `exposure-tone-mapping`).
- [ ] Leg 10 → 9 demonstrates the interior-to-ambient exposure cut through
  descriptor and headless key evidence; facility geometry is not required.
- [ ] Atmospheric legs provide GPU-shell evidence for haze, sky deepening,
  limb glow, and the dual Kármán-line framing on the supported tier paths.
- [ ] Each leg emits events visible in the feature-owned `game_debug`
  transitions panel and drainable by a future autosave consumer.

## Constraints & Assumptions

- Lands after its primitive features; scheduling per
  `docs/milestones/README.md` v0.2.0.
- The facility, terrain, and network/content product items remain outside this
  feature; no new asset or network policy is introduced here.

## Open questions

- Which legs are scripted sequences vs emergent from primitives (plan
  decision per leg).
