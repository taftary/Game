# PlanetCrafter

A high-performance game built from scratch in **Rust**, using **Vulkan** as the
sole graphics API. Target: desktop Vulkan first (Windows, Linux CI); Android
(native Vulkan) and iOS (Vulkan via MoltenVK) are **Planned** - see the
[technology status](docs/book/architecture/technology.md).

Current baseline: buildable workspace scaffold - `Cargo.toml`, `crates/`,
`tests/`, `docs/examples`, and `assets/` exist on disk with stub APIs
(identity and contract types, typed errors, headless runtime tick). The
Target early runtime is the geometric node system and a Vulkan debug viewer.
See the [Rust architecture book](docs/book/index.md).

## Requirements

- Git
- Rust toolchain (edition 2024, Rust 1.85+) via [rustup](https://rustup.rs).
  Verify with:
  ```sh
  rustc --version
  cargo --version
  ```
- Python 3 (for `docs/scripts` link/section checks) and a Bash shell.
- For the Target viewer (Planned, not runnable yet): a Vulkan-capable
  GPU/driver and a display. Headless machines can only validate docs.

## Quickstart

```sh
git clone https://github.com/taftary/Game.git
cd Game
cargo test --workspace --all-targets
cargo run --bin planet-crafter-game
docs/scripts/check-book.sh
```

Expected: the workspace builds and all headless tests pass, the game binary
prints a headless runtime summary, and the mdBook builds from `docs/book`
with link/section checks passing.

## Validation

Enforced by CI (`.github/workflows/docs.yml`):

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo test --doc --workspace
cargo check --example viewer --features gpu
docs/scripts/check-book.sh    # mdBook build + link/section checks
```

Local only (needs a Vulkan display): `cargo run --example viewer
--features gpu` (currently a stub, see `AGENTS.md`).

## Project structure

Actual tree today:

```text
Cargo.toml    - workspace (engine, game, tools, examples, tests)
Cargo.lock    - committed for reproducible builds
.cargo/       - Windows main-thread stack reserve (16 MB)
AGENTS.md
README.md
crates/
  engine/   - reusable geometry, scene, text, runtime, lod, visibility, Vulkan viewer
  game/     - application binary and content policy
  tools/    - asset/developer tooling (not a default member)
tests/      - consolidated test suite (one target per engine module + game)
assets/
  fonts/    - Target: bundled JetBrains Mono (SIL OFL); placeholder only
docs/       - target architecture book, handbook, and validation scripts
  book/       - mdBook source (Target blueprint)
  examples/   - examples package, viewer gated by gpu feature
  scripts/    - check-book.sh, link/section checks
.github/    - CI workflows (docs validation)
```

## Viewer controls

Planned. The `viewer` example is still a stub, so no controls are documented.
When scene/viewer behavior lands, controls will be documented here per
`AGENTS.md` Definition of Done.

## Troubleshooting

- Docs build is slow on first run: `check-book.sh` installs the pinned mdBook
  under `target/mdbook` - this can take a few minutes.
- `docs/book-output/` and `target/` are generated and gitignored - never edit
  them directly.

## Documentation

- [Rust architecture book](docs/book/index.md) - Target workspace, crate
  boundaries, principles, patterns, and practices
- [Technology targets](docs/book/architecture/technology.md) - Target/Planned/Open status
- [Contributing guidelines](docs/CONTRIBUTING.md) - how to submit
  documentation and architecture changes
- [Review checklist](docs/REVIEW_CHECKLIST.md) - checklist for docs and
  architecture reviews
- [Style guide](docs/STYLEGUIDE.md) - status language, Rust conventions,
  page structure, and validation commands
- [Code of conduct](docs/CODE_OF_CONDUCT.md)
- [Retirement map](docs/RETIREMENT.md) - where deleted docs went

Status language: `Current baseline`, `Target`, `Planned`, `Open` only. Never
describe a planned crate or workflow as implemented.

## Main dependencies (Target)

- [`vulkano`](https://crates.io/crates/vulkano) - safe Vulkan bindings
- [`winit`](https://crates.io/crates/winit) - cross-platform windowing
- [`naga`](https://crates.io/crates/naga) - GLSL to SPIR-V at runtime (pure Rust)
- [`fontdue`](https://crates.io/crates/fontdue) - font rasterization
- [`glam`](https://crates.io/crates/glam) - SIMD-accelerated math

See `AGENTS.md` for AI assistant instructions, dependency direction rules,
and the full Definition of Done.
