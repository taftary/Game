# Simulation, physics, time

- Fixed-step sim tick (e.g. 20 Hz logic), decoupled from render framerate.
- Surface movement: kinematic character/rover controller; no full rigid-body engine in v1. Use a minimal collision (heightfield + capsule vs. building AABBs).
- Orbital layer: Kepler-ish visuals, not n-body.
- Time: game-time only; pause + 1x/3x speed on colony screens. No real-money timers.
- Determinism: sim state hashable for tests; replays / regression via recorded inputs (tooling).
