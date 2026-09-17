# Plan — exposure-tone-mapping

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Exposure kernel, pure math (`engine::render::exposure`)

Owning module: new `engine::render::exposure` (pure f64 math,
headless-testable, no GPU — same shape as `render::zodiacal` and
`render::depth`). No `unsafe`, no I/O, no input surfaces.

- `DominantSource { Sun, PlanetAlbedo, Starlight }` + `select_source(key)`
  with a hysteresis band (separate enter/exit thresholds) so the
  waypoint 6 → 5 hero-source handoff cannot oscillate.
- `ExposureParams` (key value, hysteresis band, adaptation time
  constants — dark slower than light, mirroring scotopic behavior, per
  UX notes) + validated `new()`.
- `aces_approx(x)`: ACES-fitted curve (Narkowicz fit) with pinned
  reference values; never-clip property asserted on synthetic HDR
  scenes (DoD-3).
- `star_visibility(sky_luminance)`: scotopic fade-in curve — gradual
  across civil → nautical → astronomical twilight stages, no step at a
  fixed altitude (DoD-2).
- `PHOTOMETRIC_ZERO_POINT`: the absolute calibration constant that
  `zodiacal.rs::radiance_relative` deliberately left to this feature;
  documented with its reference basis.
- Invariant row: pure functions over caller-supplied scalars; no
  camera/projection/picking code, no rendering invariant at risk.

### Phase 2 — HDR scene target + resolve pass (GPU plumbing)

Owning modules: `engine::render` pipeline helpers + `game_tools`
first (renderer smoke binary), `game_debug` Planet View second.
Dependency direction stays legal (`game` untouched; no vulkano outside
`render` + the two binaries).

- Offscreen HDR color attachment `R16G16B16A16_SFLOAT` with a runtime
  format-support query; tier fallback (`B10G11R11_UFLOAT_PACK32` where
  supported, LDR bypass on Low if unsupported) — this resolves the
  notion's open question (ACES variant vs mobile cost, ADR-007).
- Scene renders into the HDR attachment; a fullscreen resolve pass
  writes to the swapchain. Fixed exposure first (tone mapping lands in
  Phase 3) so plumbing is verified independently.
- `game_debug`: backdrop/sky/planet stay the scene pass; the UI quad
  band splits into a post-resolve LDR pass (UI must never be tone
  mapped). Tools binary leads; debug follows in the same feature.
- Invariant rows: resolve pass uses a fullscreen triangle with culling
  disabled — winding/cull state (`FrontFace::CounterClockwise` +
  `CullMode::Back`) of the scene passes is untouched; projection stays
  un-flipped (no Y-flip in resolve UV mapping; NDC +1 = top row
  preserved end to end).
- Budget row: resolve adds 1 fullscreen pass + 1 transient HDR image;
  `docs/techstack/quality.md` gains a post-chain budget row
  (TECHLEAD decision, recorded in Phase 5).

### Phase 3 — Tone mapping + photometric calibration in the resolve shader

- ACES fit from Phase 1 composed into the resolve fragment shader by
  string composition (same pattern as
  `planet_vert_logdepth(PLANET_VERT)` — never hand-duplicated GLSL).
- `ZodiacalSource::radiance` output scaled by the Phase-1 zero point:
  first consumer of the trait, proving the interface the
  `zodiacal-light` feature left behind.
- Never-clip unit tests on synthetic HDR scenes (DoD-3); validation
  captures via the headless harness (Phase 5).

### Phase 4 — Auto-exposure + dark-adaptation loop

- Per-frame key computation: analytic dominant-source model driven by
  the active frame's light environment (Sun angular diameter /
  illuminance at camera, planet albedo term, ambient starfield floor).
  Contract boundary (documented, not invented): no Sun position,
  planet albedo field, or `Waypoint` runtime exists in code yet — the
  loop consumes caller-supplied key inputs; `waypoint-transitions`
  owns the real frame→light wiring later. The analytic harness
  (Phase 5) is the caller for DoD-1.
- Hysteresis switching + adaptation lerp with separate up/down time
  constants; no visible exposure step at switches (hardest: 6 → 5).
- Star-visibility multiplier from Phase 1 fed into the sky-point alpha
  path — this calibrates the presentation-only `stars.rs` photometry
  (`spectral_color`, `mag_to_size_px`) the catalog feature deferred.

### Phase 5 — Evidence harness + docs + lifecycle

- `game_tools --headless` gains an exposure self-test line: analytic
  Sun angular-diameter sweep across the 6 → 5 regime asserting exposure
  continuity and no pops (DoD-1, approved evidence form 2026-09-17);
  twilight luminance sweep asserting progressive star fade-in
  (DoD-2); synthetic HDR scenes asserting no clipping (DoD-3).
- Debug-viewer captures for the twilight sequence (manual screenshots,
  house style).
- `docs/techstack/rendering.md`: post-chain section (replaces the
  "Post: tone map is a v1 plan item" note); ADR-021 status note updated
  (exposure section landed); milestones v0.2.0 table
  `exposure-tone-mapping` → `done`; `docs/techstack/README.md` version
  bump (0.23.0 → 0.24.0); `quality.md` post-chain budget row.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| ETM-001 | done | `engine::render::exposure`: `DominantSource` + hysteresis selection + `ExposureParams` with validated `new()` | Functional requirements |
| ETM-002 | done | `aces_approx` + pinned reference values + never-clip tests on synthetic HDR scenes | Goals 2, DoD 3 |
| ETM-003 | done | `star_visibility(sky_luminance)` twilight-stage curve + no-step tests | Goals 3, DoD 2 |
| ETM-004 | done | `PHOTOMETRIC_ZERO_POINT` constant + zero-point doc (consumes `ZodiacalSource` contract) | Constraints & Assumptions |
| ETM-005 | done | HDR scene target (`R16G16B16A16_SFLOAT` + runtime query + tier fallback) + resolve pass in `game_tools`, fixed exposure | Goals 2, Open questions |
| ETM-006 | done | `game_debug` exposure application WITHOUT pass split: `MapPush.exposure` multiplier on the map pipeline (stays 1.0 outside PlanetView twilight control) + F5 cycles twilight stages for DoD-2 captures. TECHLEAD scope decision 2026-09-17: the full debug HDR split (3 windows × framebuffers + resolve passes in a 5.5k-line file) is deferred to `waypoint-transitions`, which owns the debug transitions panel; all three DoDs are reachable without it (tools `--hdr` proves the chain, viewer proves the behavior) | Goals 3 |
| ETM-007 | done | ACES in resolve shader via string composition; winding/cull/projection invariants asserted | Goals 2, NFR |
| ETM-008 | done | Auto-exposure loop: frame key inputs + hysteresis switching + adaptation lerp (no visible step) | Functional requirements, NFR |
| ETM-009 | done | Star-visibility multiplier into sky-point alpha path (calibrates `stars.rs` photometry) | Goals 3 |
| ETM-010 | done | Headless exposure self-test: 6 → 5 angular-diameter sweep (continuity, no pops) | DoD 1 |
| ETM-011 | done | Headless twilight sweep + never-clip scenes; debug-viewer captures | DoD 2, DoD 3 |
| ETM-012 | done | Docs: rendering.md post-chain, ADR-021 note, milestones → `done`, version 0.24.0, quality.md budget row; full gate list green | NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`,
`cargo run -p game_tools -- --headless --tier low`.
Mobile compile-guards (`cargo check --workspace --target aarch64-linux-android`,
`aarch64-apple-ios`) apply to the HDR format path.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new pure
module `engine::render::exposure` + resolve-pass plumbing confined to
`render` and the two existing binaries; dependency direction legal
(`game` untouched, no vulkano outside `render`/binaries); no locked
choice changed (ADR-021 mandates auto-exposure + filmic + adaptation;
ADR-007 tiers respected via the runtime format query + fallback); no
new ADR (HDR target is implementation detail under ADR-021);
invariants listed explicitly (winding/cull/projection rows in Phase 2);
foundations before surfaces (kernel → plumbing → mapper → loop))_ ·
Todos approved by: TECHLEAD _(signed 2026-09-17 — todos risk-first
(kernel pins before GPU plumbing, tools binary before debug split),
each independently verifiable with a named test or headless line; the
resolve pass budget risk recorded with its quality.md row; gate list
linked above)_ · UX acceptance rows: _(signed 2026-09-17 — adaptation
pacing constants tunable; dark adapts slower than light; star fade-in
gradual across civil/nautical/astronomical twilight with no altitude
pop; no visible exposure step at dominant-source switches; UI never
tone mapped (stays post-resolve LDR))_ · DoD verified by: ANALYST
_(signed 2026-09-17 — every row re-checked, gate suite re-run green on
this tree (fmt, clippy -D warnings, build, workspace tests 420+, doc
tests, `cargo run --bin game`, both `--headless` smokes, mobile
compile-guards aarch64-android + aarch64-ios); DoD-1 =
`exposure_handoff=switches=2 final=Starlight max_step=0.0101 pass=true`
(exit 0, gate-worthy) + engine tests
`selection_switches_exactly_once_across_handoff_sweep` /
`loop_handoff_sweep_is_continuous_with_no_pops`; DoD-2 =
`twilight_fade=day=0.00 civil=0.20 nautical=0.88 astro=1.00
monotonic=true pass=true` + `twilight_fade_is_gradual_with_no_step` +
F5 stage path in the viewer (viewer runs clean); DoD-3 =
`tonemap_clip=max_out=1.0000 scenes=3 pass=true` +
`aces_never_clips_and_rises_monotonically`; headless pins unchanged
(`a0175dd0c40690a8`, `7b49a5870d50b1a0`, `29393b57504e021d`); ugly
paths pinned (NaN/Inf sanitization, degenerate-params rejection,
HDR-incapable LDR bypass, `--hdr`+`--headless` arg rejection); E2E
journey explicitly out of scope (renderer-only feature — the headless
harness is its e2e form))_ · Security reviewed by: SECURITY _(signed
2026-09-17 — no new untrusted-byte surface (no save/asset/network
parsing; shader sources are in-process constants); input surface = one
validated CLI flag (`--hdr`) + two keys (H, F5) in dev binaries; no new
dependency (Cargo.toml/Cargo.lock untouched — vulkano/naga already in
tree); new `unsafe` follows the existing documented-invariant pattern
(naga-validated SPIR-V + matching layouts, comments in place); corrupt
input handled (NaN/Inf pinned tests, params validation, format query
errors → clean bypass, no panic); GPU validation error found in dev
(VUID-02097, empty vs dynamic vertex input) fixed and re-verified on
real GPU; no findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Sun-disk → Sun-as-point (6 → 5) smooth exposure handoff | done | `exposure_handoff=switches=2 final=Starlight max_step=0.0101 pass=true` (headless, exit 0) + engine loop tests | ANALYST _(signed 2026-09-17)_ |
| 2 | Twilight sequence: progressive star fade-in, no altitude pop | done | `twilight_fade=day=0.00 civil=0.20 nautical=0.88 astro=1.00 monotonic=true pass=true` + `twilight_fade_is_gradual_with_no_step` + F5 stage captures path (viewer) | ANALYST _(signed 2026-09-17)_ |
| 3 | Tone mapper never clips test HDR scenes | done | `tonemap_clip=max_out=1.0000 scenes=3 pass=true` + `aces_never_clips_and_rises_monotonically` | ANALYST _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine render::exposure::` green: hysteresis
  selection, params validation, ACES pins, never-clip scenes,
  twilight-curve no-step, zero-point doc test. — verified 2026-09-17
  (10 unit + 8 doc tests green).
- `game_tools --headless --tier low` prints the exposure self-test
  lines; 6 → 5 sweep asserts continuity (no pops); outputs otherwise
  byte-identical apart from timing and the new lines. — verified
  2026-09-17 (`exposure_handoff`/`twilight_fade`/`tonemap_clip`, exit
  0, hash pin `a0175dd0c40690a8` unchanged).
- Resolve pass leaves winding/cull/projection invariants intact
  (asserted in Phase 2 review; AGENTS.md rendering invariants hold).
  — verified 2026-09-17 (culling disabled only on the fullscreen
  triangle; scene passes untouched; orientation contract pinned in
  `post` module docs + composition tests).
- Swapchain-absent tiers fall back without error (format query path
  covered by a headless-safe unit test, not live GPU). — verified
  2026-09-17 (`select_hdr_format` preference/fallback/bypass tests).
- Doc tests for all new public APIs (`quality.md` test policy). —
  verified 2026-09-17 (`cargo test --doc --workspace` green).

## Risks & Next steps

- Mobile HDR format support (Vulkan 1.1 floor): runtime query + tier
  fallback is the mitigation; Low-tier LDR bypass must not change the
  scene content, only its dynamic-range headroom.
- `game_debug` same-pass UI coupling: the pass split was descoped
  (TECHLEAD decision 2026-09-17, ETM-006 row) — the full debug HDR
  split moves to `waypoint-transitions`, which owns the debug
  transitions panel; the tools binary proved the chain first.
- No Sun position / albedo field / `Waypoint` runtime exists: the loop
  takes caller-supplied key inputs; inventing a sun/world model here
  would pre-empt `waypoint-transitions` (same boundary discipline as
  `zodiacal-light`'s no-binary rule).
- Zodiacal source term is untrusted inside the near-Sun cone
  (θ ≲ 15°, recorded in that feature's plan) — the harness must not
  pin behavior there.
- Next feature: `depth-cueing` (independent) or `waypoint-transitions`
  (composes this feature) — PO sequences after this lands.
