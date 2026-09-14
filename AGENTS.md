# AGENTS.md - Instructions for AI Coding Assistants

This file is the single source of truth for AI assistants working on this
repository. Read it fully before making changes. If you change anything this
file documents, update this file to match.

## Project overview

PlanetCrafter is a freshly cleaned Rust workspace scaffold. Every source
file is an empty starting point: there is no implemented behavior, no
documentation book, and no CI. The project is a blank slate to grow from.

## Workspace layout

Cargo workspace (edition 2024, resolver 3) in the root `Cargo.toml`:

- `crates/engine` - library `planet_crafter_engine`. `src/` holds empty
  module files (`lib`, `node`, `scene`, `text`, `render`, `runtime`,
  `lod`, `visibility`) kept as layout placeholders.
- `crates/game` - game binary (default workspace member). `src/main.rs`
  is an empty `fn main() {}`.
- `crates/tools` - tooling binary (not a default member). `src/main.rs`
  is an empty `fn main() {}`.
- `docs/examples` - examples package. Each example is an empty
  `fn main() {}`; `viewer` is gated behind the `gpu` feature via
  `required-features`.
- `tests` - consolidated test package. One integration-test target per
  engine module plus `game`; every target and the fixtures library are
  empty files.
- `assets/fonts` - placeholder README only.

## Commands

```text
cargo build --workspace
cargo run --bin planet-crafter-game
cargo test --workspace --all-targets
cargo test --doc --workspace
cargo check --example viewer --features gpu
```

## Conventions

- Keep the workspace buildable: binaries and examples need an empty
  `fn main() {}`; library and test targets may be empty files.
- Shared dependency versions live in the root `[workspace.dependencies]`;
  member crates reference them with `{ workspace = true }`.
- `Cargo.lock` is committed - keep it in sync with manifest changes.
- `.cargo/config.toml` raises the Windows main-thread stack reserve to
  16 MB. Do not remove it.
- There is no CI pipeline; validate locally with the commands above.
