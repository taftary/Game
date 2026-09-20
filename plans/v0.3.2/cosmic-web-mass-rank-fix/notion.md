# Notion — cosmic-web-mass-rank-fix

## Status

`done` (closed 2026-09-20 by PO decision at the v0.3.3 cut — code
landed on `main` in `128ab27` / squash `e60a036`; evidence in
`plan.md`; the ANALYST/SECURITY rows are signed retroactively on the
recorded deviation, see `plans/README.md` § *7. Version branch*)

## Context

Stage D (`descriptor.rs:188-191`) assigns halo masses through a
Press-Schechter quantile table using `u = (rank + 0.5)/count` over the
densest-first accepted peak list.  This means the densest peak gets
`u ~ 0` (mass floor, 5e12 Msun) and the least-dense peak gets the
giant tail (~2.4e15 Msun) -- exactly inverted from the documented
intent in `classify.rs:7-8` ("the densest peak gets the rarest mass")
and both spec documents.

No test caught it because the statistical gates assert distribution
*shape* (median < M*, max >= 3e14, bottom-heavy), which passes under
either rank direction.  Nothing asserts the density-mass correlation.

Impact: the brightest, biggest, bloom-heaviest node impostors sit on
the weakest density peaks -- the visual hierarchy contradicts the
physics, and the home-node selection band (1e12-1e13 Msun) picks
different nodes than intended.

## Problem & Needs

A one-line rank-direction fix plus a new monotonic pin test that
asserts the density-mass correlation is correct.

## Goals

1. Invert the rank-to-quantile mapping so k-th densest peak gets
   k-th largest mass.
2. Add a test: masses must be non-increasing in acceptance order
   (densest-first = most-massive-first).
3. Re-roll the committed hash vector (`committed_web_vectors_pin_stage0`).
4. Re-verify home node selection (band membership changes).
5. Visual re-look: giant impostors should now sit on dense hubs where
   braid strands accumulate light.

## Non-goals

- Changing any generation parameter (thresholds, lattice, growth
  factor).
- Enrichment changes (braids, grain, impostors code untouched).
- LOD, performance, or rendering pipeline work.

## Users / Stakeholders

- Players: visual hierarchy of galaxy clusters now matches physics.
- Save compatibility: node indices stay the same; masses, virial
  radii, and home node index change.  UNIVERSE_VERSION bump signals this.

## Roles

Author: DEV.  UX consulted: yes (visual hierarchy change).
ARCHITECT consulted: yes (descriptor schema behavior change).

## Functional requirements

1. `descriptor.rs:189`: `u = 1.0 - (rank as f64 + 0.5) / count.max(1) as f64`
2. New test in `web/mod.rs`: `fn masses_decrease_with_density_rank`
   asserts `nodes[i].mass_msun >= nodes[i+1].mass_msun` for all
   consecutive pairs in acceptance order (densest-first).
3. Existing statistical gates continue to pass unchanged.
4. `UNIVERSE_VERSION` bumped (2 -> 3) since descriptor bytes change.

## Non-functional requirements

- No performance regression (one-line code change, no hot path).

## Definition of Done

1. Code fix landed in `descriptor.rs`.
2. Monotonic pin test added and passing.
3. Committed hash vector re-rolled to new value.
4. Home node verified (still in 1e12-1e13 band, nearest center).
5. Statistical gate tests pass unchanged.
6. `UNIVERSE_VERSION = 3` in `universe/mod.rs`.
7. Visual re-look in debug build confirmed (giant impostors on dense
   hubs).
8. Spec #1 updated (Stage C claim "denser peaks get rarer ranks"
   becomes factually correct post-fix).

## Constraints & Assumptions

- The fix must land before the cinematic refresh `feat:` commit on
  v0.3.2, so visual sign-off (DoD 8, update-2026-09-18-2328) covers
  the corrected landscape once.

## Open questions

- Home node band: re-verify after fix that a node in 1e12-1e13 Msun
  is still nearest center (expected: yes, but different node than before).
