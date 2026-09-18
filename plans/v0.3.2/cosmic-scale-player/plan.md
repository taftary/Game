# Plan — cosmic-scale-player

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes (module + invariant annotations; dependency-safe order:
foundations before surfaces; ADR-022 shell intact throughout):

### WS1 — Stage-0 seeded Zel'dovich web (`engine::universe`, pure)

New module `engine::universe::web` (files: `field.rs` stages A–B,
`classify.rs` stage C, `descriptor.rs` stage D, `params.rs` shared
`CosmicWebParams`, re-exported in `universe/mod.rs`). Consumes
`engine::seeding::{region_seed, RegionId}` (ADR-019 domains
`cosmic_web/field`, `cosmic_web/displace`, `cosmic_web/classify`,
`cosmic_web/populate`, `cosmic_web/links`) and `engine::frames::FrameId`
for `RegionId` cells; nothing else. Integer decisions + sqrt-only in
hashed paths; any exp/log confined to value transforms per the `cue.rs`
precedent and pinned by snapshot vectors. `UNIVERSE_VERSION` 1 → 2
(explicit fork); `SEED_VERSION` untouched (grammar unchanged); galaxy
streams (`galaxy/stars`, …) untouched → existing galaxy descriptors
replay identically. Invariant rows: "universe pure + deterministic —
same (seed, version) ⇒ byte-identical descriptor", "statistical bands,
not laws — tolerance-banded headless asserts only".

### WS2 — Cosmic player state + flight wiring (`debug` app state over `engine` flight APIs)

New `debug::cosmic_player` state: owns `ShipState` (active frame
`Cosmological`, f64 Mpc), `CompressionClock`, optional `FlyToExec`,
input intent (throttle/strafe/steer), and a fixed-step accumulator fed
from the existing per-frame tick pattern (`tick_player` area). Gravity
closure = no-op returning `DVec3::ZERO` (spec §5 + `StaticDensityField`
contract — asserted, not assumed). `Occupancy { frame: Cosmological,
in_blend_band: false, depth_fraction }` drives the 10⁹ ceiling via
`target_ratio`. Spawn: pick the home node, walk its strongest link,
place the ship 20–40 Mpc out along the link, aim at the far endpoint.
No `game`-crate changes; `engine::flight` consumed read-only. Invariant
row: "coast is bit-exact at fixed step (flight precedent)".

### WS3 — Space camera + cosmic marker (`debug` render path)

New `debug::cosmic_camera`: `CosmicCamera { mode: Chase|Orbit|FirstPerson,
anchor: DVec3 (ship f64), yaw, pitch, distance }`; Chase aligned to the
ship orientation, Orbit = free rotate/zoom around the anchor (near/far
derived from distance × scene extent, `MapOrbitCamera` policy),
FirstPerson = eye at anchor along heading. Projection = glam
`directx::perspective` (never `vulkan::perspective`); all content draws
use `view_proj` built from the f64 anchor with `frames::recenter`.
Marker: reuse binary-local `world_to_pixels` + existing `draw_player_marker`
UI path into `UiItems` for the demo viewport; FirstPerson yields `None`
by construction (pinned). Invariant rows: "projection un-flipped",
"marker only via `world_to_pixels`", "FrontFace/CCW rules hold for any
new solid geometry (points/lines exempt)".

### WS4 — Game Demo rebuild (`debug` app + main; shell otherwise untouched)

`ViewContent::CosmicWeb` variant; `screen_content()` maps `GameDemo → CosmicWeb`;
demo dispatch builds the web buffers (one `PointList` + one `LineList`
draw, camera-relative f32 via `recenter`, sky-clear indigo) + HUD
(`game::hud` fed by live ship/clock/executor — `SoiReadout: None`,
`target` only in `FlyTo` mode) + hint line; the journey-following demo
builders are unmounted from the demo path. Registry: add
`Action::FlyToToggle` (`E`, Travel group, button label `[E]`), re-pin
`registry_size_is_pinned` 44 → 45; `T` dual-binding precedent covers the
context-dependent `E` (fly-to in demo / travel-begin in galaxy-system
tabs). Headless: rewrite only the demo-content asserts
(`screen_content() == CosmicWeb`, boot framing); dropdown/placeholder/tab
asserts stay green. Invariant row: "journey-following demo removed, but
no `Journey`/transit behavior changes — tabs are pixel/byte-identical".

### WS5 — Cosmic Web dimension tab (`debug` new inspector module)

New `debug::cosmic_web::CosmicWebInspector`: holds its own
`MapOrbitCamera`-style inspector state over the shared `Arc`-style
descriptor (borrowed from the demo state — single generation, read
through two cameras). Dispatch arm `Dimensions(CosmicWeb) →
build_cosmic_web_ui` replaces the placeholder arm; left dock (260 px)
with web stats + selected-node readout; click-pick via
`project_to_screen` (8 px, `PICK_RADIUS_PX` precedent); `Home` =
top-down snap; player live position appended as one highlighted point
(map content, distinct color/size). Pinned test: selecting/rendering the
tab never mutates `Journey`/system/viewer state (the `dimension-debug`
contract). Absorbed-view doc rules updated.

### WS6 — Fly-to select/engage/cancel + real pill/console feed (`debug` over `game::hud` + `flight`)

Click (demo viewport) → `Target { frame: Cosmological, position }`
(nearest node, `CrossFrame` impossible by construction);
`E` → `plan_fly_to` → `FlyToExec` (10 s/decade precedent); `E`/thrust
cancels with continuous hand-back; arrival → notice + console entry.
`App::sync_console` drains a real event queue (fly-to start/cancel/
complete, target select) instead of `transitions.preview()` — `preview()`
deleted; the pill renders `FlyTo` progress only (hidden otherwise).
`game::hud::update` feeds demo lines. Invariant row: "spec §2 —
constant time per decade only for select-to-focus; free flight has no
fixed pacing".

### WS7 — Gates + docs sweep + ADR-023 (repo-wide)

Full gate list from `docs/techstack/quality.md` green; docs sweep
(`controls.md`, `journey.md`, `universe.md`, `architecture.md`,
`rendering.md`, milestones v0.3.2 section, `plans/README.md` list,
techstack version bump 0.32.0 → 0.33.0, ADR-023 *extending* ADR-022);
link check; ANALYST DoD audit + SECURITY review; single `done` commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CSP-001 | completed | Stage A: integer-lattice Gaussian field (`field.rs`): hashed cell streams, Irwin–Hall shaping, dyadic smoothing; replay-identical test | Goals §3A, NFR |
| CSP-002 | completed | Stage B: Zel'dovich displacement via central differences; D₊ param; displacement snapshot vectors | Goals §3B, NFR |
| CSP-003 | completed | Stage C: T-web eigenvalues (closed-form, sqrt-only) + λ_th calibration; Press–Schechter n=0 masses; home-node tag | Goals §3C |
| CSP-004 | completed | Stage D: descriptor (nodes/links/glow points) + hash vectors + public-API doc tests; `UNIVERSE_VERSION` → 2 | Goals §3D, NFR, DoD 2 |
| CSP-005 | completed | Statistical-realism gates (void frac/volume, void Ø, filament lengths, mass-function shape), tolerance-banded headless tests | Goals §3, DoD 3 |
| CSP-006 | completed | `CosmicPlayerState`: ShipState in Cosmological + CompressionClock + fixed-step accumulator; zero-gravity closure asserted static | Goals §1/4, DoD 5 |
| CSP-007 | completed | Tick wiring: thrust/strafe/steer intent → `step_free_flight`; Occupancy 10⁹ ceiling verified; coast bit-exact test | Goals §4, DoD 5 |
| CSP-008 | completed | Spawn-in-filament selection (home node → strongest link → 20–40 Mpc offset, aimed at far node) | Goals §1, FR |
| CSP-009 | completed | `CosmicCamera` (Chase/Orbit/FirstPerson, derived near/far, f64 anchor + `recenter`); un-flipped-projection pin | Goals §2, DoD 7 |
| CSP-010 | completed | Cosmic marker via `world_to_pixels` + existing marker UI path; FirstPerson-`None` pin; jitter test at Mpc offsets | Goals §1, DoD 6/7 |
| CSP-011 | completed | Demo rebuild: `ViewContent::CosmicWeb`, dispatch, web buffer builds (1 pt + 1 line draw), sky clear; journey builders unmounted from demo path | Goals §5, DoD 1 |
| CSP-012 | completed | `game::hud` live wiring in demo (frame/time/target; SOI `—`); hint-line update; placeholder demo lines deleted | Goals §7, DoD 9 |
| CSP-013 | completed | Registry +1 (`Action::FlyToToggle [E]`, Travel group); re-pin 45; Settings → Controls flight row | Goals §5, FR |
| CSP-014 | completed | Cosmic Web inspector module: view state, inspector camera, docks, node pick + readout, player point, dispatch arm replacing placeholder | Goals §6, DoD 4 |
| CSP-015 | completed | Inspector read-only pin: selection/rendering never mutates Journey/System/Viewer state | Goals §6, DoD 4 |
| CSP-016 | completed | Fly-to: click-select → `plan_fly_to` → `FlyToExec`; `E`/thrust cancel with hand-back; arrival notice; HUD target line | Goals §4, DoD 8 |
| CSP-017 | completed | Real pill/console feed from fly-to event queue; `transitions.preview()` deleted; pill hidden outside `FlyTo` | Goals §7, DoD 9 |
| CSP-018 | completed | Headless rewrite (demo-content + cosmic asserts; dropdown/tab asserts preserved) + full gate suite green | NFR, DoD 1 |
| CSP-019 | completed | Docs sweep + ADR-023 (extends ADR-022) + milestones v0.3.2 + version bump; link check | DoD 10 |
| CSP-020 | pending | ANALYST DoD audit + SECURITY review; single `done` commit on `v0.3.2` | plans/README §7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-18 — module boundaries legal:
new engine code is pure + deterministic under `engine::universe`; new
debug code is map-domain like `MapOrbitCamera`; `game` untouched; no
vulkano outside `debug`/`tools`; locked choices changed only via
`UNIVERSE_VERSION` bump + ADR-023) · Todos approved by: TECHLEAD
(2026-09-18 — risk-first where it matters: WS1 pure-first, WS2/WS3
invariant-sensitive work before WS4 integration, WS5 reuses WS4 buffers;
every todo independently verifiable with one row; budgets checked: +2
draws/surface worst case, descriptor single-digit MB — inside Low tier;
gate commands listed in WS7) · UX acceptance rows (player-facing):
approved 2026-09-18 — see Acceptance criteria UX-1…UX-6 · DoD verified
by: ANALYST _(pending — WS7)_ · Security reviewed by: SECURITY
_(pending — WS7)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Demo rebuilt (`GameDemo → CosmicWeb`); tabs behave as v0.3.1 | pending | CSP-011, CSP-018 | ANALYST (WS7) |
| 2 | Stage-0 generator deterministic; home tagged; hashes committed; `UNIVERSE_VERSION` 2 | pending | CSP-001…CSP-004 | ANALYST (WS7) |
| 3 | Statistical-realism gates green | pending | CSP-005 | ANALYST (WS7) |
| 4 | Cosmic Web tab renders web + player point; read-only (pinned) | pending | CSP-014, CSP-015 | ANALYST (WS7) |
| 5 | Player = ShipState in Cosmological; fixed-step reproducible; 10⁹ ceiling | pending | CSP-006…CSP-008 | ANALYST (WS7) |
| 6 | Marker via `world_to_pixels`; FirstPerson hides; pinned | pending | CSP-010 | ANALYST (WS7) |
| 7 | 3-mode camera, derived near/far; no Mpc jitter | pending | CSP-009, CSP-010 | ANALYST (WS7) |
| 8 | Fly-to select/engage/cancel; arrival at node center; HUD target line | pending | CSP-016 | ANALYST (WS7) |
| 9 | `game::hud` live; pill/console real; `preview()` deleted | pending | CSP-012, CSP-017 | ANALYST (WS7) |
| 10 | Docs sweep + ADR-023; links resolve | pending | CSP-019 | ANALYST (WS7) |

## Acceptance criteria

ARCHITECT invariant rows (must hold at `done`, verified by tests where
marked [T]):

- A-1. `engine::universe::web` is pure + deterministic: same (seed,
  `UNIVERSE_VERSION`) ⇒ byte-identical descriptor [T: hash vectors].
- A-2. Hashed paths use integer decisions + sqrt-only; no sin/cos/ln/exp
  in seed derivation [T: audit test over the module + snapshot vectors].
- A-3. Galaxy streams untouched: `generate_galaxy` descriptors replay
  identically before/after (existing determinism tests stay green).
- A-4. Projection stays un-flipped `directx::perspective` everywhere
  (demo + inspector); front-face/cull rules unchanged [T].
- A-5. The cosmic marker (demo) uses only `world_to_pixels`; inspector
  picking uses only `project_to_screen` + the pinned
  `ndc = (2u−1, 1−2v)` path [T].
- A-6. Free-flight coast is bit-exact at fixed step in the Cosmological
  frame [T].
- A-7. Cosmic Web tab selection/rendering never mutates
  Journey/System/Viewer state [T].
- A-8. No vulkano/pipeline code outside `debug`/`tools`; `game` crate
  diff = none; save-envelope version unchanged.
- A-9. Registry parity holds: every action has key label + title + group;
  size re-pinned at 45 [T].

UX acceptance rows (observable player-side behavior):

- UX-1. Boot lands on GAME DEMO: web fills the frame, marker visible, a
  bright cluster node in view; steering toward it works within 3 s.
- UX-2. `P` cycles Chase → Orbit → FirstPerson with an on-screen mode
  indication; FirstPerson hides the marker.
- UX-3. Clicking a node shows a target affordance; `E` engages fly-to
  with distance + ETA in the HUD; `E`/thrust cancels instantly with a
  notice.
- UX-4. Cosmic Web tab: orbit/pan/zoom + `Home` behave like the other
  map tabs; clicked node shows mass/r_vir/link readout; player point is
  highlighted and visually distinct from nodes.
- UX-5. Every flight key has a discoverable button and vice versa
  (Settings → Controls lists the flight group); no dead keys in the demo.
- UX-6. Failure surfaces: no-target `E` = no-op + hint line; fly-to
  cancel = notice; reseed (`R`) = notice with new seed value.

## Risks & Next steps

### WS1 tuning outcome (recorded 2026-09-18 — nominal params are final)

- **Periodic box**: all stencils (blur, gradient, Hessian, neighborhoods,
  deposit, watershed BFS) wrap; `border_cells() = 0`; fit rule is
  `radius + cell < half_width`. The notion's "~400 Mpc" was corrected to
  a **250 Mpc** descriptor sphere (largest inscribed in the 512 Mpc box).
- **Contrast**: D₊ 1.0 → **3.0** (D₊ = 1 shuffled tracers sub-cell:
  void fraction 0.37, 24 peaks). Threshold 0.03 → **0.06** (node cells
  7.5% → 2.6%).
- **Yield**: peak cut 2.5 → 2.0 → **1.7**; seeds from node **or filament**
  cells (groups live in filaments); min separation 8 → **6 Mpc**;
  glow 1.2 → **2.0**/Mpc. Result: 10,305 peaks → 6,000 nodes,
  22,666 links, 149,977 glow (cap), home 7.3e12 M☉.
- **Eulerian smoothing REJECTED**: a radius-1 de-noise pass collapsed the
  void fraction 0.63 → 0.18 (filled real evacuated cells). Raw NGP kept.
- **Voids**: ZOBOV-style watershed (minima compared *within* the void
  mask — singleton fix for ringed cells); catalog floor Ø ≥ 8 Mpc
  (VoidFinder precedent); median **12.0 Mpc**, top voids ~24 Mpc,
  75k cataloged basins.
- **PS table**: flat-tail stagnation (tail increments below accumulator
  resolution) fixed by right-edge quantile resolution.
- **Exact-half knife-edges**: lattice-exact positions can sit exactly on
  half-milli-Mpc points; stable cross-platform by exact arithmetic
  (correctly-rounded ops), so the ulp test targets the libm-derived
  fields (mass via exp-table, radius via cbrt) with a 1-ulp-relative
  drift model instead.
- **Committed vectors**: web(1234, nominal) = 14062203475186774008;
  galaxy(1234, 100) re-rolled to 11228504424767388277 (v2 stamp);
  system vector unchanged (no version field — content untouched).
- Final nominal stats: void_frac 0.629 · nodes 6,000 · links 22,666 ·
  glow 149,977 · home 7.3e12 M☉ · max link ≥ 50 Mpc · median mass < M\*.
- R-1 retired (calibration landed inside all bands with margin).

### WS4 integration notes (recorded 2026-09-18)

- Buffers hold origin-relative f32; camera recenters on the same
  `upload_origin` (new `CosmicCamera::origin` — anchor does math,
  origin does matrices); rebase past 50 Mpc rebuilds at the ship
  (`tick_cosmic` → `rebased()` → `refresh_cosmic()`). Marker uses the
  same origin frame (`recenter(ship, O)`), so dot and web never jitter
  apart. Fixed-center (no rebase) was analyzed and rejected: stale
  origins degrade nearby precision to the 15 kpc f32 rim quantum.
- Marker reuses binary-local `world_to_pixels` + `draw_player_marker`
  pin-for-pin (contract A-5); gated on the demo *screen* (the WS5 tab
  shows the player point instead).
- Input routing is demo-first for drag/wheel/`P` (an armed walker
  persists across screens and must not swallow demo input); WASD/arrows
  drive cosmic thrust on the demo (releases always clear);
  `hold_walk` + release path cover both walkers for Settings parity;
  `R` reseeds cosmic on the demo; `E` stays travel-bound on tabs and
  unbound in the demo until WS6 (documented at the `FlyToToggle` arm).
- `tick_cosmic` parks thrust on non-demo screens; dt floor 1e-6 keeps
  the flight `dt > 0` assert green on frame one.
- One master seed drives stage 0 everywhere: `R` (demo), seed-field
  loads, and `--seed` boot all funnel through `reseed_cosmic`.
- `depth_fraction_override` (test/dev plumbing, no UI) pins the
  compressed regime in tests — Mpc units cannot resolve real-time
  thrust in test-length runs (correct near-node behavior).
- HUD target line renders engine meters (WS6 follow-up: Mpc formatting
  needs a `game::hud` change — out of scope here).

- R-1 (calibration risk, WS1): hitting all statistical bands with one
  (D₊, λ_th) nominal set may need 1–2 tuning rounds — mitigated by
  tolerance bands (not exact values) and by decoupling visual glow
  density from classification thresholds. TECHLEAD cut option: shrink
  lattice to 96³ if boot-time generation exceeds the cold-start budget.
- R-2 (untested frame, WS2): `engine::flight` has never run above the
  solar-system frame — mitigated by fixed-step unit tests first
  (CSP-006/007 before any windowed wiring).
- R-3 (contract surface, WS3/WS4): new viewport + marker + camera paths —
  mitigated by mirroring the existing pinned patterns (`MapOrbitCamera`
  near/far policy, `world_to_pixels` marker) and pinning each with a
  named test.
- R-4 (context-key split, WS4): `E` meaning differs demo-vs-tabs —
  mitigated by the existing `T` dual-binding precedent + registry
  documentation + headless asserts per surface.
- R-5 (v0.3.0 `dimension-debug` still `in-progress`): this feature
  cross-links but does not close it — PO reconciles its status
  separately (recorded in notion Constraints).
- Next steps after `done`: frame handoff into the home galaxy
  (Galactocentric), `RegionId` Cosmological streaming, sheet rendering,
  release-shell gating of debug UI.
