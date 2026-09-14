# Scale and camera journey (hardest technical problem)

## Scales

| Layer | Approx. radius / extent | Representation |
|---|---|---|
| Galaxy | 10k–100k light-years (compressed) | Star points + nebula impostors, log/compressed space |
| Star system | AU-scale (compressed) | Orbital map, patched positions |
| Planet globe | 2–64 km gameplay radius (not real-Earth scale) | Spherical LOD mesh, far view |
| Orbit | 5–50 km altitude | Ship/orbital camera, planet fills view |
| Atmosphere | 0–10 km | Scattering shell + sky transition |
| Sky → soil | 0–2 km → ground | Terrain chunks, colonies, robots |

Real astronomical units are **not** simulated. Use compressed, game-feel distances with consistent meters internally per layer and a **floating origin** on the surface layer.

## Rules

- One active high-detail layer at a time (surface XOR orbit-far). Stream the next during descent/ascent.
- Double-precision (f64) for orbital/galaxy coordinates; f32 + camera-relative origin for rendering.
- No hard loading screen on planet descent in the ideal path; fades + LOD pop-in budget allowed in v1.
- Zoom-out path (soil → sky → atmosphere → orbit → globe → system → galaxy) reuses the same state machine in reverse.
- Planet gameplay radius starts small (2–8 km) and grows only when perf allows. Universe breadth comes from count and variety, not per-planet km².

## State machine (v1)

`GalaxyMap → SystemMap → Orbit → Descent → Surface ⇄ Ascent → Orbit → ...`

Each transition has explicit enter/exit, asset prefetch hints, and a fallback (fade + spinner) if streaming misses budget.

Rendering side: [`../techstack/rendering.md`](../techstack/rendering.md). Budgets: [`../techstack/quality.md`](../techstack/quality.md).
