# PlanetCrafter

A fresh Rust workspace scaffold. All source files are empty starting points;
there is no implemented behavior yet.

## Requirements

- Rust toolchain (edition 2024, Rust 1.85+) via [rustup](https://rustup.rs).

## Quickstart

```sh
cargo build --workspace
cargo test --workspace --all-targets
cargo run --bin planet-crafter-game
```

All commands succeed immediately: binaries and examples run an empty
`main`, and every test target contains zero tests.

## Project structure

```text
Cargo.toml    - workspace (engine, game, tools, examples, tests)
Cargo.lock    - committed for reproducible builds
.cargo/       - Windows main-thread stack reserve (16 MB)
crates/
  engine/     - library crate planet_crafter_engine (empty modules)
  game/       - game binary, empty fn main() {}
  tools/      - tooling binary, empty fn main() {} (not a default member)
docs/
  examples/   - example stubs; viewer is gated behind the gpu feature
tests/        - consolidated test package; every target is empty
assets/
  fonts/      - placeholder for bundled fonts
```

## Validation

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo test --doc --workspace
cargo check --example viewer --features gpu
```

There is no CI; run the commands above locally before committing.
