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

Budgets are enforced by the `tools` renderer smoke + device profiles, not by vibes. Any feature that blows Low tier is cut or tier-gated.

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

Test policy:

- `engine::universe`: determinism tests (same seed → same descriptor hash across N runs).
- `engine::sim`: fixed-step regression (scripted inputs → expected state hash).
- `engine::save`: round-trip + migration tests; corrupt-input tests.
- `tests/`: integration (colony tick, travel, save/load round-trip).
- Doc tests for all public `engine` APIs with examples.
- Device-profile perf tests via `tools` (synthetic planet + bot colony) — informational in v1, gates in later milestones.

Coverage goal is behavioral (DoD per feature), not a % number.
