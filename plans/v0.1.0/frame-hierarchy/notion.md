# Notion — frame-hierarchy

## Status

`draft`

## Context

Milestone v0.1.0 (navigation core), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§3. The engine must represent player and camera positions across 26
decades (10²⁶ m → 10⁰ m, spec §1) without precision collapse. Decided by
ADR-012 (nested frames) and ADR-013 (floating origin) in
[`../../docs/decisions/`](../../../docs/decisions/). Builds on the v0.0.1
`scale-hierarchy` contract (8 journey levels).

## Problem & Needs

- A single world space (even rebased `f64`) cannot hold cosmic-web and
  facility-interior coordinates simultaneously.
- The GPU consumes `f32` only; world truth must be recentered on the
  camera every frame.
- Frame transitions must be explicit — no silent coordinate swaps that
  desync physics, rendering, and HUD.

## Goals

1. Frame tree per spec §3: Comoving Cosmological (Mpc) → Galactocentric
   (kpc) → Local-Group (Mpc) → Stellar-Neighborhood (pc/ly) →
   Solar-System Barycentric (AU) → Planetocentric (km, ECEF) → Local
   Tangent-Plane ENU (m).
2. Ship/camera state = composed transform chain, resolved only through
   the active frame and its immediate parent; never flattened globally.
3. CPU-side `f64` (or per-frame non-dimensionalized units); GPU upload is
   camera-relative `f32` only (floating origin).
4. Mapping from the 10 spec waypoints to the 8 journey levels is
   documented and consumable by `scale-debug-screens`.

## Non-goals

- No SOI blending logic (owned by `soi-handoff`).
- No time-compression logic (owned by `time-compression`).
- No depth-buffer strategy (owned by `log-depth-rendering`).

## Users / Stakeholders

- Players (invisible correctness: no jitter, no jumps).
- Developers: every navigation/physics/rendering feature consumes this
  frame API.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): pending
— required before `draft → planned` for any player-visible frame readout.
ARCHITECT consulted (required if cross-module): pending — cross-module by
construction (render + simulation + persistence).

## Functional requirements

- Frame ID enum + per-frame unit system (spec §5 examples: AU/day/solar
  masses with G ≈ 1; km/s/planet masses).
- Composed transform chain type: position/orientation in active frame +
  parent link; conversion API active ⇄ parent only.
- Floating-origin recenter step in the render path: camera-relative
  `f32` vertex upload per frame.
- Active-frame transition API with explicit commit (feeds ADR-004
  autosave trigger, ADR-015 time state).
- ENU local frame preserves `east × north == up` (rendering invariants,
  `docs/techstack/rendering.md`).

## Non-functional requirements

- No precision regression: position error < 1 mm at facility scale after
  a full chain resolve from the cosmological frame (validation target,
  spec §10 open item).
- Quality gates in `docs/techstack/quality.md` stay green.

## Definition of Done

- [ ] Frame tree + chain type implemented with unit tests at every
  frame boundary (round-trip precision evidence).
- [ ] Floating-origin upload active; no world-space `f32` position
  exceeds one frame's extent.
- [ ] Waypoint ⇄ journey-level mapping documented in
  `docs/game/journey.md` or linked doc.
- [ ] `scale-debug-screens` can display active frame + chain from this
  API.

## Constraints & Assumptions

- ADR-012/ADR-013 drafts become binding here.
- Vulkan projection contract unchanged (RH, Z ∈ [0, 1], no Y-flip).

## Open questions

- Exact frame-boundary radii per celestial body (needs the validation
  plan, spec §10 open item).
- Player craft state vector layout (blocked on craft definition, spec
  §10 open item).
