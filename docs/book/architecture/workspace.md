# Target Workspace

## Summary

Target: the project is a Cargo workspace with three responsibilities: an `engine`
library, a `game` binary, and `tools` for asset and developer workflows.
Dependencies always point toward the engine.

Current baseline: the workspace exists on disk as a buildable scaffold
(`Cargo.toml`, `crates/`, `tests/`, `docs/examples`, `assets/`). Engine
modules expose stub APIs (identity and contract types, typed errors,
headless runtime tick); the layout below is the Target they grow into.

The workspace layout is:

```text
Cargo.toml
crates/
  engine/       reusable runtime and rendering library
  game/         application and game content binary
  tools/        asset and developer tooling
assets/         source and processed asset inputs
docs/           architecture and engineering handbook, examples crate
tests/          consolidated application test suite
```

The engine is the reusable foundation. The game owns content and application
policy. Tools operate on defined formats and must not become a second runtime.

## Key points

- The engine crate exposes capabilities and data contracts, not backend handles.
- The game crate consumes engine APIs and owns content.
- Tools read engine formats but do not own runtime game state.
- The tests package consolidates the application test suite; it depends on
  the engine and is never a runtime dependency.
- A dependency-direction exception requires an architecture decision record.

## Dependency direction

```text
crates/tools ──> engine formats
crates/game  ──> engine APIs
tests/       ──> engine APIs and the test-internals surface
crates/engine ──> platform adapters and low-level dependencies
```

The engine never imports game, tools, or test code.
