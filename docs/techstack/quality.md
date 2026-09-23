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
| Cosmic field render (v0.3.4 `cosmic-gpu-tracers`, ADR-026 §2 — procedural, shipped 2026-09-23) | no tracer vertex buffers (0 B vs 16 MB v0.3.3 High); draws are `cells × k` index-driven invocations — Low 1 072 958 (k=1), Medium 2 145 916 (k=2), High 8 583 664 (k=8); hubs 5574 impostors + 17 070 members; veil = cell sprites ≈ 168k drawn (stride-2 of 335 387, ≈ 6.0 MB, no raymarch — shipped); bloom 5 levels, `scene + 2·5` images ≈ 14 MB at 1080p High (shipped); `WebField` sidecar 16.8 MB displacement + 2 MB grid + ≈ 4.4 MB cell list (18 MB, NFR2 ≤ 24 MB); `cells_ms` 46.6 ms release nominal (NFR3 ≤ 50 ms); vertex-stage texture fetch is a required device feature (Vulkan 1.0 core; mobile guards check it) | splats ≤ 1.0M; veil = quarter-res raymarch 32 steps ≈ 4.1M fetches/frame at 1080p (one 2 MB R8 3D image + ≈ 1 MB quarter-res HDR target — shipped); bloom 4 levels | proc splats all cells ×8 (+ optional `refine(2)` retired); raymarch 48 steps ≈ 6.2M fetches/frame at 1080p (shipped); bloom 5 levels |
| Cosmic rebase (v0.3.4 `cosmic-rebase-async`, ADR-026 §3; glow-only since `cosmic-gpu-tracers` CGT-010; done 2026-09-23) | no frame > 33 ms attributable to rebase; main-thread cost per rebase = upload + swap of the demo glow buffer only (measured UHD 620 dev-profile: 22 194 pts / 798 984 B in 1.50–2.11 ms — ≤ 8 ms, no two-frame split; the 17 MB splat upload retired — origin rides a push constant); headless traverse (507 Mpc, 10 rebases): max tick 0.47 ms dev-profile (frame-work analog, ≤ 33 ms gate); worker compute 5.5 ms dev-profile (glow only; was ≈ 13.8 s with splats), release 384–487 ms (off-frame) | same | same |
| Cosmic void contrast (v0.3.4 `cosmic-void-contrast`, ADR-026 §4 — transfer-grading, in-progress 2026-09-23) | shared `cosmic_transfer` (floor 0 / band 2.5 / knots slope 1 / rim 30 Mpc) across proc splats + march + sprite colours (CPU mirror; alpha untouched); MAP splat gain 0.25 (graded); HDR clear deep indigo `(0.02, 0.02, 0.06)`, resolve adds no backdrop; UHD 620 slab scans (seed 1337): void floor 1.00× backdrop, knot ridge 8.0×, wall sheet 1.55×; zero new fetches, ≤ 6 ALU per vertex/step; fragment arithmetic-only, write-once untouched | same | same |

Budgets are enforced by the `tools` renderer smoke + device profiles, not by vibes. Any feature that blows Low tier is cut or tier-gated.

Note (v0.3.3 `cosmic-gas-veil-v2`, shipped): the v0.2.0
"Cosmic-web cue raymarch" row described the CPU reference in
`engine::render::cue` (kept as a CPU test reference only); the GPU
veil row above is the shipped budget.

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
