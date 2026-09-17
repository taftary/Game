# Notion — Player Sphere Movement

## Status

`done`

## Context

The game binary (`crates/game`) was an empty `fn main() {}`. The engine
already provided the pieces this feature composes: `HexSphere` with
`ChunkId = cell index` (ADR-010), `OrbitCamera`, and the Lambert
`visible_hemisphere` / `project_to_tangent` flat-map rule
(`plans/v0.0.1/chunk-flat-view`). No player, camera-switching, or streaming
logic existed.

## Problem & Needs

A player that walks continuously on the sphere surface (longitude /
latitude), a camera that switches between the global view and
player-relative views, and chunk loading that follows the player on the
flat map: load ahead in the direction of movement, unload the trail
behind after a delay.

## Goals

- Simple player position system (lon/lat on a perfect sphere).
- Switchable cameras: global orbit + follow + first-person + third-person.
- Chunk streaming with distance-based loads and time-delayed unloads,
  reusing the flat map's hemisphere rule.
- Implemented in the game binary; engine stays generic.
- Headless demo loop so `cargo run --bin game` keeps passing.

## Non-goals

- No terrain heightfield or collision (M3).
- No player animation or models (position only).
- No interactive `winit` input binding yet (scripted demo + `MoveInput`
  API ready for it).
- No save/load, UI/HUD, multiplayer.

## Users / Stakeholders

- Primary: players exploring the planet on foot (future M3).
- Secondary: developers verifying chunk streaming behavior headlessly.

## Functional requirements

1. Player has lon/lat on the sphere; tangent-plane input (north/east).
2. Four camera modes, instantly switchable, all tracking the player
   except the free global orbit.
3. Desired chunk set = `visible_hemisphere` centered on the player
   (same rule as today); eviction delayed by a grace period.
4. Flat map recenters on the player (player projects to origin).

## Non-functional requirements

- Deterministic: same input script → same load/unload sequence
  (tick-based grace, no wall clock).
- Headless demo terminates (quality-gate friendly).
- Stable frame cost: streaming work is a ribbon over the desired set.

## Definition of Done

- [x] Player moves on the sphere with tangent-plane input.
- [x] Four camera modes implemented and switchable in one cycle.
- [x] Chunks load within the player hemisphere immediately.
- [x] Chunks unload after the grace delay when left behind.
- [x] Flat map recenters on the player (asserted every demo tick).
- [x] 20 unit tests pass; `cargo run --bin game` terminates with a log.
- [x] Docs updated (`docs/game/controls.md`, techstack version bump).

## Constraints & Assumptions

- Perfect sphere (no heightfield); pole margin clamp.
- 20 Hz fixed sim step for the demo (`DT = 0.05`).
- Demo planet N=2 (162 cells); rules are subdivision-independent.

## Open questions

- Interactive `winit` binding (which key cycles cameras?) — deferred.
- Tuning of walk speed, follow distances, grace period — playtest later.
