# Public APIs and Dependencies

**Status:** Target

## Summary

A public API is a long-term compatibility commitment. Keep public surfaces
small, document invariants, and re-export only types users should depend on.

## Key points

- Keep implementation modules private by default.
- Avoid leaking Vulkan, filesystem, or tool-specific types across engine/game boundaries.
- Treat feature flags and dependency choices as API design decisions.
- Follow semantic versioning when a crate is published or consumed externally.
- Record dependency-direction exceptions in an architecture decision record.
- Run `cargo fmt`, Clippy, tests, and documentation builds in CI.

**References**

[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
