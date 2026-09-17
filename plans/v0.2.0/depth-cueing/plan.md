# Plan - depth-cueing

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 - Regime keying and cue state

Owning module: new pure `engine::render::cue` module. Keep the kernel
headless-testable and keep Vulkan plumbing in the renderer-owned binaries.

- Add a `CueRegime` enum and map the existing `FrameId` chain to the five
  physical cue regimes in spec section 9.1.
- Key the model by `FrameId` plus caller-supplied intra-frame position. No
  `Waypoint` runtime exists yet; `waypoint-transitions` owns that wiring.
- Add cue-state replacement semantics so an active-frame change cannot retain
  the previous regime's state.
- Invariant: this phase does not touch camera, projection, picking, ENU, or
  winding/cull code.

### Phase 2 - Vacuum extinction and color shifts

- Add a seeded dust-column source using
  `region_seed(master, region).layer("dust_column")`; raw region seeds never
  cross the layer boundary.
- Convert column density to `E(B-V)` and `A_v` using the standard `R_v = 3.1`
  relationship, with validated parameters and monotonic attenuation.
- Add wavelength-dependent extinction/reddening for the interstellar and
  local-intergalactic regimes.
- Add peculiar-velocity Doppler tinting and gate Hubble-flow redshift to the
  cosmic-web regime only. Pin a local blueshift case to ensure cosmological
  redshift is not used as a local intergalactic cue.
- Add determinism tests: identical master seed, region, and position produce
  identical dust and density results across repeated runs.

### Phase 3 - Atmospheric cue model and ADR-003

- Add analytic Rayleigh and Mie scattering primitives for atmospheric regimes
  7-10, with altitude and view-angle inputs supplied by the caller.
- Add a reduced-cost mobile fallback with the same output contract and no
  change to the physical cue direction.
- Draft ADR-003 with the analytic atmosphere choice, fallback behavior, valid
  altitude range, and calibration boundaries.
- Keep atmosphere source terms in linear radiance before exposure and tone
  mapping.

### Phase 4 - Cosmic-web density and tier budget

- Add deterministic seeded filament and dust-node density fields using
  domain-separated layers under ADR-019.
- Add a raymarch contract for filament luminosity falloff and dust-node glow.
  The pure density and step calculations remain headless-testable.
- Add the GPU proof in `game_tools` first; live-view transition wiring remains
  owned by `waypoint-transitions`.
- Resolve the open density-resolution question with a tier-keyed resolution
  and step-count table. Add a `quality.md` budget row and tier-gate the Low
  path; use the analytic/mobile fallback if the Low budget is exceeded.

### Phase 5 - Composition, evidence, and lifecycle

- Compose each regime's source term through one cue interface before the
  existing exposure/tone-mapping stage.
- Consume `ZodiacalSource` for the interplanetary regime without duplicating
  the zodiacal model or changing its interface.
- Add a headless active-frame switch harness that asserts cue replacement and
  absence of leftover state.
- Add pinned per-regime analytic checks and reference-capture procedures.
- Update ADR-003, ADR-021's landing note, `rendering.md`, the decisions index,
  milestones, and the techstack version only when the feature reaches `done`.

## Todo

| ID | Status | Task | Ref notion section |
|----|--------|------|--------------------|
| DCU-001 | done | Define `CueRegime`, `regime_for_frame(FrameId)`, and clean cue-state replacement with frame-switch tests | Functional requirements; DoD 2 |
| DCU-002 | done | Implement seeded dust-column generation with the `dust_column` domain and repeated-seed determinism tests | Functional requirements; Constraints & Assumptions |
| DCU-003 | done | Implement `E(B-V)`, `A_v`, wavelength attenuation, reddening, and monotonicity pins | Goals 2; Functional requirements |
| DCU-004 | done | Implement peculiar-velocity Doppler tint and cosmic-web-only Hubble redshift, including a local blueshift pin | Goals 3-4; Functional requirements |
| DCU-005 | done | Implement analytic Rayleigh/Mie scattering, mobile fallback, and the ADR-003 draft | Goals 5; DoD 3 |
| DCU-006 | done | Implement seeded filament/node density fields and tier-keyed raymarch calculations | Goals 4; Open questions |
| DCU-007 | done | Add the cue composition interface, linear-radiance contract, and `ZodiacalSource` consumer | Non-goals; Constraints & Assumptions |
| DCU-008 | done | Add `game_tools` tier proof, Low-tier fallback, and the `quality.md` volumetric budget row | Non-functional requirements; Open questions |
| DCU-009 | done | Add active-frame switch self-test and per-regime pinned evidence/reference-capture procedure | DoD 1-2 |
| DCU-010 | done | Complete ADR-003, rendering docs, decisions index, lifecycle metadata, and all quality gates | DoD 3; Non-functional requirements |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`,
`cargo test --doc --workspace`, `cargo run --bin game`,
`cargo run -p game_debug -- --headless`, and
`cargo run -p game_tools -- --headless --tier low`.
Mobile compile guards apply when the raymarch path is added:
`cargo check --workspace --target aarch64-linux-android` and
`cargo check --workspace --target aarch64-apple-ios`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 - new pure cue kernel,
seed ownership remains in `engine::seeding`, renderer-owned GPU proof, legal
dependency direction, ADR-003 explicitly required, and camera/projection/
picking/ENU/winding invariants listed)_ · Todos approved by: TECHLEAD _(signed
2026-09-17 - risk-first order puts regime contracts, determinism, and
atmosphere decisions before raymarch plumbing; Low-tier resolution is
tier-gated; every todo has a notion reference and verifiable outcome)_ . UX
acceptance rows: _(signed 2026-09-17 - passive visual layer adds no controls;
calibration is tunable with readable defaults; physical cue direction is never
inverted; each regime requires reference evidence)_ · DoD verified by: ANALYST
_(signed 2026-09-17 - 8 cue unit tests, full workspace tests, tools
self-test, clippy, build, doc tests, and both headless smoke paths green;
DoD-1 maps to regime/source tests and the headless reference line; DoD-2 maps
to the active-frame switch test and tools line; DoD-3 maps to binding ADR-003)_
· Security reviewed by: SECURITY _(signed 2026-09-17 - no unsafe, I/O,
serialization, network, or new dependency surface; seed inputs are pure and
non-finite values sanitize to bounded source terms; no findings)_ .

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Each regime table row has an implemented cue and reference capture | done | `render::cue` regime/source tests plus `depth_cue=... radiance=0.3101 steps=16 pass=true` headless reference line | ANALYST _(signed 2026-09-17)_ |
| 2 | Regime switch test swaps cue models with no leftover state | done | `frame_switch_replaces_regime_without_leftover_state` and tools active-frame self-test | ANALYST _(signed 2026-09-17)_ |
| 3 | ADR-003 is updated/closed by the atmosphere rows | done | Binding `docs/decisions/ADR-003.md`, decisions index checkbox, and atmosphere/fallback tests | ANALYST _(signed 2026-09-17)_ |

## Acceptance criteria

- `engine::render::cue` tests cover regime mapping, parameter validation,
  seeded determinism, monotonic extinction, local peculiar-velocity tint,
  cosmic-web redshift gating, atmosphere fallback parity, and cue-state
  replacement.
- The active-frame switch harness exits successfully and proves that a
  previous regime's state is not used after a switch.
- Every regime has pinned analytic evidence and a documented reference
  capture path; no cue uses atmospheric haze in vacuum.
- All source terms remain linear before the existing exposure and tone-mapping
  stages, including the `ZodiacalSource` path.
- The raymarch path is tier-keyed and stays within the Low-tier budget; the
  fallback preserves the cue contract when the budget or format is unavailable.
- No camera, projection, picking, ENU, scene winding, or scene culling
  invariant changes are introduced.
- UX acceptance: no new input or UI surface is required; calibration remains
  tunable; visual exaggeration never reverses physical cue direction; captures
  demonstrate readable depth in all regimes.
- All commands listed above pass, including public API doc tests and available
  mobile compile guards.

## Risks & Next steps

- Volumetric raymarch cost may exceed the Low-tier budget. Mitigation: resolve
  step counts and resolution before GPU integration, tier-gate aggressively,
  and retain the analytic/mobile fallback.
- No waypoint runtime exists yet. Mitigation: keep the engine contract on
  `FrameId` plus caller-supplied position; defer transition wiring to
  `waypoint-transitions`.
- Atmosphere parameter choices can become stylistic rather than physical.
  Mitigation: ADR-003 records the physical basis and limits exaggeration to
  calibration parameters.
- The zodiacal source is not reliable inside its documented near-Sun cone.
  Consumers must preserve the existing handoff constraint from
  `zodiacal-light`.
- Next feature after this lands: `waypoint-transitions`, which composes these
  cue primitives with exposure, zodiacal light, navigation, and debug events.
