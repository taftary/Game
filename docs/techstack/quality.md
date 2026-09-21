# Performance budgets (v1 ship gates)

| Metric | Phone low | Tablet medium | Desktop high |
|---|---|---|---|
| Frame (sustained) | 30 fps @ dynamic ≤1080p | 30–60 fps @ ≤1440p dynamic | 60 fps @ native |
| Frame p95 hitch (surface) | <100 ms, no save corruption | <66 ms | <33 ms |
| Draw calls (surface) | <300 | <600 | <1500 |
| Tris (surface) | <500k | <1.2M | <3M |
| Sim tick (20 Hz) | <8 ms avg, <16 ms p99 | same | same |
| Memory (app) | <1 GB | <2 GB | <4 GB |
| Cold start to menu | <5 s mid-tier phone | <4 s | <3 s |
| Planet chunk stream (descent) | fade ≤2 s acceptable | ≤1.5 s | ≤1 s |
| Depth bands per frame (v0.2.0) | ≤3 passes, one transient D32F depth image (demo-pinned) | ≤3 until ADR-007 tier data lands | same as Medium |
| Post chain per frame (v0.2.0) | +1 fullscreen resolve pass, one transient HDR image (16F preferred, packed-float fallback, content-preserving LDR bypass) | same | same |
| Cosmic bloom per frame, cosmic views only, debug shell (update-2026-09-18-2328; mip pyramid since v0.3.3 `bloom-mip-chain`) | prefilter + downs + pass-through + tent ups over 5 dedicated levels (`scene + 2·5` HDR images; pixel sum ≈ 1.7× scene — less than the retired 5× half-res targets ≈ 2.3× scene); `BLOOM=0` resolves scene + `down[0]`; LDR bypass draws the same layouts direct (no post) | same (pyramid runs High on all tiers in the debug binary) | same |
| Cosmic-web cue raymarch (v0.2.0) | 64x64 source grid, ≤16 steps, analytic fallback available | 128x128, ≤32 steps | 256x256, ≤64 steps |
| Cosmic field render (v0.3.3, ADR-025 — planned; each feature fills its row on `done`) | tracer splats ≤ 300k pts (≤ 5 MB/surface — shipped: stride gives 255 900, 4.1 MB, overdraw 2.0×; no new draw/pass); hubs 6720 impostors + ≤ 40k members (shipped, net +9k pts vs v0.3.2); veil = cell sprites ≤ 200k (no raymarch); bloom 5 levels, `scene + 2·5` images ≈ 14 MB at 1080p High (shipped); `WebField` sidecar ≤ 20 MB, boot +≤ 120 ms (shipped: 17 MB, +0 ms) | splats ≤ 1.0M; veil = quarter-res raymarch 32 steps (one 2 MB R8 3D image + one quarter-res HDR target); bloom 4 levels | splats all tracers (+ optional `refine(2)`); raymarch 48 steps; bloom 5 levels |

Budgets are enforced by the `tools` renderer smoke + device profiles, not by vibes. Any feature that blows Low tier is cut or tier-gated.

Note (v0.3.3): the "Cosmic-web cue raymarch (v0.2.0)" row describes
the CPU reference in `engine::render::cue`; the GPU veil row above
supersedes it as the shipped budget once `cosmic-gas-veil-v2` lands
(the v0.2.0 row is then removed by that feature's docs sweep).

Note (v0.3.3 `cosmic-depth-window`, shipped): the inspector slab
window keeps 12 % of Low splats at the nominal framing (8.3× fill
relief, `slab_relief` headless line); demo fog `L = 90` Mpc is a
visibility term only (no culling, no frame-time claim).

Thermal: sustained 15-min session must not throttle below Low-tier fps on reference phones (named in M6, see [`../milestones/`](../milestones/) and ADR-007 in [`../decisions/`](../decisions/)).

---

# Testing and quality gates

CI (`.github/workflows/ci.yml`) mirrors this list on every push/PR —
this file stays the single source of truth; CI never adds its own
gates. Run the local gates before every commit:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace
cargo test --workspace --all-targets
cargo test --doc --workspace
cargo run --bin game
cargo run -p game_debug -- --headless   # viewer check, GPU-free (debug-sphere-viewer)
cargo run -p game_tools -- --headless --tier low   # renderer smoke, GPU-free (M1)
```

Mobile compile-guard (CI always; local only if targets installed —
`rustup target add aarch64-linux-android aarch64-apple-ios`):

```text
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target aarch64-apple-ios
```

Local visual gate (v0.3.3 `cosmic-capture-harness`, GPU required — never
in CI; active since that feature is `done`): every cosmic
feature's DoD carries before/after PNGs from the fixed presets, and two
captures with identical arguments must be byte-identical on at least
one reference GPU:

```text
cargo run -p game_debug -- --capture shots/<preset>-after.png --seed 1337 --view inspector|slab|demo|vista
```

Reference result (Intel UHD 620, 2026-09-20): three consecutive
`inspector` captures at `1408x768` share SHA256
`CDC86FE0…` (gate script in
`plans/v0.3.3/cosmic-capture-harness/plan.md` CAP-008). Windowed
`F12` writes the same PNG of the current cosmic surface to
`captures/` (exploration only — DoD evidence always comes from
presets).

Test policy:

- `engine::universe`: determinism tests (same seed → same descriptor hash across N runs).
- `engine::sim`: fixed-step regression (scripted inputs → expected state hash).
- `engine::save`: round-trip + migration tests; corrupt-input tests.
- `tests/`: integration (colony tick, travel, save/load round-trip).
- Doc tests for all public `engine` APIs with examples.
- Device-profile perf tests via `tools` (synthetic planet + bot colony) — informational in v1, gates in later milestones.

Coverage goal is behavioral (DoD per feature), not a % number.
