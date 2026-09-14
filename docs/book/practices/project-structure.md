# Project Structure

## Summary

Organize code by domain and responsibility. Keep entry points thin, expose the
smallest public surface, and move implementation into modules that can be tested
without starting the renderer or opening a window.

## Key points

- Use `lib.rs` for reusable logic and keep `main.rs` focused on startup.
- Prefer `pub(crate)` for internal collaboration; reserve `pub` for intentional boundaries.
- Keep business rules separate from I/O, platform callbacks, and configuration.
- Keep application tests in the workspace-root `tests/` package, mirroring the module tree.

## Workspace layout

```text
PlanetCrafter/
├── Cargo.toml
├── crates/
│   ├── engine/
│   ├── game/
│   └── tools/
├── assets/
├── tests/  # consolidated application test suite
└── docs/
```

Within a crate, use `lib.rs` for reusable logic and keep `main.rs` focused on
startup and orchestration. Application tests live in the workspace-root
`tests/` package; use `examples/` for runnable usage specifications and
`benches/` for measured performance work.

## Modules

Use one focused file for a small module and a directory for a cohesive feature.
Prefer `pub(crate)` for internal collaboration and reserve `pub` for an
intentional crate boundary.

```text
src/
├── lib.rs
├── render.rs
└── scene/
    ├── mod.rs
    └── geometry.rs
```

Name modules, files, functions, and variables with `snake_case`; name structs,
enums, and traits with `PascalCase`; name constants with `SCREAMING_SNAKE_CASE`.

## Rules for maintainable code

- Give each module one clear responsibility.
- Keep business rules separate from I/O, platform callbacks, and configuration.
- Prefer small functions with explicit inputs and outputs.
- Keep application tests in the workspace-root `tests/` package, organized by module.
- Do not create a generic utility module when a domain module owns the behavior.

The engine, game, and tools boundaries in
[crate boundaries](../architecture/crate-boundaries.md) take precedence over
generic application layouts.