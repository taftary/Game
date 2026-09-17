# Notion — star-catalog-streaming

## Status

`draft`

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§4 (LOD & streaming) + §8 (data sources). Decided by ADR-017 (HEALPix) and
ADR-020 (data policy) in [`../../docs/decisions/`](../../../docs/decisions/).
Gaia DR3's ~1.8 billion stars cannot be memory-resident.

## Problem & Needs

- All-sky catalog access must stream on demand keyed by camera position
  and active frame.
- Slow/absent tiles must not blank the sky or break selection and
  navigation state.

## Goals

1. HEALPix spatial index, order 12 default (nside 4096, 2.0×10⁸ px,
   51.5″), order 13 dense galactic plane, order 14 premium close-ups —
   per density tier + device memory class.
2. Budgets: 2 GB active star-tile memory (decoded + GPU-ready);
   < 100 ms async tile-load once data is local; priority = view frustum
   → predicted travel direction → camera distance.
3. Deterministic fallback after 200 ms: preserve approximate source
   count + mean color from tile metadata, seeded golden-angle lattice
   distribution, seamless replacement on arrival.
4. Stable shared object/tile identifiers across catalog and fallback —
   replacement never affects selection, focus targeting, or navigation
   state.

## Non-goals

- Offline-vs-network packaging policy (open product item, spec §10).
- Ephemeris data (planets) — interface consumed by `scale-physics`,
  pipeline here only if it shares the tile machinery.
- Proper-motion math itself (owned by `scale-physics`).

## Users / Stakeholders

- Players: a complete, responsive sky at every scale.
- Developers: stable IDs for selection/navigation debugging
  (`scale-debug-screens`).

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): pending
— fallback visual seams are player-visible; consult before
`draft → planned`. ARCHITECT consulted (required if cross-module): pending
(assets + render + navigation identity).

## Functional requirements

- HEALPix tile addressing + decode to GPU-ready buffers.
- Streaming scheduler with the stated priority order and latency budget.
- Fallback generator (golden-angle lattice from tile metadata) +
  seamless swap.
- Identity layer: one ID space covering catalog and fallback objects.

## Non-functional requirements

- Memory ceiling 2 GB enforced + instrumented.
- Load latency p50/p95 metrics logged (`tracing`, ADR-009).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Order-12 tile set streams within budget on the reference tier
  (evidence: memory + latency metrics).
- [ ] Forced-tile-miss test shows fallback inside 200 ms and seamless
  replacement; IDs stable across the swap.
- [ ] Gaia DR3 epoch/proper-motion fields plumbed to `scale-physics`.

## Constraints & Assumptions

- ADR-017/ADR-020 drafts become binding here.
- Real-data override (ADR-019) consumes this feature's catalog lookup.

## Open questions

- On-disk cache format + size (ties to the network/content open item).
