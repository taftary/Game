# Scale and camera journey (hardest technical problem)

## Scales

Eight canonical levels, ordered widest → deepest (zoom order: 1 is the
widest view, 8 the deepest). Each level's parent is the row above it,
its child the row below. Real-world ranges are reference only.

| Lv | Name | Real-world reference | Game extent (compressed) | v1 representation | Key entities | Domain |
|---|---|---|---|---|---|---|
| 1 | Universe Level | $10^{26}\text{ m}$ | Container only: one generated galaxy per save | Generated cosmic web (stage 0, v0.3.2): player-traversable in the Game Demo; cosmic-web backdrop in the galaxy map | Cosmic web, superclusters | Deep Space |
| 2 | Galactic Scale | $10^{21}\text{ m}$ | 10k–100k light-years (compressed) | Star points + nebula impostors, log/compressed space | Spiral arms, galactic core, interstellar medium | Deep Space |
| 3 | Stellar System Level | $10^{13}\text{ m}$ | AU-scale (compressed) | Orbital map, patched positions | Central star, planetary orbits, Kuiper belt, heliosphere | Deep Space |
| 4 | Planetary System Domain | $10^{7}\text{ m}$ – $10^{8}\text{ m}$ | 2–64 km gameplay radius (not real-Earth scale) + visual companions | Spherical LOD mesh, far view; moons/rings as backdrop | Main planet, moons, ring systems, probes, rockets | Orbital & Atmospheric |
| 5 | Orbital Expanse | $10^{5}\text{ m}$ – $10^{6}\text{ m}$ | 5–50 km altitude | Ship/orbital camera, planet fills view | Low/high orbits, magnetosphere (visual), reentry corridors, stations (future) | Orbital & Atmospheric |
| 6 | Atmospheric & Sky Boundary | $10^{2}\text{ m}$ – $10^{4}\text{ m}$ | 0–10 km | Scattering shell + sky transition | Thermosphere, troposphere, weather (visual), flight altitudes | Orbital & Atmospheric |
| 7 | Terrain & Human Dimension | $10^{0}\text{ m}$ – $10^{3}\text{ m}$ | 0–2 km → ground | Terrain chunks, colonies, robots | Topography, ecosystems, human structures, bodily scale | Surface & Biological |
| 8 | Subterranean Domain | $10^{-1}\text{ m}$ – $10^{6}\text{ m}$ (below surface) | v1: surface markers only | Caves/vents as POI markers on the surface map; no geometry | Aquifers, caverns, tectonic plates, mantle, core | Planetary Interior |

Real astronomical units are **not** simulated. Use compressed, game-feel distances with consistent meters internally per layer and a **floating origin** on the surface layer.

## Waypoint ⇄ level ⇄ frame mapping

Spec waypoints ([`cosmic-navigation-engine-v0.4.md`](../techstack/cosmic-navigation-engine-v0.4.md)
§1, 10 named, 10²⁶ m → 10⁰ m) mapped onto the 8 journey levels above
and the navigation frame chain (`plans/v0.1.0/frame-hierarchy`).

| WP | Waypoint (spec §1) | Journey level | Frame |
|---|---|---|---|
| W1 | Cosmic Web, 10²⁶ m | L1 Universe (backdrop, not traversable) | Cosmological |
| W2 | Supercluster, 10²⁴ m | L1 Universe (backdrop) | Cosmological |
| W3 | Local Group, 10²³ m | L2 Galactic | LocalGroup |
| W4 | Milky Way, 10²¹ m | L2 Galactic | Galactocentric |
| W5 | Solar Neighborhood, 10¹⁷ m | L3 Stellar System | StellarNeighborhood |
| W6 | Solar System, 10¹⁴ m | L3 Stellar System | SolarSystem |
| W7 | Planetary Surface / Earth, 10⁶ m | L4 Planetary System / L5 Orbit | Planetocentric |
| W8 | Regional Aerial, 10³ m | L6 Descent / L7 Surface | LocalEnu |
| W9 | Facility Exterior, 10¹ m | L7 Surface | LocalEnu |
| W10 | Facility Interior, 10⁰ m | L7 Surface | LocalEnu |

Precision note (frame-hierarchy, 2026-09-17): millimetre truth holds
inside the solar subtree (W6–W10) by float conversion; W1–W5 descents
re-anchor at SOI handoffs, never by absolute float conversion.

## Drill-down (zoom order)

```text
Deep Space
  L1 Universe Level ............ cosmic web (stage 0, v0.3.2:
                                 player-traversable in the Game Demo)
  L2 Galactic Scale ............ GalaxyMap
  L3 Stellar System Level ...... SystemMap
Orbital & Atmospheric
  L4 Planetary System Domain ... SystemMap (planet focus)
  L5 Orbital Expanse ........... Orbit
  L6 Atmospheric & Sky Boundary  Descent ⇄ Ascent
Surface & Biological
  L7 Terrain & Human Dimension . Surface
Planetary Interior
  L8 Subterranean Domain ....... surface markers only (no v1 state)
```

## Level 8 — Subterranean deep-dive

v1 ownership: **markers only**. Caves and vents appear as POI markers on
the surface map ([`gameplay.md`](gameplay.md)); no underground geometry,
no underground player state.

1. **Shallow subsurface (0–100 m):** topsoil, roots, basements. First
   candidate for post-v1 geometry (colony foundations, buried resources).
2. **Mid-depth lithosphere (100 m–10 km):** deep mines, caverns, water
   tables. Post-v1: mineable volumes driven by the planet seed.
3. **Deep interior (10 km–6,000+ km):** crust, mantle, liquid outer core,
   solid inner core. Backdrop/lore only; never traversable.

## Rules

- One active high-detail layer at a time (surface XOR orbit-far). Stream the next during descent/ascent.
- Double-precision (f64) for orbital/galaxy coordinates; f32 + camera-relative origin for rendering.
- No hard loading screen on planet descent in the ideal path; fades + LOD pop-in budget allowed in v1.
- Zoom-out path (L7 → L6 → L5 → L4 → L3 → L2) reuses the same state machine in reverse.
- Planet gameplay radius starts small (2–8 km) and grows only when perf allows. Universe breadth comes from count and variety, not per-planet km².

## State machine (v1)

`GalaxyMap` (L2; L1 is backdrop) → `SystemMap` (L3; L4 planet focus) → `Orbit` (L5) → `Descent` (L6) → `Surface` (L7) ⇄ `Ascent` (L6) → `Orbit` (L5) → ...

The debug Dimensions dropdown lists the ten waypoint tabs with the
active journey layer marked. Milky Way / Solar System / Earth mount the
absorbed Galaxy Map / System Map / Planet View; the Cosmic Web tab
mounts the absorbed cosmic-web inspector (v0.3.2); the other six tabs
are placeholders (S7: last state + `INACTIVE` badge, never blank)
until their runtime state exists.

Each transition has explicit enter/exit, asset prefetch hints, and a fallback (fade + spinner) if streaming misses budget.

Implementation (M5 universe maps, `plans/v0.0.1/universe-maps`): the top
segment is built — `game::journey` runs `GalaxyMap`/`SystemMap`/`Orbit`
with selection/event transitions, fade + prefetch/evict effects, and a
pinned-hash fixed-step regression; `game::transit` adds the timed,
pre-commit-cancellable interplanetary hop (60 ticks @ 20 Hz, deferred
fuel/energy hook); `engine::universe` generates stage 0 (cosmic web,
v0.3.2) then stages 1–2 with quantized cross-platform hashes;
`game_debug` mounts the generated web twice from one descriptor — the
Game Demo tab (`F1`, default) renders the player flying inside it
(`ShipState` in `FrameId::Cosmological`, marker + Chase/Orbit/
FirstPerson camera, scale-relative cruise + click/`E` fly-to, shipping
HUD with live speed). Since v0.3.3 (`cosmic-vista-intro`) the demo
boots on a vista intro: the reference composition (nearest Tier-A hub,
25° FOV, 40 Mpc slab) holds 2 s, then dives 8 s to the spawn Chase
pose — skippable with any input, replayable with `V`.
while the Cosmic Web dimension tab (`F2` dropdown, digit `1`) inspects
it (orbit/pan/log-zoom, click-node readout, live player point,
`Home` top-down snap) — plus the System Map on the Solar System tab
(digit `6`: inclined orbit rings + planets + L4 focus + travel offer)
plus the orbit arrival binding (viewer rebuilds at descriptor
radius with the atmosphere tint). The Game Demo tab drives the
player directly (no journey-layer following); the dimension tabs keep
their travel keys (`E` drills down, `T`/`Q` offer/ascend) verbatim.
`Descent`/`Surface` (L6–L7) land with the descent milestone (M2).

Rendering side: [`../techstack/rendering.md`](../techstack/rendering.md). Budgets: [`../techstack/quality.md`](../techstack/quality.md).
