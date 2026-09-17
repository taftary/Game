# Notion — scale-physics

## Status

`draft`

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§5. Decided by ADR-018 (models + integrator) and ADR-020 (data policy) in
[`../../docs/decisions/`](../../../docs/decisions/). Uniform-fidelity N-body
across all scales is impractical and unnecessary — each scale gets the
cheapest model that is visually and physically indistinguishable from
correct.

## Problem & Needs

- 26 decades cannot share one integrator or one unit system; raw SI is
  poorly conditioned for `f32`/`f64` force math (10²⁶ m, 10⁴² kg).
- Long sessions must not watch orbits decay (naive Euler drifts).

## Goals

1. Implement the spec §5 model table: static density field (cosmic
   web/supercluster); Miyamoto-Nagai disk + NFW halo potential (Milky
   Way / Local Group; a = 6.5 kpc, b = 0.26 kpc, r_s = 20 kpc); Gaia
   proper motions (solar neighborhood); precomputed ephemeris
   (solar-system background); closed-form Keplerian ship dynamics with
   patched-conic blending; seeded exoplanet-statistics orbits for
   procedural bodies.
2. Symplectic integrator (velocity Verlet / leapfrog) for every
   numerically integrated regime.
3. Per-frame non-dimensionalized units (AU/day/solar masses with G ≈ 1;
   km/s/planet masses).

## Non-goals

- Frame switching itself (owned by `frame-hierarchy` / `soi-handoff`).
- Ephemeris/catalog data loading pipelines (owned by
  `star-catalog-streaming`; this feature consumes their interfaces).
- Live gravity at cosmic-web scale (spec: static, no real-time motion).

## Users / Stakeholders

- Players: believable orbital behavior at every scale.
- Developers: one documented model per scale, no fidelity surprises.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): n-a at
draft (no direct UI). ARCHITECT consulted (required if cross-module):
pending (simulation core).

## Functional requirements

- Galactic potential evaluator: Miyamoto-Nagai disk + NFW halo with the
  spec §5 formulas and typical parameters.
- Proper-motion extrapolation with the ADR-020 ±1000-year validity
  window + warning beyond.
- Ephemeris interface: VSOP87 default, DE440-derived simplified elements
  as optional high-precision mode (Jupiter–Neptune).
- Keplerian element solver for the ship's dominant two-body case.
- Unit tables per frame with conversion tests.

## Non-functional requirements

- Energy stability: no visible orbit decay over a long-session soak
  (tolerance per validation plan, spec §10 open item).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Each spec §5 table row has an implemented model + regression test
  (rotation curve flatness, proper-motion displacement, ephemeris
  reference positions, Kepler period).
- [ ] Symplectic integrator demonstrated stable vs Euler baseline.
- [ ] `simulation.md` documents the per-frame unit table.

## Constraints & Assumptions

- ADR-018/ADR-020 drafts become binding here.
- Procedural-body orbital elements draw from seed + real exoplanet
  population statistics (links `hierarchical-seeding`).

## Open questions

- High-precision ephemeris mode: runtime toggle or build-time?
- Validation tolerances per scale (spec §10 open item).
