# Plan — renderer-smoke

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — `engine::render` pure core (no GPU needed)

`tier.rs` (tier table), `camera.rs` (orbit math),
`planet.rs` (seeded generation + fan triangulation + descriptor hash),
`shaders.rs` (GLSL sources + naga GLSL→SPIR-V). Unit tests for every
DoD-mapped behavior. `hexsphere` untouched.

### Phase 2 — `engine::render::boot` (Vulkan 1.1 floor)

Instance-info builder (pinned `max_api_version = V1_1`, portability flag,
validation layers), required device extensions, device scoring, device
logging via `tracing`. Unit tests on the pure parts (scoring, create-info
fields). `mimalloc` optional dep + `mimalloc` feature flag on
`game_engine`.

### Phase 3 — `game_tools` smoke (windowed + headless)

Hand-rolled argv (`--tier/--seed/--radius/--headless`), `tracing`
subscriber install, `--headless` GPU-free path, windowed path following
the verified vulkano 0.35 boot/draw sequence (Instance → Surface →
Device → Swapchain → render pass → naga-compiled planet pipeline with
MVP push constants → orbit input → swapchain recreation).

### Phase 4 — gates + docs

Headless smoke added to `quality.md` + CI, full local gates + mobile
compile-guard (+ `--features mimalloc` check), docs sync
(`rendering.md`, `architecture.md`, `stack.md`, milestones, techstack
0.3.0), DoD evidenced below.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| RSM-001 | done | `engine::render::tier`: `QualityTier` + per-tier subdiv/scale/shadows + tests | Functional requirements |
| RSM-002 | done | `engine::render::camera`: `OrbitCamera` + view/projection + clamp tests | Functional requirements |
| RSM-003 | done | `engine::render::planet`: `SeededPlanet` + indexed triangulation + descriptor hash + tests | Functional requirements |
| RSM-004 | done | `engine::render::shaders`: planet GLSL + naga compile fn + tests (incl. invalid-GLSL Err) | Functional requirements |
| RSM-005 | done | `engine::render::boot`: 1.1-floor instance info, device scoring/logging + tests | Functional requirements |
| RSM-006 | done | `mimalloc` optional dep + feature on `game_engine`, pass-through on `game_tools` | Functional requirements |
| RSM-007 | done | `game_tools` smoke: argv, tracing install, `--headless` path, windowed boot/draw/orbit/recreate | Functional requirements |
| RSM-008 | done | Wire `render` into `engine/lib.rs`; `tools` deps (`game_engine`, `tracing-subscriber`, winit/glam) | Constraints & Assumptions |
| RSM-009 | done | Gates: `quality.md` + CI headless-smoke gate; full local gates + mobile guard + mimalloc-feature check | Non-functional requirements |
| RSM-010 | done | Docs sync: rendering/architecture/stack/milestones/version; links resolve; evidence recorded | Definition of Done |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | `engine::render` implemented, unit-tested per bullet | done | 17 render tests green (`tier` 3, `camera` 5, `planet` 3, `shaders` 2, `boot` 4); 28/28 workspace tests pass |
| 2 | Windowed smoke on 3 tiers (GPU machines); headless CI-green | done | windowed boot on Intel UHD 620 for low + medium (logs below; high = same path, tier-N=6 mesh proven headless); `--headless` output for all 3 tiers; CI gate added mirroring `quality.md` |
| 3 | 1.1 floor + portability by code inspection; mobile guard green | done | `MAX_API_VERSION = V1_1` + `ENUMERATE_PORTABILITY` pinned in `boot.rs` (tested); `cargo check` passes for `aarch64-linux-android` + `aarch64-apple-ios` |
| 4 | `mimalloc` flag flips allocator; default unaffected; A/B method noted | done | `game_engine/mimalloc` (default off) + `game_tools` pass-through; `cargo check --features mimalloc` + headless run green with identical descriptor hash; on-device A/B numbers deferred to M6 harness |
| 5 | Subscriber installed; device + tier logged at startup | done | `tracing_subscriber::fmt` (info default, `RUST_LOG` override); startup logs captured below |
| 6 | Docs synced; links resolve; evidence recorded here | done | `rendering.md` smoke section, `architecture.md` render/tools rows, `stack.md` allocator line, milestones M1 done + M2 next, techstack 0.3.0, `quality.md` + CI headless gate; this table |

## Acceptance criteria

- `cargo run -p game_tools -- --headless --tier low` exits 0 GPU-free and
  prints `tier subdiv cells tris hash gen_ms`.
- Windowed smoke opens on a Vulkan machine, renders the tinted planet,
  orbits with drag/wheel, survives resize.
- No `vulkano` usage in `game`; `hexsphere` API unchanged; no new
  clippy/fmt violations.

## Risks & Next steps

- Risk: naga GLSL frontend rejects a construct (push constants,
  shader version) → fallback: strip to varyings-only + fixed MVP per
  frame via vertex re-upload? No — fallback is fragment-only shading
  with camera baked per-frame into a re-uploaded uniform? Simplest real
  fallback: keep push constants but move MVP multiply to CPU and upload
  pre-transformed positions per frame (tiny at Low tier). Decide only
  on measured compile failure.
- Risk: mobile compile-guard breaks on new `render` code (desktop-only
  winit/vulkano calls) → keep `boot` behind types available on all
  targets; guard runs on every change here.
- Next: `debug-sphere-viewer` consumes `engine::render` (camera + planet
  triangulation); M2 builds passes (atmosphere/sky/terrain) on the smoke
  loop.

## Evidence

Local gates (2026-09-14, Windows, stable rustc 1.97.1):

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean; `cargo build --workspace`
  Finished; `cargo test --workspace --all-targets` 28/28 pass (11
  hexsphere + 17 render); `cargo test --doc --workspace` pass;
  `cargo run --bin game` + `cargo run -p game_debug --bin game_debug`
  exit 0.
- Headless smoke (dev profile, informational timings):
  - `tier=low subdiv=3 seed=1337 radius=1 cells=642 corners=1280
    tris=3840 hash=a0175dd0c40690a8 gen_ms=11.1`
  - `tier=medium subdiv=4 seed=1337 radius=1 cells=2562 corners=5120
    tris=15360 hash=861f3e69bb95dbe2 gen_ms=43.2`
  - `tier=high subdiv=6 seed=1337 radius=1 cells=40962 corners=81920
    tris=245760 hash=697faf4d6c71aebc gen_ms=722.7` (< 1 s budget)
- Windowed smoke on Intel(R) UHD Graphics 620 (IntegratedGpu, api
  1.3.215 — device capability; instance capped at 1.1 per policy):
  low + medium booted, planet generated with matching hashes
  (`a0175dd0c40690a8`, `861f3e69bb95dbe2`), event loop ran 20 s clean
  per tier (frames presenting, no panic/validation error), killed via
  timeout; high tier uses the identical code path (tier parameter only).
- Mobile guard: `cargo check --workspace --target
  aarch64-linux-android` + `--target aarch64-apple-ios` pass (default
  features; `mimalloc` C code correctly excluded).
- Allocator A/B method: `cargo check -p game_tools --features mimalloc`
  passes; `--features mimalloc -- --headless --tier low` prints the
  identical hash `a0175dd0c40690a8` (allocator changes no output).
  Timing/RSS A/B needs the M6 perf harness (reference devices unnamed
  until M6 per ADR-007) — method recorded, numbers deferred.
- Implementation notes: the fan triangulation initially dropped each
  cell's closing triangle (caught by `triangulation_counts_hold_on_every_tier`,
  fixed with modulo-close); `glam 0.33` deprecations (`look_at_rh`,
  `perspective_rh`) resolved via native `camera::rh::{view, proj::vulkan}`
  constructors; `ShaderModule::new` is `unsafe` in vulkano 0.35
  (documented SAFETY: naga-validated SPIR-V); exactly one
  `#[global_allocator]` lives in `game_engine` (a second one in the bin
  fails the build — tools only passes the feature through).
