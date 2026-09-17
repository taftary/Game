# Notion — universe-maps

## Status

`done` (all 8 DoD criteria checked — ANALYST reproduced every row on
commit `5365652`, SECURITY passed 2026-09-17 with no findings; windowed
visual rows accepted by PO per 2026-09-17 close instruction)

## Context

M0 (scaffold) and M1 (renderer smoke) are done. The roadmap was
re-sequenced by ADR-011 (`../../docs/decisions/ADR-011.md`): M5 content
(universe maps) executes before M2–M4; execution order is M0, M1, M5,
M2, M3, M4, M6, M7, M8; milestone numbers are labels, not sequence.
Landing-site selection moved from M5 to the descent milestone (M2).

[`journey.md`](../../../docs/game/journey.md) levels L1–L4 have no code:
`plans/v0.0.1/scale-hierarchy` confirmed only the planet-globe layer and below
have code (`HexSphere`, `OrbitCamera`, `PlayerCamera`,
`ChunkStreamer`). Today the viewers open on one ad-hoc `--seed`
planet: no galaxy, no systems, no travel, no planet types.

The v1 scope chain this feature implements against:

- [`scope.md`](../../../docs/game/scope.md) — v1 in: procedural galaxy →
  systems → planets with deterministic seeds; galaxy/system/planet map
  + travel.
- [`universe.md`](../../../docs/game/universe.md) — staged generation
  (1: galaxy seed → star systems; 2: system seed → planets;
  3: planet seed → surface) plus purity, versioning, stable content-ID
  and quantization rules.
- [`gameplay.md`](../../../docs/game/gameplay.md) — maps are UI-first, no
  manual interstellar flight; interplanetary transit is a timed action,
  cancellable before commit; Orbit is a holding state (survey, pick
  landing site, descend).
- [`journey.md`](../../../docs/game/journey.md) — L1–L4 extents, the v1
  state machine (`GalaxyMap` → `SystemMap` → `Orbit`), the f64/f32
  rule, one-active-layer rule, and the fade + spinner fallback.
- [`risks/`](../../../docs/risks/README.md) — #2 bit-stable procedural
  noise across ARM/x86 activates with this feature.

## Problem & Needs

Player need: depart from somewhere, choose where to go, arrive at a
planet — the travel flow [`gameplay.md`](../../../docs/game/gameplay.md)
promises. Without maps, travel and (later) site selection and descent
have no origin, and the journey rule "universe breadth comes from
count and variety" is unserved: one planet, no variety.

Developer need: deterministic universe data (seed → galaxy → systems →
planets) that later milestones consume — descent arrivals, surface
detail, saves. No throwaway map-only data: the descriptors built here
must be the same ones saves store later (stable content IDs per
[`universe.md`](../../../docs/game/universe.md)).

## Goals

1. `engine::universe` stages 1–2: seed → `GalaxyDescriptor` → `SystemDescriptor` → `PlanetDescriptor` (type, radius, mesh seed, atmosphere color/density, resource bias, visual-only companions), pure + versioned, f64/log-compressed map space.
2. Journey state machine top segment in the `game` lib (headless-testable): `GalaxyMap` (L2, L1 cosmic-web backdrop) → `SystemMap` (L3, with L4 planet focus as a SystemMap zoom mode per the journey drill-down) → `Orbit` (L5 arrival on the existing planet mesh + `OrbitCamera`) — explicit enter/exit, fade requests on layer swaps.
3. Selection-driven travel ([`gameplay.md`](../../../docs/game/gameplay.md): UI-first, no manual interstellar flight): star → system → planet → orbit. Interplanetary transit as a timed, cancellable-before-commit action; resource cost deferred with hooks to the colonies/resources milestone (M4).
4. ≥2 planet types visually distinct in focus/orbit views, descriptor-driven (palette/atmosphere variety axes per [`universe.md`](../../../docs/game/universe.md)), no hardcoded special-casing.
5. Interactive maps in `game_debug` (the `player-sphere-movement` precedent: pure logic in the lib, interactive shell in the debug viewer). Lib covered by fixed-step regression + determinism tests; `--headless` self-tests extended.

## Non-goals

- Descent / Ascent / Surface states (next milestone, M2); the L5 altitude bands are fully exercised there.
- Landing-site selection and precomputed site candidates (moved to M2; they need stage-3 surface data).
- Stage-3 surface detail: elevation, biomes, water table, POIs, site candidates (M2/M3).
- Save/load persistence (M4 era): the galaxy regenerates from a runtime seed; descriptors carry `universe_version` for forward compatibility.
- Transit resource/fuel cost model (M4 hooks).
- f64 outside map coordinates; all rendering stays f32 camera-relative.
- Ship / station / probe entities; moons/rings beyond backdrop sprites; manual flight of any kind.
- Release-binary windowed shell: the release entry stays headless; interaction lives in `game_debug`.

## Users / Stakeholders

- Player (v1): "where can I go, in 3 seconds?" Galaxy map → pick a star → system map → pick a planet → orbit. Reachable with mouse (click select, wheel zoom) + keyboard; touch/gamepad arrive with the M6 input pass ([`controls.md`](../../../docs/game/controls.md) unified action map is the target; `engine::input` is not built yet, so the debug viewer uses direct `winit` input as it does today).
- Developer (later milestones): stable descriptors + content IDs to land descent arrivals, surface detail, and saves onto.
- UX consulted (player-facing travel UI): consistency with [`controls.md`](../../../docs/game/controls.md) and the camera journey; failure states get a surface (see Constraints).

## Roles

Author: PO. UX consulted (required if player-facing): yes — travel UI is player-facing (debug-hosted under the `ux.md` debug-screen exemption; must not leak into the release binary).
ARCHITECT consulted (required if cross-module): yes — `engine::universe` + `game` journey + debug views.

## Functional requirements

- FR-1 `GalaxyDescriptor`: from (seed, version): star list (f64 log-compressed position, spectral class, companion count per [`universe.md`](../../../docs/game/universe.md) stage 1); galactic extent 10k–100k ly compressed ([`journey.md`](../../../docs/game/journey.md) L2).
- FR-2 `SystemDescriptor`: from (seed, version, star): star + planet orbit list at AU-scale compressed radii with patched positions (stage 2; L3).
- FR-3 `PlanetDescriptor`: type (≥2 of rocky / desert / ice / volcanic / toxic / oceanic), radius in the 2–8 km v1 start band, mesh-seed binding to `engine::render::SeededPlanet` params, atmosphere color/density, resource bias, visual-only companions; stable content IDs `galaxy/seed:…/system:…/planet:…`; carries `universe_version`.
- FR-4 Journey machine: states `GalaxyMap` / `SystemMap` / `Orbit` (L1 backdrop flag on `GalaxyMap`; L4 planet focus as a SystemMap zoom mode); enter/exit hooks; selection events drive transitions; layer swaps emit fade requests (no hard loading screens); the `Orbit` state never assumes it is the root (M2 appends `Descent` below).
- FR-5 GalaxyMap view (debug): star points + nebula impostors from descriptors; f64 map coords → f32 camera-relative draw; log zoom / pan / select; full-range zoom without jitter.
- FR-6 SystemMap view (debug): star + orbit rings + planets at patched positions; select planet → planet focus → `Orbit` arrival on the seeded planet mesh.
- FR-7 Travel: timed transit action between systems/planets, cancellable before commit ([`gameplay.md`](../../../docs/game/gameplay.md)); arrival binds the orbit camera to the target planet descriptor.
- FR-8 Headless: scripted `GalaxyMap → SystemMap → Orbit → back` traversal with expected state hash (regression); descriptor determinism tests (same seed + version → identical hashes across runs; quantized cross-platform per [`risks/`](../../../docs/risks/README.md) #2); doc tests for new `engine::universe` APIs ([`quality.md`](../../../docs/techstack/quality.md) test policy).

## Non-functional requirements

- Determinism: pure functions `(seed, version, id) → descriptor`; no wall-clock, no thread-order-dependent iteration in output ([`universe.md`](../../../docs/game/universe.md) rules).
- Precision: f64/log-compressed map space; f32 + camera-relative origin for all rendering ([`journey.md`](../../../docs/game/journey.md) rules).
- Perf: Low tier must hold the map views (points/impostors are cheap by construction); budgets per [`quality.md`](../../../docs/techstack/quality.md); nothing that blows Low ships ([`rendering.md`](../../../docs/techstack/rendering.md) tiers rule).
- Gates: full [`quality.md`](../../../docs/techstack/quality.md) gate list green, including the mobile compile-guards.

## Definition of Done

- [ ] Descriptor determinism: same seed + version → byte-identical Galaxy/System/Planet descriptor hashes across repeated runs on x86-64, outputs quantized per [`risks/`](../../../docs/risks/README.md) #2 for cross-platform stability; ARM runtime hash verification lands with M6 mobile hardening or the first ARM device run.
- [ ] Journey regression: fixed-step scripted traversal `GalaxyMap → SystemMap → Orbit → back` yields the expected state hash in automated tests.
- [ ] Windowed galaxy map: full-extent zoom in/out with no jitter or pop; star selection by click.
- [ ] End-to-end travel in the windowed viewer: pick star → system → planet → orbit arrival, every layer swap faded, no loading screen.
- [ ] Two planet types visually distinct in focus/orbit views, driven by descriptor with no hardcoded special-casing.
- [ ] Timed transit with pre-commit cancellation from the map UI; cost shown as deferred/hook (no real deduction).
- [ ] Rendering invariants intact: all pinned camera/projection tests still green.
- [ ] Docs synced: [`journey.md`](../../../docs/game/journey.md) implementation annotation for L1–L4, [`architecture.md`](../../../docs/techstack/architecture.md) universe module, techstack README version bump; every touched link resolves.

## Constraints & Assumptions

- Scale contract ([`journey.md`](../../../docs/game/journey.md)): L1 container = one generated galaxy per runtime seed (saves arrive in M4); L2 10k–100k ly compressed; L3 AU-scale compressed; L4 2–64 km radius band with v1 start 2–8 km, companions visual-only ([`scope.md`](../../../docs/game/scope.md)); orbit zoom-out stops at the L5 ceiling — the descent milestone owns the L5–L7 bands.
- Binding: [`rendering.md`](../../../docs/techstack/rendering.md) camera & screen-space conventions; one active high-detail layer at a time.
- UI: [`controls.md`](../../../docs/game/controls.md) consistency (touch-first direction; small-text and hover-only affordances are bugs even in debug-hosted UI); photosensitivity-safe transitions; every failure gets a surface — fade + spinner fallback per journey rules, a map-data miss never hard-crashes.
- Module boundaries ([`architecture.md`](../../../docs/techstack/architecture.md)): the `game` bin stays the clean headless gate (`cargo run --bin game` keeps passing); no `vulkano` pipeline code in `game`; debug screens never leak into the release binary.
- Assumes `debug-ui-reorganize` reaches `done` and the gnomonic-checker issue closes (windowed runs + unblocked gates) before this feature's `in-review` gate.

## Open questions

- OQ-1: v1 star count for the galaxy map (extent is fixed; count vs Low-tier point budget) — plan proposes, TECHLEAD approves.
- OQ-2: nebula impostor technique (billboard quads vs baked backdrop) — ARCHITECT decides at plan breakdown.
- OQ-3: v1 planet-type subset (≥2 from rocky / desert / ice / volcanic / toxic / oceanic) + palettes — UX consulted.
- OQ-4: runtime seed plumbing (CLI flag vs viewer panel input) — plan detail.
- OQ-5: L1 cosmic-web backdrop technique (procedural shader vs generated sprite field) — plan detail.
- OQ-6: exact transit timing model (fixed durations per hop class?) — [`gameplay.md`](../../../docs/game/gameplay.md) says timed; numbers are plan detail.
