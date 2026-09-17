# Notion — dimension-debug

## Status

`in-progress`

## Context

The debug binary already renders the implemented galaxy, system, and planet
layers, but the scale and transition data is split between separate tools
panels. Developers need one read-only 3D section that makes the implemented
journey layers and their connections visible together.

## Goals

- Add a Dimensions section with tabs for L1 Universe, L2 Galactic, L3 System,
  L4 Planetary, L5 Orbit, and Connections.
- Reuse existing generated buffers, cameras, descriptors, and journey state.
- Show the ten waypoint nodes and nine adjacent waypoint legs in Connections.
- Keep the section visualization-only: it must not mutate simulation state.

## Non-goals

- No L6 Descent, L7 Surface, or L8 Subterranean geometry before those layers
  have v1 state.
- No new universe generation, journey events, or gameplay behavior.

## Definition of Done

- [ ] Six tabs are reachable in the debug tools window.
- [ ] L1-L5 render existing data through the existing 3D pipelines.
- [ ] Connections renders ten waypoint nodes and nine adjacent legs.
- [ ] Dimension selection and rendering do not mutate Journey/System/Viewer state.
- [ ] Tests, formatting, and clippy gates pass.
