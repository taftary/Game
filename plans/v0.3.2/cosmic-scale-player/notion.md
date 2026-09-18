# Notion — cosmic-scale-player

## Status

`in-review` (DEV gates green + evidence in `plan.md`; awaiting
ANALYST DoD verification + SECURITY review per `plans/README.md`
role gates)

## Context

v0.3.1 unified the debug shell (`game_debug`, ADR-022): one window, three
top-level items (`GAME DEMO` / `DIMENSIONS` dropdown of the 10 waypoints /
`SETTINGS`). The Game Demo tab today only *follows the journey layer*
(galaxy/system/planet maps) and every travel key (`E`/`T`/`Q`) navigates
away into a dimension tab. Seven of the ten dimension tabs are
placeholders — including the Cosmic Web tab (dropdown digit `1`), which is
completely empty.

There is no player in space anywhere in the project: `game::player::Player`
is a sphere-bound lon/lat walker (v0.0.1 `player-sphere-movement`), and
`engine::flight` (`ShipState`, `step_free_flight`, `plan_fly_to` — the
shipping navigation core from v0.1.0 `free-flight-navigation`) is
frame-generic but has never been wired to a camera, a window, or any frame
above `FrameId::SolarSystem`. W1/L1 (Cosmic Web) is explicitly
"backdrop-only, not traversable" (`docs/game/journey.md`); it holds only
400 dim backdrop sprites — while spec §1 promises "Filaments, voids,
procedural large-scale structure".

PO decision (2026-09-18): v0.3.2 turns the Game Demo into the seed of the
actual game — **a player in space, navigating the Cosmic Scale**. This is
the main game notion going forward: all later layers (galaxy, systems,
orbit, surface) are reached by the player *travelling down* from this
starting dimension.

PO decision (2026-09-18, v2 scope review): the DIMENSIONS shell is **kept
as shipped** — the dimension tabs remain the inspection surface; the Game
Demo is rebuilt around the cosmic player; and the Cosmic Web dimension tab
mounts the generated web as the fourth absorbed view (Galaxy Map →
Milky Way, System Map → Solar System, Planet View → Earth, now Cosmic Web
view → Cosmic Web tab). Only the Cosmic Web tab gains content; the
Supercluster (W2) and LocalGroup (W3) tabs stay placeholders.

PO decision (2026-09-18): the visual-reference image step is **dropped** —
no file is saved under `assets/`. The visual target is captured textually
in Goals §3 (Stage D palette and framing). `assets/references/` is not
created; `assets.md` is untouched by this feature.

## Problem & Needs

- The demo shows *map navigation*, not *player navigation*; the shipped
  navigation engine has zero interactive validation at any scale.
- W1 has no generated content — 400 dim sprites while spec §1 promises
  filaments/voids/large-scale structure — **and its dimension tab is an
  empty placeholder**, so even inspection is impossible.
- The starting dimension of the game cannot be seen, entered, or flown
  inside — by players or by developers.

## Goals

1. **Cosmic-scale player.** The player *is* an `engine::flight::ShipState`
   in `FrameId::Cosmological` (f64 Mpc frame-chain position + real
   momentum), spawned *inside a filament* 20–40 Mpc from the home node,
   looking along the filament toward a bright cluster node — the whole
   frame is filled with structure. No ship mesh: the player is represented
   **only by a marker** (screen-space dot + heading arrow + `YOU` label via
   `world_to_pixels`, the same pinned pattern as the sphere marker;
   FirstPerson hides it automatically by design).
2. **Player camera.** A space camera with 3 modes, `P` cycles: **Chase**
   (default; behind/above the marker, aligned to ship orientation),
   **Orbit** (free orbit around the marker), **FirstPerson** (eye at the
   ship position, marker hidden). Projection stays un-flipped
   `directx::perspective`; near/far are **derived** from distance/scene
   extent (the `MapOrbitCamera` policy — the fixed 0.05/1000 pair is
   invalid at Mpc). f64 anchor at the ship position; all content uploads
   camera-relative via `frames::recenter` (ADR-013).
3. **Generated Cosmic Scale — the seeded Zel'dovich web** (new
   `engine::universe` **stage 0**, one deterministic descriptor covering a
   ~250 Mpc-radius sphere around the home supercluster — the largest
   inscribed in the 512 Mpc periodic lattice box):
   - **A. Gaussian initial field** on an integer Lagrangian lattice (128³
     cells × 4 Mpc): hashed cell streams
     (`RegionSeed::layer("cosmic_web/…")`, ADR-019), Irwin–Hall Gaussian
     shaping (sum of 12 uniforms — pure arithmetic, no ln/cos), dyadic
     smoothing reproducing the ΛCDM-like spectrum rollover (n ≈ +1 → −3).
     No transcendentals in hashed paths (the `generate_galaxy` rule).
   - **B. Zel'dovich displacement** x = q − D₊·∇Ψ(q) via central finite
     differences (pure arithmetic); D₊ is a single "cosmic time" runtime
     parameter. Anisotropic collapse (sheets → filaments → nodes) is what
     makes the web a web.
   - **C. T-web classification** (tidal-tensor eigenvalues vs calibrated
     λ_th: node = 3 above, filament = 2, sheet = 1, void = 0) plus a
     **Press–Schechter n=0 halo mass function** (power law + exponential
     cutoff above M\*). One ~2×10¹² M☉ group node is tagged **home** —
     it contains the Milky Way.
   - **D. Renderable descriptor**: nodes (f64 Mpc, mass, r_vir,
     L ∝ M, blue-white → yellow-white by mass) + filament links (linking
     length with inter-node threshold path, seeded dwarf glow-points
     along each link with ~6 Mpc transverse falloff) + voids rendered as
     darkness. ≈5,000 nodes / ≈20,000 links / ≈150,000 glow points — one
     `PointList` + one `LineList` draw through the existing map/line
     pipelines. Hubble redshift tint + cue haze (`render::cue`, existing)
     + HDR exposure/tone-mapping (v0.2.0). Palette: white-yellow massive
     nodes, lavender/purple filaments, near-black indigo voids.
   - Calibration targets (bands, not laws): void volume fraction in
     [60, 90] %; void median diameter in [10, 100] Mpc; filaments commonly
     50–80 Mpc long, ~6 Mpc thick; clusters at filament intersections
     (Zel'dovich 1970; Press & Schechter 1974; Forero-Romero et al. 2009;
     Libeskind et al. 2018; survey sizes per CfA2/Sloan/Quipu).
4. **Space navigation.** Physical free flight (mouse steers heading; `W`/`S`
   main thrust; `A`/`D` lateral thrust; real momentum, rotation
   stabilization ON, translation damping OFF — the spec §10 craft
   envelope; **zero live gravity** per spec §5 / `StaticDensityField`;
   automatic time compression via `CompressionClock`/`Occupancy` up to the
   Cosmological ceiling 10⁹) **plus fly-to assist**: click a node (8 px
   pick via `project_to_screen`) to target it, `E` engages/cancels
   `plan_fly_to` (10 s/decade, smootherstep, `FlyToExec`, continuous
   state hand-back on cancel).
5. **Game Demo rebuild.** `App::screen_content()` maps `GameDemo` to a new
   `ViewContent::CosmicWeb`; the demo renders the player-in-web scene with
   the shipping HUD and keeps its per-tab chrome defaults (all hidden).
   Galaxy/system/planet content is unmounted *from the demo only* — the
   dimension tabs keep their content, keys, and travel side-effects
   verbatim (v0.3.1 behavior preserved). The hand-rolled journey-following
   builders in the demo are replaced by the cosmic scene + live HUD; the
   hint line reads
   `mouse steer · WASD thrust · click target · E fly-to · P camera`.
6. **Cosmic Web dimension tab (fourth absorbed view).** New inspector
   module (`crates/debug/src/cosmic_web.rs`): the shared stage-0
   descriptor rendered through the existing map point + line pipelines;
   a `MapOrbitCamera`-style inspector camera (orbit / pan-screen /
   log-zoom / `Home` top-down snap, Mpc units, derived near/far); a left
   dock (260 px, padded, per absorbed-view rules) with web stats (seed,
   node/link counts, void fraction, home-node info) plus a selected-node
   readout (mass, r_vir, link count) on click-pick (8 px); the ship's live
   position drawn as **one highlighted map point** inside the buffer (map
   content — no marker-contract touch); strictly read-only — it never
   mutates `Journey`/sim state (pinned test). `WaypointId::CosmicWeb` is
   removed from the placeholder path; W2/W3 stay placeholders.
7. **Real HUD.** `game::hud` is wired into the demo, fed by the live
   `ShipState` / `CompressionClock` / `FlyToExec` (frame/time/target
   lines; SOI line stays `—` at this scale), replacing the hand-rolled
   placeholder lines. The transition pill + console show **real fly-to
   events**; the fake `preview()` seed data is deleted.

## Non-goals

- Descending into the galaxy / any frame handoff (Galactocentric, SOI,
  LocalGroup): the home node is a fly-to-able impostor, not an enterable
  scene. That is the *next* version.
- Ship mesh / cockpit geometry; collision; propellant consumption; live
  gravity at W1.
- **Sheet/wall rendering** (sheets are classified in Stage C but not
  drawn — follow-up).
- **Region streaming** (single ~400 Mpc sphere; `RegionId`-cell streaming
  around the player is a documented follow-up).
- **Removing DIMENSIONS or any v0.3.1 shell element** — explicitly out
  (PO decision v2).
- Journey-machine or game-crate changes (journey, transit, absorbed views
  stay as shipped).
- Supercluster / LocalGroup tab content; remappable key bindings;
  autosave wiring into the debug shell; catalog-sky rewiring;
  touch/gamepad input (keyboard + mouse only in v0.3.2 — a developer
  shell).

## Users / Stakeholders

- Developers (direct): building and validating the navigation core
  against the real thing for the first time; strip or gate before any
  release.
- Players (indirect): the Game Demo tab is presentation-accurate, so it
  previews the shipped opening experience — UX consulted for that surface
  (see Roles).

## Roles

Author: PO (2026-09-18). UX consulted (required, player-facing demo
surface): yes — 2026-09-18 design review, v2 scope: demo = player surface
(steer toward a bright node within 3 s; `E` shows target + ETA with
instant cancel; `P` cycle includes marker-hidden FirstPerson; no-target
`E` is a no-op + hint; fly-to cancel posts a notice; keyboard + mouse
recorded as acceptable for a developer shell) and tab = inspector surface
(absorbed-view conventions: orbit/pan/zoom + `Home` snap, click-node
readout, player point highlighted but visually distinct, padded docks;
key↔button parity preserved — registry grows by exactly one action and is
re-pinned). ARCHITECT consulted (required, cross-module): yes —
breakdown in `plan.md` carries module + invariant annotations; ADR-023
required (**extends** ADR-022: demo content mapping + fourth absorbed view
+ stage-0 generation + marker/camera contract); `UNIVERSE_VERSION` 1 → 2
recorded.

## Functional requirements

- Boot: `game_debug` lands on GAME DEMO showing the generated web; the
  player spawns inside a filament 20–40 Mpc from the home node; the Chase
  camera faces a bright cluster node. `--seed N` / `R` reseed live; the
  same (seed, `UNIVERSE_VERSION`) replays a byte-identical descriptor.
- Flight: mouse drag steers heading; `W`/`S` ±main thrust; `A`/`D`
  lateral thrust; thrust is physical (30 m/s² sim, compression-scaled);
  unthrusted coast preserves momentum bit-exact at fixed step.
- Fly-to: left-click selects the nearest node within 8 px (highlight +
  target line); `E` engages the eased fly-to; `E` again or any thrust
  input cancels with continuous state hand-back; arrival = node center
  (no standoff offset in v0.3.2).
- `E` is context-dependent: fly-to engage/cancel where the demo's cosmic
  content is active; travel-begin where galaxy/system content is active
  (existing dual-binding precedent: `T` = travel offer / top preset).
- Camera: `P` cycles Chase → Orbit → FirstPerson; Orbit gets drag-rotate +
  wheel zoom; FirstPerson hides the marker and looks along the ship
  heading.
- Chrome: F1/F2/F3, dropdown digits (Cosmic Web = digit `1`), docks,
  widget, `Esc` unwind — all unchanged from v0.3.1. Settings → Controls
  lists the new flight action with its key.
- Cosmic Web tab: orbit / pan / log-zoom / `Home` snap; click-node
  readout; live player point; left dock with web stats; strictly
  read-only.
- Stage-0 content satisfies the calibration bands in Goals §3
  (headless-verified, tolerance-banded).

## Non-functional requirements

- Quality gates in `docs/techstack/quality.md` stay green
  (`cargo fmt --check`; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`; `cargo build --workspace`;
  `cargo test --workspace --all-targets`; `cargo test --doc --workspace`;
  `cargo run -p game_debug -- --headless`; `cargo run -p game_tools
  -- --headless --tier low`).
- Rendering invariants in `docs/techstack/rendering.md` preserved
  verbatim; the cosmic-marker path is pinned by new tests (marker visible
  in Chase/Orbit; `world_to_pixels` returns `None` in FirstPerson;
  projection un-flipped).
- Determinism: byte-identical stage-0 descriptor per (seed,
  `UNIVERSE_VERSION`); committed hash/snapshot vectors; integer decisions
  + sqrt-only in hashed paths (universe-maps rule); any exp/log confined
  to value transforms per the `cue.rs` precedent, pinned by snapshots.
- One point draw + one line draw for web content per surface (demo,
  inspector), inside existing debug budgets; descriptor memory
  single-digit MB (galaxy map precedent: ~1.2 MB for 25k stars).
- No new third-party dependencies.
- New public `engine` APIs carry doc tests (quality.md test policy).

## Definition of Done

- [ ] Demo rebuilt: `GameDemo → ViewContent::CosmicWeb`; journey content
      unmounted from the demo only; dimension tabs (content, keys, travel
      side-effects, dropdown, badges, placeholders for W2/W3 + 4 others)
      behave exactly as v0.3.1.
- [ ] Stage-0 generator: 4-stage seeded Zel'dovich pipeline
      (A field → B displacement → C classification + masses → D
      descriptor); deterministic; home node tagged; hash vectors
      committed; `UNIVERSE_VERSION` 1 → 2 (`SEED_VERSION` unchanged —
      the `RegionId` grammar is untouched).
- [ ] Statistical-realism gates green (headless, tolerance-banded): void
      volume fraction ∈ [60, 90] %; void median diameter ∈ [10, 100]
      Mpc; filament lengths reach the 50–80 Mpc class; node mass function
      follows power-law + exponential-cutoff shape.
- [ ] Cosmic Web tab: renders web + live player point; inspector camera
      (orbit/pan/zoom/`Home`); left-dock stats + click-node readout; no
      placeholder; read-only — dimension selection and rendering never
      mutate Journey/System/Viewer state (pinned test).
- [ ] Player = `ShipState` in `FrameId::Cosmological`; momentum flight
      bit-reproducible at fixed step; compression ceiling 10⁹ verified
      via `Occupancy`.
- [ ] Marker via `world_to_pixels` in every camera mode; FirstPerson
      hides it; contract tests pinned.
- [ ] 3-mode space camera with distance-derived near/far; no precision
      jitter at Mpc offsets (camera-relative `recenter` path).
- [ ] Fly-to: click-select + `E` engage/cancel; eased arrival at node
      center; HUD target line with distance + ETA.
- [ ] `game::hud` live in the demo (frame/time/target; SOI `—`);
      pill + console fed by real fly-to events; `preview()` seed data
      deleted.
- [ ] Docs sweep done (`controls.md`, `journey.md` — L1 traversable in
      the demo + Cosmic Web tab mounted, `universe.md` — stage 0,
      `architecture.md`, `rendering.md` — marker/camera contract,
      milestones v0.3.2, techstack version bump, ADR-023 extending
      ADR-022); every touched link resolves.

## Constraints & Assumptions

- Spec `cosmic-navigation-engine-v0.4.md` applies verbatim: §1 (W1
  content), §2 (free flight + compression, not a global multiplier), §5
  (cosmic scale = static density field, no live gravity), §6
  (hierarchical seeding, ADR-019 domains), §8 (procedural, explicitly
  flagged — no literal catalog), §10 (craft envelope: 5,000 kg,
  30 m/s², fuel persisted but consumption disabled).
- Depends on shipped, read-only APIs: `engine::flight` (`ShipState`,
  `step_free_flight`, `plan_fly_to`/`FlyToExec`, `CraftParams`),
  `engine::time` (`CompressionClock`, `Occupancy`), `engine::frames`
  (`FrameId::Cosmological`, `recenter`), `engine::seeding`
  (`region_seed`, `RegionId`), `engine::physics::StaticDensityField`,
  `render::cue` (web density, Hubble redshift), `game::hud`.
- Module boundaries (`docs/techstack/architecture.md`): `game` gets no
  `vulkano` code; `engine::universe` stays pure + deterministic;
  developer screens never leak into the release binary. The space camera
  and the Cosmic Web inspector live in the `debug` crate (map-domain
  cameras, `MapOrbitCamera` precedent); promotion to `game`/`engine` is a
  later version's decision.
- ADR-022 (unified shell) stays fully in force; ADR-023 extends it (demo
  content mapping, fourth absorbed view, stage-0 generation,
  marker/camera contract).
- Each feature lands as exactly one commit on branch `v0.3.2`
  (`plans/README.md` §7); docs-only commits (`docs:`) separate.
- Calibration targets are bands, not laws; the web is explicitly
  procedural per spec §8 (flagged in docs + ADR-023).
- The v0.3.0 `dimension-debug` notion (still `in-progress`) is
  cross-linked, not closed, by this feature — its L1-tab aim is partially
  realized here; reconciling its status is a separate PO decision.

## Open questions

- Fly-to arrival point = node center (no standoff offset in v0.3.2) —
  default taken; revisit when handoff lands.
- `T`/`Q`/`F`/`U`/`G`/`B`/`Home` (`Home` stays tab-scoped: top-down snap
  in map tabs, unbound in the demo) stay as v0.3.1 except where the demo
  remaps `E` to fly-to.
- D₊ (cosmic time) and λ_th (web threshold) live in a `CosmicWebParams`
  struct (runtime params, not constants); nominal lattice 128³ × 4 Mpc /
  radius 250 Mpc — tuned during WS1, recorded in `plan.md`.
- Region streaming (`RegionId` Cosmological cells around the player) is
  the documented follow-up, not this version.
