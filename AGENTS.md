# AGENTS.md - Instructions for AI Coding Assistants

This file is the single source of truth for AI assistants working on this
repository. Read it fully before making changes. If you change anything this
file documents, update this file to match.

## Project overview

PlanetCrafter is a high-performance game built from scratch in **Rust**, using
**Vulkan** as the sole graphics API. The Target baseline is desktop Vulkan
(Windows first, Linux CI); Android (native Vulkan) and iOS
(Vulkan via MoltenVK) are **Planned**, not implemented - no mobile target,
build, or CI validation exists yet (`docs/book/architecture/technology.md`).
Current baseline: buildable workspace scaffold - `Cargo.toml`, `crates/`,
`tests/`, `docs/examples`, and `assets/` exist on disk with stub APIs
(identity and contract types, typed errors, headless runtime tick). The
Target early runtime is the geometric node system (hierarchical triangle
subdivision with directional vectors and bidirectional links) and a Vulkan
debug viewer to visualize it; subdivision behavior and rendering are still
Target/Planned.

## Workspace layout

Cargo workspace (edition 2024, resolver 3) in the root `Cargo.toml`:

- `crates/engine` - library `planet_crafter_engine`. Reusable geometry,
  scene data, text, the headless planet runtime manager, the
  headless LOD scheduler, the headless visibility culling pass, and the
  Vulkan viewer. Source modules: `node`, `scene`, `text`, `render`,
  `runtime`, `lod`, `visibility` (scaffolded with identity and contract
  types; subdivision and rendering behavior is Target).
- `crates/game` - game binary (the default workspace member).
  Application entry point and content policy.
- `crates/tools` - asset/developer tooling. Not a default workspace member.
- `docs/examples` - examples package. Compile-tested
  documentation examples; the `viewer` example is gated behind the `gpu`
  feature. Current baseline: headless example stubs plus a `gpu`-gated
  `viewer` stub; real viewer rendering is Planned.
- `tests/` - consolidated test package. Consolidated engine and game
  test suite, organized by engine module (see "Testing layout").
- `assets/fonts` - Target: bundled JetBrains Mono (SIL OFL), embedded via
  `include_bytes!`. Current baseline: README placeholder only, no font file
  vendored yet.
- `docs/` - mdBook architecture book (`docs/book`, the target blueprint)
  and validation scripts (`docs/scripts`).

Shaders are not files: Target GLSL `#version 450` sources are `pub(crate) const &str`
literals in `crates/engine/src/render/shaders.rs`, compiled to SPIR-V at
runtime by naga. No shader code exists on disk yet.

## Further reading

Before changing a module, read the relevant book pages:
- Engineering practices: `docs/book/practices/` - project structure, error
  handling, testing, public APIs, dependencies, performance.
- Design principles: `docs/book/principles/` - ownership, safety, type-driven
  design, SOLID, and DRY.
- Patterns: `docs/book/patterns/` - adopt one only when a concrete problem
  justifies it; patterns must be validated before they become project APIs.

## Commands

Run the application (local only, not in CI):

```text
cargo run --bin planet-crafter-game        # headless runtime summary (debug)
cargo run --release --bin planet-crafter-game  # optimized build
```

The windowed Vulkan viewer is Planned; `cargo run --example viewer
--features gpu` currently runs a stub (see "Validation that is local only").

Validation enforced by CI (`.github/workflows/docs.yml`):

```text
cargo fmt --check                              # formatting
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets           # test suite + headless examples
cargo test --doc --workspace                   # doc tests
cargo check --example viewer --features gpu  # compile-check only
docs/scripts/check-book.sh                     # docs: mdBook build + link/section checks
```

Validation that is local only (not run by CI, still expected to pass once
implemented):

```text
cargo run --example viewer --features gpu    # needs a Vulkan display
```

## Environment notes

- Rust toolchain: edition 2024, Rust 1.85+.
- Running the viewer requires a Vulkan-capable GPU and display. CI only
  compile-checks the GPU example; do not expect the `viewer` example to
  render on a headless machine.
- Windows: `.cargo/config.toml` raises the main-thread stack reserve to 16 MB.
  This is required by vulkano's runtime SPIR-V parser. Do not remove it.
- Test runs are headless; the `gpu`-gated `viewer` example is compile-checked
  separately (see "Testing layout").
- The mdBook version is pinned in `docs/.mdbook-version` and installed under
  `target/mdbook` by `docs/scripts/check-book.sh`. The first docs build may
  take a few minutes.
- `docs/book-output/` (mdBook output) and `target/` are generated and
  gitignored - never edit them directly.

## Code conventions

- Rust naming: `snake_case` for functions and modules, `PascalCase` for types,
  `SCREAMING_SNAKE_CASE` for constants.
- Keep public APIs documented: contracts, errors, ownership, and examples.
- Prefer small, named types over primitive values with implicit units.
- Visibility: prefer `pub(crate)` for internal collaboration; reserve `pub`
  for intentional crate boundaries and keep implementation modules private.
- Give each module one clear responsibility; do not create a generic utility
  module when a domain module owns the behavior.
- Error handling: `Result<T, E>` for recoverable failures,
  `Option<T>` for an expected absence, `panic!` only for violated internal
  invariants or unrecoverable startup. The scaffold follows this direction
  (typed errors such as `TextError`); keep it that way.
- `unsafe` code (target direction): safe Rust is the default. New `unsafe` is
  permitted only at platform/GPU boundaries, with a small scope and a
  documented safety invariant. Planned allowance is in
  `crates/engine/src/render/` (the vulkano `draw` call,
  `ShaderModule::new`, and the `wait_idle` device wait before in-place
  vertex-buffer rewrites). No `unsafe` exists on disk yet.
- Shared dependency versions are centralized in the root
  `[workspace.dependencies]` (e.g. `glam`); member crates reference them with
  `{ workspace = true }`.
- Dependency direction: the engine must not depend on game content or tools,
  and tools-only dependencies do not belong in runtime crates. `Cargo.lock`
  is committed - keep it in sync with manifest changes.
- The core dependencies are deliberate choices: `vulkano` (Vulkan bindings),
  `winit` (windowing), `naga` (GLSL to SPIR-V at runtime), `fontdue` (font
  rasterization), `glam` (math). Do not add new dependencies without a stated
  need.

## Testing layout

Testing layout (implemented by the scaffold):

- All engine and game tests live in the workspace-root `tests/` package
  (the consolidated test suite): one integration-test target per engine module
  (`node`, `scene`, `text`, `render`, `runtime`, `lod`, `visibility`) plus one `game` target. Modules with
  submodules are folders that split the tests one file per submodule (for
  example `tests/node/subdivision.rs`); each folder's `main.rs` declares the
  files as modules of its test target.
- Test fixtures shared across targets live in `tests/src/fixtures.rs` (the
  package's small library target).
- Whitebox internals reach the tests through the engine's `test-internals`
  feature, which adds the `planet_crafter_engine::testing` re-export module
  plus feature-gated re-exports in each engine module. This is a test-only
  surface, not a public API contract: builds without the feature expose
  nothing extra.
- `docs/examples` keeps its own inline tests: those are compile-tested
  documentation examples, not application tests.
- Tests and examples that need a window, GPU, or device are gated behind the
  `gpu` feature via `required-features`, so headless runs skip them
  automatically.

## Documentation rules

- `docs/book` is the **target** architecture blueprint. Current implementation
  facts belong in source documentation and architecture
  decision records - do not mix current-state claims into the target blueprint.
- Status language: use only `Current baseline`, `Target`, `Planned`, and
  `Open`. Never describe a planned crate, dependency, platform, or workflow as
  implemented.
- Use ASCII prose punctuation in documentation (hyphens, not em dashes).
  Mathematical and technical notation (arrows, degree signs, set membership)
  and box-drawing characters in directory trees are exempt.
- Documentation changes must pass `docs/scripts/check-book.sh` (build, link
  check, required page sections). Examples in docs must be small enough to
  compile and review.
- When a change retires a document, record its replacement in
  `docs/RETIREMENT.md`. Follow `docs/REVIEW_CHECKLIST.md` for docs and
  architecture reviews, and `docs/CONTRIBUTING.md` for PR expectations.

## Definition of done

Before considering a change complete:

1. `cargo fmt --check` passes.
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   passes.
3. `cargo test --workspace --all-targets` and `cargo test --doc --workspace`
   pass.
4. If docs changed: `docs/scripts/check-book.sh` passes.
5. If a module's behavior changed: the relevant book page or ADR is still
   accurate.
6. If scene/viewer behavior changed: the viewer controls documented in
  `README.md` are still accurate.
7. If the change affects anything this file documents (commands, workspace
   layout, conventions): `AGENTS.md` is updated to match.
