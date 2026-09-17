# Notion — star-catalog-streaming

## Status

`done` (all DoD criteria checked in `plan.md`, ANALYST + SECURITY signed 2026-09-17)

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

UX consult (2026-09-17): the sky is never blank — the fallback lattice
renders within 200 ms of a tile miss (that deadline is the player-side
failure surface). Catalog arrival replaces fallback through a ~300 ms
crossfade (reuses the `FxState` fade pattern) so the swap has no
visible pop. Tile state (loaded vs fallback) shows only in the
`game_debug` overlay — dev-only, never leaks into the release binary.
Beyond-±1000 yr proper-motion validity warns in the overlay + `tracing`
now; the player-facing HUD flag is handed to `navigation-hud`
(v0.3.0). No new controls; `controls.md` untouched.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): yes
(notes in Users / Stakeholders, 2026-09-17) — fallback visual seams are
player-visible; ~300 ms crossfade adopted. ARCHITECT consulted (required
if cross-module): yes (breakdown in plan.md, 2026-09-17 — new pure
module `engine::catalog`, cooker in `crates/tools`, ADR-017/ADR-020 go
binding on land, determinism/rendering/save invariants listed).

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
- PO decisions 2026-09-17 (spec §10 addendum "Resolved for
  star-catalog-streaming"): `std::thread` worker pool (no async runtime,
  no new dependency — `stack.md` untouched), hand-rolled HEALPix in
  `engine`, small-extract + synthetic data, space-sky first surface.
- The fallback is a pure function of (master seed, tile metadata);
  loader threads never feed the sim tick (determinism invariant holds).
- Rendering stays in the Backdrop band + log-depth; no
  projection/camera/picking invariant changes.
- The `BodyId` placeholder swap in `frames.rs` is deferred — its own
  change when navigation consumes canonical ids.

## Open questions

- On-disk cache format + size: RESOLVED provisionally (PO 2026-09-17) —
  same binary tile v1 on disk, cooker output configurable (`--out`,
  default `assets/catalog/gaia-dr3/` for the packaged demo extract);
  cache-size cap provisional pending the network/content product item
  (spec §10, still open).
