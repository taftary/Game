# Gameplay systems (v1)

## Navigation / travel

- Galaxy and system maps are UI-first; no manual interstellar flight in v1.
- Interplanetary transit is a timed action with resource cost (fuel/energy), cancellable before commit.
- Orbit is a holding state: survey, pick landing site, descend.

## Landing / descent / ascent

- Pick site from 2–5 precomputed candidates (flatness, resources, hazard).
- Descent is guided (v1), not a physics landing sim.
- Ascent returns ship + robots + cargo to orbit; colony persists.

Travel flow: [`journey.md`](journey.md).

## Colonies

- Grid-free placement with snap + validity test (slope, water, hazard, proximity).
- Buildings (v1 set, small): habitat, power, extractor, storage, pad.
- Hazard defense / turret-equivalent (PvE-lite): cut from v1 (mobile CPU + engine cost); revisit post-M7.
- Power + storage + upkeep tick; offline progress is capped and deterministic (no wall-clock exploitation).

## Robots

- Robots are the workers: haul, build, extract, repair, scout.
- Player issues orders (go/build/mine/ferry/patrol); no direct robot embodiment in v1.
- Cap active robots per colony for mobile CPU (e.g. 8–24, tuned). Behavior is simple state machines, no full pathfinding mesh in v1 — flow-field or waypoint graph per colony.
- All sim ticks fixed-step, deterministic given inputs (see [`../techstack/simulation.md`](../techstack/simulation.md)).

## Exploration

- POIs per planet (wrecks, vents, caves-as-markers, resource fields).
  Caves/vents are surface markers only in v1 — no underground geometry
  (see [`journey.md`](journey.md) Level 8 deep-dive).
- Scan → reveal loop; codex-lite entries stored in save.
- Hazards gate progression (suit upgrades are data, not new mechanics, in v1).

## Resources (v1, small)

Energy, metal, water/ice, organics, rare element per planet type. Recipes are shallow (2–3 inputs). Full tech tree is out; 2 tiers max.
