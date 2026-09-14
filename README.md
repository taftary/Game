# PlanetCrafter

A fresh Rust workspace scaffold. All source files are empty starting points;
there is no implemented behavior yet.

## Requirements

- Rust toolchain (edition 2024, Rust 1.85+) via [rustup](https://rustup.rs).

## Quickstart

```sh
cargo build --workspace
cargo test --workspace --all-targets
cargo run --bin game
```

All commands succeed immediately: binaries and examples run an empty
`main`, and every test target contains zero tests.

## Project structure

```text
Cargo.toml    - workspace (engine, game, tools, examples, tests)
Cargo.lock    - committed for reproducible builds
.cargo/       - Windows main-thread stack reserve (16 MB)
crates/
  engine/     - library crate game_engine (empty modules)
  game/       - game binary, empty fn main() {}
  tools/      - tooling binary, empty fn main() {} (not a default member)
docs/         - docs of the project in md
plans/        - features lifecycle: notion -> plan -> implement -> done -> update/issue (see plans/README.md)
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

## Feature lifecycle (plans/)

Each feature lives in `plans/<feature-name>/` (kebab-case, unique). Full spec in
[`plans/README.md`](plans/README.md); templates in `plans/_template/`.

```text
1. notion.md defines needs + Status + Definition of Done
2. plan.md organizes features + todo list + DoD verification
3. implement from plan.md only
4. after done: update-YYYY-MM-DD-HHMM/ (UTC, own notion + plan)
5. issues: issue-YYYY-MM-DD-HHMM-<slug>/ (UTC, specs -> report + plan)
```

Status everywhere: `draft -> planned -> in-progress -> in-review -> done`,
plus `on-hold` / `cancelled`.
