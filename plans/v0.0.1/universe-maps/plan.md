# Plan — universe-maps

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — `engine::universe` descriptors (stages 1–2)

New module `engine::universe` (per
[`architecture.md`](../../../docs/techstack/architecture.md) layout: "seeds,
galaxy/system/planet generation"). Pure functions `(seed, version, id) →
descriptor`; no wall-clock, no thread-order-dependent output. Quantized
outputs up front ([`risks/`](../../../docs/risks/README.md) #2 activates
here — risk-first). `GalaxyDescriptor` (stars: f64 log-compressed
position, spectral class, companion count; FR-1), `SystemDescriptor`
(planet orbits at AU-scale compressed radii, patched positions; FR-2),
`PlanetDescriptor` (type subset, 2–8 km radius band, `SeededPlanet`
mesh-seed binding params, atmosphere color/density, resource bias,
visual-only companions; FR-3). Stable content IDs
`galaxy/seed:…/system:…/planet:…` + `universe_version` on every
descriptor. Descriptor hash + determinism tests.

Modules: `engine::universe` (new), `engine::core` (seeded RNG, quantized
math helpers — reused, not duplicated). Dependency direction: engine-only,
no `game`/`debug` imports.

Invariants touched: **`engine::universe` purity + determinism**
(architecture rule: same seed + version → byte-identical descriptors) —
committed hash tests from day one. **f64 map space / f32 rendering**
(journey rule) — descriptors carry f64; no rendering code in this phase.

### Phase 2 — Journey state machine top segment (`game` lib)

States `GalaxyMap` (L2, L1 cosmic-web backdrop flag) / `SystemMap` (L3,
with L4 planet focus as a zoom mode) / `Orbit` (L5 arrival); explicit
enter/exit hooks; selection events drive transitions; layer swaps emit
fade requests (no hard loading screens). `Orbit` never assumes it is the
root — M2 appends `Descent` below. Scripted fixed-step regression
traversal `GalaxyMap → SystemMap → Orbit → back` with expected state
hash. Transit timing model (OQ-6: fixed durations per hop class) lands
in the machine here; the action itself wires in Phase 4.

Modules: `game` lib only (headless-testable, state hashable per
[`simulation.md`](../../../docs/techstack/simulation.md)). Consumes
`engine::universe` descriptors.

Invariants touched: **`game` contains no `vulkano` pipeline code**
(architecture rule) — the machine is pure state + events; rendering
hooks are requests, not GPU calls. **One active high-detail layer at a
time** (journey rule) — the machine owns the active-layer pointer.

### Phase 3 — Map views (`game_debug`)

`GalaxyMap` screen: star points + nebula impostors from descriptors
(OQ-2 resolved by ARCHITECT: **billboard-quad impostors** — descriptor
seed drives per-impostor sprite params; no baked backdrop texture, keeps
assets procedural per scope.md "procedural > downloaded bulk"); L1
cosmic-web backdrop (OQ-5: **generated sprite field** — same impostor
path, parallax-far layer, no new shader family); f64 map coords → f32
camera-relative draw; log zoom / pan / click-select with full-range zoom
without jitter. `SystemMap` screen: central star + orbit rings + planets
at patched positions; planet focus zoom mode (L4); select → focus →
travel offer. Runtime seed plumbing (OQ-4: `--seed` CLI flag plus viewer
panel input, matching today's viewer convention). Fade + spinner
fallback surface; a map-data miss never hard-crashes. Star count
proposal vs Low-tier point budget (OQ-1) gates the draw code — see
UMAP-010.

Modules: `game_debug` lib + binary only. Consumes `engine::render`
(camera, tiers) and `engine::universe`.

Invariants touched: **rendering camera & screen-space conventions**
([`rendering.md`](../../../docs/techstack/rendering.md)) — map cameras use
the un-flipped ortho/perspective path; `FrontFace::CounterClockwise` +
`CullMode::Back` unchanged; picking uses `ndc = (2u−1, 1−2v)`; pinned
camera/projection tests must stay green. **f32 camera-relative origin
for all rendering** (journey rule) — f64 exists only in map logic.
**Debug screens never leak into the release binary** (architecture
rule) — `game` bin untouched by this phase.

### Phase 4 — Travel + orbit arrival

Timed interplanetary transit action in the `game` lib machine
(durations per hop class from Phase 2), cancellable before commit; cost
shown as deferred/hook — no real deduction (M4 owns the resource
model). Arrival binds the existing `OrbitCamera` to the target planet:
`PlanetDescriptor` mesh-seed params feed `engine::render::SeededPlanet`,
atmosphere color/density drive the orbit backdrop. ≥2 planet types
visually distinct in focus/orbit views, descriptor-driven (OQ-3 subset
chosen with UX during this phase; palettes/atmosphere variety axes per
[`universe.md`](../../../docs/game/universe.md)); no hardcoded
special-casing. End-to-end flow in the windowed viewer: pick star →
system → planet → timed transit → orbit arrival, every layer swap
faded.

Modules: `game` lib (transit state), `game_debug` (shell + visuals),
`engine::render` (consumed, unchanged).

Invariants touched: **`OrbitCamera::projection_matrix` stays glam
`directx::perspective` (RH, Z ∈ [0, 1], no Y-flip)** — arrival rebinds
the camera, it does not re-create projection code. **One active
high-detail layer** — transit fades enforce the XOR between map layer
and orbit layer.

### Phase 5 — Gates + docs sync

Full [`quality.md`](../../../docs/techstack/quality.md) gate list + mobile
compile-guards; pinned camera/projection tests re-run. Docs: journey.md
implementation annotation for L1–L4 (states + views now exist),
architecture.md `engine::universe` module status line, techstack README
version bump, every touched link resolves.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UMAP-001 | done | `engine::universe` module skeleton: descriptor types, `universe_version`, content-ID type (`galaxy/seed:…/system:…/planet:…`) | FR-3, NFR determinism |
| UMAP-002 | done | Seeded quantized RNG/math helpers in `engine::core` (no wall-clock; quantized outputs per risks #2) | NFR determinism, Constraints |
| UMAP-003 | done | `GalaxyDescriptor` gen: star list, f64 log-compressed positions, spectral class, companion count; 10k–100k ly compressed extent | FR-1 |
| UMAP-004 | done | `SystemDescriptor` gen: planet orbit list, AU-scale compressed radii, patched positions | FR-2 |
| UMAP-005 | done (landed inside UMAP-004: per-planet content is built by `generate_system`; no separate unit exists) | `PlanetDescriptor` gen: type subset (placeholder pair, finalized UMAP-021), 2–8 km radius band, `SeededPlanet` binding params, atmosphere color/density, resource bias, visual-only companions | FR-3 |
| UMAP-006 | done | Descriptor hash + determinism tests: same seed + version → identical hashes across N repeated runs on x86-64 | FR-8, DoD 1 |
| UMAP-007 | done | Journey machine: `GalaxyMap`/`SystemMap`/`Orbit` states + enter/exit hooks; L1 backdrop flag on `GalaxyMap`; L4 planet focus as SystemMap zoom mode; `Orbit` never root | FR-4 |
| UMAP-008 | done | Selection events → transitions; fade requests on every layer swap (no loading screens) | FR-4 |
| UMAP-009 | done | Fixed-step scripted traversal `GalaxyMap → SystemMap → Orbit → back` regression with expected state hash | FR-8, DoD 2 |
| UMAP-010 | done (25,000 stars; headroom math on `DEFAULT_STAR_COUNT`) | OQ-1: v1 star-count proposal vs Low-tier point/draw budget (TECHLEAD approves the number; tier-gate if blown) | FR-5, NFR perf |
| UMAP-011 | done | GalaxyMap draw: star points + billboard nebula impostors (OQ-2) + L1 generated sprite-field backdrop (OQ-5) | FR-5 |
| UMAP-012 | done | GalaxyMap camera: f64 log-compressed map coords → f32 camera-relative; log zoom/pan full-range without jitter or pop | FR-5, NFR precision, DoD 3 |
| UMAP-013 | done | GalaxyMap picking: click star select via `ndc = (2u−1, 1−2v)` | FR-5, DoD 3 |
| UMAP-014 | done | SystemMap draw: central star + orbit rings + planets at patched positions | FR-6 |
| UMAP-015 | done | SystemMap planet focus zoom mode (L4): select planet → focus → travel offer | FR-6, FR-4 |
| UMAP-016 | done | Runtime seed plumbing: `--seed` CLI flag + viewer panel input (OQ-4) | OQ-4, Users/Stakeholders |
| UMAP-017 | done | Fade + spinner fallback; map-data miss surfaces, never hard-crashes; photosensitivity-safe fades | Constraints, FR-4 |
| UMAP-018 | done | Timed transit action between systems/planets (fixed durations per hop class, OQ-6), cancellable before commit | FR-7, DoD 6 |
| UMAP-019 | done | Transit cost shown as deferred/hook (no real deduction; M4 owns the model) | FR-7, Non-goals |
| UMAP-020 | done | Orbit arrival: bind `OrbitCamera` to target; descriptor mesh-seed params → `SeededPlanet`; atmosphere color/density → orbit backdrop | FR-7, FR-3, DoD 4 |
| UMAP-021 | done (OQ-3: Rocky + Ice showcased; all six generatable, tint descriptor-driven) | ≥2 planet types visually distinct in focus/orbit views, descriptor-driven; OQ-3 subset + palettes chosen with UX; no hardcoded special-casing | Goal 4, DoD 5, OQ-3 |
| UMAP-022 | done (incl. both mobile compile-guards) | Full quality gates + mobile compile-guards; pinned camera/projection tests green | NFR gates, DoD 7 |
| UMAP-023 | done | Docs sync: journey.md L1–L4 annotation, architecture.md `engine::universe` line, techstack README version bump, links resolve | DoD 8 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-16) · Todos approved by:
TECHLEAD (2026-09-16) · UX acceptance rows (if player-facing): n-a
(debug-hosted UI in `game_debug`; player-facing UX acceptance lands with
the release UI) · DoD verified by: ANALYST (2026-09-17 — all 8 rows
reproduced, see DoD table) · Security reviewed by: SECURITY (2026-09-17 —
pass, no findings: pure `engine::universe`, no `vulkano` in `game`,
guarded seed parsing, no new deps, no unsafe, no file I/O).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Descriptor determinism: same seed + version → byte-identical hashes across repeated x86-64 runs; quantized per risks #2; ARM runtime check at M6/first device | done | ANALYST reproduced 2026-09-17: `universe::hash::tests::repeated_runs_replay_identical_hashes`, `committed_vectors_pin_stage1_and_stage2`, `same_triple_replays_identical_descriptor`, `same_inputs_replay_identical_system`, `ulp_perturbation_cannot_flip_a_hash` (quantization), `version_stamp_participates` — all green; ARM check stays deferred to M6 per DoD text | ANALYST (2026-09-17) |
| 2 | Journey regression: scripted `GalaxyMap → SystemMap → Orbit → back` yields expected state hash | done | `journey::tests::scripted_traversal_replays_pinned_hashes` green + `cargo run --bin game` 2026-09-17 shows the full traversal with pinned hashes (`EnterOrbit → Orbit`, `Ascend → Galaxy`) | ANALYST (2026-09-17) |
| 3 | Windowed galaxy map: full-extent zoom without jitter/pop; click star select | done | Headless map path reproduced: `galaxy_seed=1234 galaxy_stars=25000`, `pick_selftest=star0 ok`; `select_at_picks_the_projected_star`, `select_at_honors_pixel_threshold` green; windowed visual (no-jitter zoom) accepted by PO (close instruction 2026-09-17) | ANALYST (2026-09-17) |
| 4 | End-to-end travel: star → system → planet → orbit arrival, faded swaps, no loading screen | done | Headless: `system_star=0 planets=5 system_hash=29393b57504e021d focus_toggle=ok journey=System ok`, `transit_ticks=60 fuel=5.1 energy=2.5 ok`; windowed travel-flow visual accepted by PO (close instruction 2026-09-17) | ANALYST (2026-09-17) |
| 5 | Two planet types visually distinct in focus/orbit views, descriptor-driven | done | `stage2_tests::all_six_types_are_generatable` green (Rocky + Ice showcased per UMAP-021); visual distinctness accepted by PO (close instruction 2026-09-17) | ANALYST (2026-09-17) |
| 6 | Timed transit with pre-commit cancellation; cost shown as deferred/hook | done | `transit_ticks=60 … ok` in headless run; `commits_need_armed_selections` journey test green; cancellation path unit-covered; windowed cancel visual accepted by PO (close instruction 2026-09-17) | ANALYST (2026-09-17) |
| 7 | Rendering invariants intact: pinned camera/projection tests green | done | `map_camera::tests::projection_uses_unflipped_perspective`, all camera/convention pins green in the 259-test workspace run 2026-09-17; `crates/game/Cargo.toml` has no `vulkano` (verified) | ANALYST (2026-09-17) |
| 8 | Docs synced: journey.md L1–L4 annotation, architecture.md universe module, techstack README bump; links resolve | done | `journey.md` L2/L3 + drill-down annotations present, `architecture.md` `engine::universe` line present, version bumped (0.13.x line); all links resolve (123-file check 2026-09-17, 0 broken) | ANALYST (2026-09-17) |

## Acceptance criteria

- `cargo test -p game_engine` runs the descriptor determinism suite
  (UMAP-006) green; hashes committed per `universe_version`.
- `cargo run --bin game` stays a clean headless gate (no `vulkano` in
  `game`); the scripted traversal regression (UMAP-009) passes there.
- `cargo run -p game_debug -- --headless` extended with map self-tests;
  windowed run demonstrates DoD 3–6.
- Invariant rows: projection un-flipped + `FrontFace::CounterClockwise`
  + `CullMode::Back` everywhere new; picking only via
  `ndc = (2u−1, 1−2v)`; f64 confined to map logic, all draws f32
  camera-relative; one active high-detail layer at a time; debug UI
  absent from the release binary.
- Gate list for DEV (from [`quality.md`](../../../docs/techstack/quality.md)):
  `cargo fmt --check` · `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` · `cargo build --workspace` ·
  `cargo test --workspace --all-targets` · `cargo test --doc
  --workspace` · `cargo run --bin game` · `cargo run -p game_debug --
  --headless` · `cargo run -p game_tools -- --headless --tier low` ·
  mobile compile-guards (`aarch64-linux-android`, `aarch64-apple-ios`).

## Risks & Next steps

- **Risk #2 (bit-stable noise ARM/x86)** — activates with Phase 1;
  mitigated by quantized outputs in UMAP-002 before any generation code;
  ARM runtime hash check deferred to M6 per the reworded DoD 1.
- **f64→f32 jitter on full-range galaxy zoom** — the riskiest visual
  item (DoD 3); UMAP-012 lands before any polish; if log-compression +
  camera-relative is insufficient at extreme zoom, escalate to
  ARCHITECT (never flip the projection to compensate).
- **Low-tier budget (OQ-1 star count)** — UMAP-010 proposes the number
  before UMAP-011 draws anything; points/impostors are cheap by
  construction, but the count is tier-gated if the proposal blows Low.
- **OQ-3 open until Phase 4** — `PlanetDescriptor` ships with a
  placeholder type pair; the UX-consulted subset + palettes finalize in
  UMAP-021. DoD 5 cannot verify before that.
- **External assumption (notion Constraints):** `debug-ui-reorganize`
  `done` + gnomonic-checker issue closed before this feature's
  `in-review` gate — both are in flight in parallel, neither blocks
  implementation start.
- **Next milestone after `done`:** M2 descent slice appends `Descent`
  below `Orbit` (the machine already guarantees `Orbit` is never root);
  landing-site selection consumes stage-3 surface data there.
