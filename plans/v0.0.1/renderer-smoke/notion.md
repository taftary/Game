# Notion — renderer-smoke

## Status

`done` (all DoD criteria checked — see `plan.md` DoD verification)

## Context

M0 is done (workspace builds green on Linux/Win/mac). `hex-sphere` is done:
`engine::hexsphere::HexSphere::generate(subdivisions, radius)` produces the
pure, deterministic base mesh for every spherical body (default N=6 → 40,962
cells, exactly 12 pentagons, committed mesh hash). `debug-screens` is done
(`game_debug` crate exists, rendering deferred to M1+). `build-foundations`
is done (profiles, toolchain pin, CI with mobile compile-guard, ADR-001/007/
008/009).

No pixel has ever been drawn: `crates/engine` has no `render` module,
`crates/tools` is an empty `fn main`, and the Vulkan 1.1 device floor
(ADR-007) plus the `vulkano` kill-switch (ADR-001) are paper only. The
`mimalloc`-by-default decision (ADR-009) is accepted but explicitly "validate
in M1". This feature is M1 from `docs/milestones/`:
**renderer smoke + tiers** — the first frame, and the harness every later
milestone enforces the `quality.md` budgets with.

## Problem & Needs

- Developers need proof that `vulkano` boots on a real window
  (Instance → Surface → Device/Queues → Swapchain), reports the physical
  device + driver, survives resize/recreation, and renders the `HexSphere`
  planet mesh — on desktop now, compilable for Android/iOS from day one.
- Every later milestone needs the Low/Medium/High quality tiers to exist as
  code (subdivision level, resolution scale, shadows), not prose, so
  features either run on Low or are tier-gated.
- The orbit camera (drag rotate, scroll zoom) needed by the M1 smoke is
  also the camera the `debug-sphere-viewer` draft is blocked on.
- ADR-009 needs its M1 validation step: `mimalloc` wired as global
  allocator behind a one-flag A/B switch.

## Goals

- New `engine::render` module (headless-testable where possible):
  - `tier.rs` — `QualityTier::{Low,Medium,High}` with per-tier
    `subdivisions` (Low 3 → 642 cells / 3,840 tris; Medium 4 → 2,562 cells /
    15,360 tris; High 6 → 40,962 cells / 245,760 tris, inside the Low
    <500k-tri budget), `resolution_scale` (0.6 / 0.85 / 1.0), `shadows`.
  - `camera.rs` — `OrbitCamera` (target, distance, yaw, pitch; drag-rotate,
    scroll-zoom, clamped range; `glam` view + Vulkan-corrected projection
    matrices). Pure, unit-tested.
  - `boot.rs` — Vulkan 1.1 floor as code: `max_api_version = V1_1`,
    `ENUMERATE_PORTABILITY` always set (Apple/MoltenVK per ADR-007),
    `khr_swapchain` device extension, discrete > integrated > virtual >
    CPU scoring, physical device + driver logged via `tracing`,
    validation layers in dev/debug builds. Pure parts unit-tested.
  - `planet.rs` — `SeededPlanet { seed, tier, mesh }`: generates the
    tier-chosen `HexSphere`, fan-triangulates dual cells to an indexed
    `(vertices, indices)` mesh (`PlanetVertex { position, normal, tint }`,
    pentagons tinted), deterministic descriptor hash (same seed+tier →
    same hash; mesh hash seed-independent per ADR-002). Pure, unit-tested.
  - `shaders.rs` — planet vertex/fragment GLSL sources + `compile_glsl_to_spirv`
    via `naga` (`glsl-in` → `spv-out`, no `shaderc`, per `stack.md`);
    headless test asserts valid SPIR-V magic + non-empty words.
- `game_tools` renderer smoke (`crates/tools`): `--tier low|medium|high`
  (default medium), `--seed`, `--radius`, `--headless`.
  - Windowed: `winit` window + `vulkano` boot via `engine::render`
    (Instance → Surface → Device → Swapchain), planet vertex/index GPU
    buffers, single-pass render pass, naga-compiled planet pipeline
    (flat-shaded, backface-culled, MVP push constants), orbit camera input,
    explicit swapchain recreation, tier + device logged at startup.
  - `--headless`: no window, no Vulkan calls (CI-safe): generates the
    tier planet, prints cells/tris/hash/generation-ms, exit 0.
- `mimalloc` wired as global allocator behind the `game_engine/mimalloc`
  feature flag (default off so the mobile compile-guard and default builds
  never touch C code); A/B is one flag flip.
- `tracing_subscriber::fmt` installed in the `game_tools` binary
  (ADR-009: binaries install the subscriber; dev/desktop `fmt` layer).
- Headless smoke added to the canonical gates (`quality.md` + CI mirrors
  it): `cargo run -p game_tools -- --headless --tier low`.

## Non-goals

- No atmosphere/sky/terrain/chunk streaming (M2 descent slice).
- No height displacement, biomes, or per-seed surface variety (later
  world-gen layers stack on `SeededPlanet::mesh`).
- No shadows/MSAA/bloom implementation — `shadows` is a tier flag only;
  effects stay tier-gated stubs.
- No `game`-binary render wiring (release entry stays clean; M2 consumes
  `engine::render`).
- No touch controls, dynamic resolution, or suspend/resume (M6).
- No on-device measurement: reference phones are named by M6 (ADR-007);
  M1 validates on desktop + guards mobile compilation.
- No `vulkano` kill-switch verdict: the trigger (ADR-001) is evaluated
  against Low-tier numbers once the M6 harness exists.

## Users / Stakeholders

- Developers (only): first rendered frame, tier enforcement point,
  unblocks `debug-sphere-viewer` (orbit camera + render module).
- Later milestones (M2–M6) consume `engine::render` (tiers, camera, boot,
  planet mesh upload) and the `tools` smoke harness.

## Functional requirements

All GPU-touching code lives in `engine::render` (called by `tools`) or
`crates/tools`; `game` contains no `vulkano` pipeline code per
`architecture.md`.

- `QualityTier`: `from_str("low"|"medium"|"high")`, `all()`,
  `subdivisions()`, `resolution_scale()`, `shadows()`; tier params are
  `const`-visible and unit-tested.
- `OrbitCamera::new(target, distance, yaw, pitch)`; `rotate(dx, dy)`,
  `zoom(delta)` (clamped to `[1.6R, 8R]`), `eye()`, `view_matrix()`,
  `projection_matrix(aspect)` (Vulkan Y-flip); unit-tested incl. clamp +
  Y-flip.
- `boot`: instance info pins `max_api_version = V1_1` + portability
  enumeration; device filter requires `khr_swapchain` + graphics-capable
  queue with presentation support; scoring prefers discrete GPUs;
  selected adapter/driver/api version emitted via `tracing::info!`.
- `SeededPlanet::generate(seed, tier, radius)`: tier subdivision mesh,
  `triangle_count() == 6 * hexagons + 12 * 5`, `to_indexed_mesh()`
  vertex count == cells + corners, normals unit-length, pentagon tint ==
  1.0 exactly on the 12 pentagons; `descriptor_hash()` deterministic.
- `compile_glsl_to_spirv(ShaderStage, src)`: naga GLSL→SPIR-V; both planet
  shaders compile; invalid GLSL returns `Err`, never panics.
- `game_tools` smoke: parses `--tier/--seed/--radius/--headless`
  (hand-rolled argv, no new CLI deps); `--headless` prints
  `tier subdiv cells tris hash gen_ms` and exits 0 without creating an
  EventLoop or touching Vulkan; windowed mode opens the planet, orbits
  with mouse, recreates swapchain on resize, logs tier + device.
- Feature `mimalloc` on `game_engine` (optional dep, default off) +
  pass-through feature on `game_tools`; `--features mimalloc` flips the
  global allocator with no code change.

## Non-functional requirements

- Vulkan 1.1 floor: no instance/device call requests anything above
  Vulkan 1.1 core in the boot path; no optional extensions in
  gameplay-critical code (rendering.md rule).
- Apple path: portability enumeration always on; `khr_portability_subset`
  auto-enabled by vulkano where advertised (ADR-007 Apple note).
- Determinism preserved: `engine::hexsphere` untouched; planet
  descriptor hash stable across runs (test).
- All `quality.md` gates green (incl. the new headless-smoke gate) +
  mobile compile-guard (`aarch64-linux-android`,
  `aarch64-apple-ios`) with default features.
- `mimalloc` is default-off: default builds + CI mobile guard never build
  C code; allocator A/B numbers are recorded when the M6 harness exists.

## Definition of Done

- [ ] `engine::render` (`tier`, `camera`, `boot`, `planet`, `shaders`)
  implemented; unit tests cover tiers, camera math/clamps, device
  scoring, triangulation counts/normals/tints, descriptor determinism,
  naga compile of both planet shaders + invalid-GLSL error.
- [ ] `game_tools` smoke opens a windowed seeded planet with orbit camera
  on all three tiers (verified where a GPU exists); `--headless --tier low`
  runs GPU-free and is green in CI.
- [ ] Vulkan 1.1 floor + portability enumeration hold by code inspection
  (pinned constants, no >1.1 requests); mobile compile-guard passes.
- [ ] `mimalloc` feature flips the global allocator; default build
  unaffected; A/B method recorded in plan (numbers deferred to M6).
- [ ] `tracing` subscriber installed in `game_tools`; device + tier logged
  at smoke startup.
- [ ] Docs synced (`rendering.md` smoke section, `architecture.md` render
  module row, `stack.md` allocator line, milestones M1 marked done,
  techstack version bumped, `quality.md` + CI gate added); all touched
  links resolve; plan.md DoD verification records evidence per criterion.

## Constraints & Assumptions

- `HexSphere::generate` panics on invalid radius — smoke validates
  `--radius > 0` finite before calling (same guard pattern as
  `debug-sphere-viewer`).
- Headless smoke must never load the Vulkan loader (CI runners have no
  GPU): no `VulkanLibrary::new()` outside windowed mode — test by code
  structure + CI green.
- Backface culling (not a depth buffer) resolves sphere visibility: the
  planet is convex and dual-cell rings are CCW from outside, so culled
  rendering is correct; depth lands with M2 terrain.
- `game` release binary and `engine::hexsphere` stay untouched.
- Follows `plans/README.md` (notion defines, plan checks) and `AGENTS.md`
  docs-maintenance rules.

## Open questions

- None blocking. Deferred by design: shaded-pipeline richness (M2),
  wireframe toggle needs `fillModeNonSolid` probe (viewer feature),
  on-device Low-tier fps verdict for the ADR-001 kill-switch (M6).
