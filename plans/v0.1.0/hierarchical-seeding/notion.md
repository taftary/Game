# Notion — hierarchical-seeding

## Status

`done`

(2026-09-17: ANALYST DoD-verified + SECURITY reviewed; single commit
on branch `v0.1.0` per `plans/README.md` § *7. Version branch*.)

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§6. Decided by ADR-019 in [`../../docs/decisions/`](../../../docs/decisions/).
The universe must regenerate identically from a single master seed with
no stored procedural content, and one region must be regenerable without
touching others.

## Problem & Needs

- Reusing one raw seed across content layers produces correlated
  artifacts (spec §6).
- Real cataloged objects must win over procedural generation wherever
  both exist.

## Goals

1. Hierarchical derivation:
   `region_seed = hash(master_seed, frame_id, cell_coordinates)`.
2. Domain separation: sub-seeds per content layer
   (`hash(region_seed, "galaxy_arms")`, `"star_field"`,
   `"crater_field"`, …) — raw region seeds never used directly.
3. Real-data override: exclusion radius around cataloged objects,
   checked before the procedural hash fires for the cell.
4. Determinism: content = pure function of `(seed, position)`; flying
   away and back (or a new session, same seed) reproduces identical
   regions.

## Non-goals

- The specific per-layer generators (galaxy arms, star fields, craters)
  — each lands with its own feature; this is the seeding contract.
- Catalog streaming (owned by `star-catalog-streaming`; supplies the
  override lookup).

## Users / Stakeholders

- Players: a consistent shared universe per seed.
- Developers: independent region regeneration for debugging.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): n-a
confirmed (2026-09-17) — no UI in this feature. ARCHITECT consulted
(required if cross-module): yes (2026-09-17) — breakdown in `plan.md`
(touches generation + catalog + persistence metadata shapes).

## Functional requirements

- Master seed → region seed → layer sub-seed derivation APIs with
  stable hash (documented algorithm + version).
- Exclusion-radius check API consulted by generators before cell hash.
- Master seed + catalog version IDs recorded in save metadata
  (ADR-004).

## Non-functional requirements

- Determinism test: same `(seed, position)` ⇒ bit-identical generated
  content across runs/sessions.
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Derivation API + domain-separated sub-seeds with tests showing
  decorrelation between layers.
- [ ] Real-data override test: catalog object suppresses procedural
  cell content inside its exclusion radius.
- [ ] Determinism soak evidence (regenerate-after-departure identical).

## Constraints & Assumptions

- ADR-019 draft becomes binding here.
- Golden-angle lattice / irrational-rotation hash from the spec §7 math
  toolkit are the preferred distribution primitives.

## Open questions

- Hash algorithm choice (needs collision + speed data; candidate
  xxHash/CityHash family — decide at plan time).
