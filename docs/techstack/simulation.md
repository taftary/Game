# Simulation, physics, time

- Fixed-step sim tick (e.g. 20 Hz logic), decoupled from render framerate.
- Surface movement: kinematic character/rover controller; no full rigid-body engine in v1. Use a minimal collision (heightfield + capsule vs. building AABBs).
- Orbital layer: Kepler-ish visuals, not n-body.
- Time: game-time only; pause + 1x/3x speed on colony screens. No real-money timers.
- Determinism: sim state hashable for tests; replays / regression via recorded inputs (tooling).

## Per-frame unit systems (v0.1.0 `scale-physics`)

Each active frame uses a well-conditioned local system (spec §5,
`engine::physics::FrameUnits`); `G` is derived exactly from SI
constants, never assumed. Solar-system `G` is the Gaussian `k²`, not 1
(spec-clarification in `plans/v0.1.0/scale-physics/plan.md`).

| Frame | Length | Time | Mass | Local G |
|---|---|---|---|---|
| Cosmological / LocalGroup | Mpc | Gyr | 10¹² M☉ | derived |
| Galactocentric | kpc | Myr | 10¹⁰ M☉ | derived |
| StellarNeighborhood | pc | kyr | M☉ | derived |
| SolarSystem | AU | day | M☉ | 2.95912208e-4 AU³/(M☉·day²) |
| Planetocentric | km | s | M⊕ | 3.986004418e5 km³/(M⊕·s²) |
| LocalEnu | m | s | kg | 6.67430e-11 (SI) |

Tick wiring (fixed-step + compression) lands with `time-compression`;
ship control with `free-flight-navigation`.
