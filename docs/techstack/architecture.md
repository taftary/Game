# Architecture (ground truth, 2026-09-14)

Repo is an early Rust workspace scaffold: binaries run an empty `main`, tests
are empty, and the engine deps ([`stack.md`](stack.md): `vulkano`, `winit`,
`naga`, `fontdue`, `glam`, plus `hecs` + `tracing`) are declared but not
yet used. Canonical layout (single-sourced here — root `README.md` points here, never copy it):

```text
Cargo.toml    - workspace (engine, game, tools, debug, tests)
Cargo.lock    - committed for reproducible builds
.cargo/       - Windows main-thread stack reserve (16 MB)
crates/
  engine/     - library crate game_engine (empty modules, src/lib.rs is empty)
  game/       - clean release game binary, empty fn main() {}
  tools/      - tooling binary, empty fn main() {} (not a default member)
   debug/      - game_debug lib (Sphere Viewer + fps/console/inspector
                 screens) + game_debug binary: windowed sphere viewer with
                 developer tools (not a default member)
docs/         - project docs (game overview + techstack/game/milestones/risks/decisions)
plans/        - feature lifecycle: notion -> plan -> implement (see plans/README.md)
tests/        - consolidated test package; every target is empty
assets/
  fonts/      - placeholder for bundled fonts
```

Toolchain: Rust edition 2024, Rust 1.87+ via rustup (`rust-toolchain.toml` pins stable).

Workspace note (2026-09-14): `docs/examples` was removed; it is no longer a
workspace member and there is no `viewer` example. Renderer smoke coverage moves
to `crates/tools` (M1).

## Workspace layout — grow inside it

Keep the existing workspace shape; grow inside it:

```text
crates/engine/   # game_engine lib: renderer, universe gen, sim, assets, input, save
  src/
    lib.rs
    render/      # vulkano boot (1.1 floor) + tiers + orbit camera +
                 # seeded planet mesh + naga shaders (M1 renderer smoke)
    universe/    # seeds, galaxy/system/planet generation
    hexsphere/   # hex-dominant geodesic sphere mesh, base for planets/stars/moons
    sim/         # colonies, robots, resources, tick
    assets/      # loading, caching, hot-reload (dev only)
    input/       # unified touch/mouse/keyboard/gamepad actions
    save/        # versioned save format
    core/        # math, units, time, RNG, error types
crates/game/     # `game` binary: clean release entry — game states, camera journey, UI wiring
crates/debug/    # `game_debug` lib (Sphere Viewer screen: mesh/params/ui/
                  # text/app modules + fps/console/inspector stubs) + binary
                  # (non-default member): windowed viewer (orbit camera,
                  # fill + wireframe + pentagon highlight, inputs panel,
                  # --headless CI mode); developer screens never leak into
                  # the release binary
crates/tools/    # `game_tools` binary (non-default member): seed inspector,
                 # planet preview / renderer smoke (M1: seeded planet, orbit
                 # camera, Low/Med/High tiers, --headless CI mode), save
                 # migrator, asset cooker
tests/           # consolidated integration tests
assets/          # fonts, placeholder content, generated reference shots
plans/           # per-feature notion -> plan -> implementation
```

### Module boundaries (rules)

- `game` contains no `vulkano` pipeline code; it calls `engine::render`.
- `game` stays the clean release entry; developer screens live in `debug` and
  never leak into the release binary.
- `engine::sim` is headless-testable: no window, no GPU handle required.
- `engine::universe` is pure + deterministic: same seed + version → byte-identical descriptors.
- `engine::hexsphere` is pure + deterministic: same (N, radius, version) → bit-identical mesh (committed hash test).
- `tools` may depend on `engine` with a `test-internals`-style feature, but `game` must not need dev-only features to run.
- Platform code (`#[cfg(target_os = ...)]`) lives in `engine`, behind traits — `game` stays portable.

Existing `test-internals` feature on `game_engine` is reserved for exactly this kind of white-box tooling.
