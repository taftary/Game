# Dependency Management

**Status:** Target

## Summary

Dependencies are part of the architecture. Add a crate only for a concrete
capability, keep features minimal, and place it in the narrowest workspace crate
that needs it.

## Key points

- Add dependencies for concrete capabilities, not speculative reuse.
- Keep shared versions in the workspace manifest. Adopt `[workspace.lints]`
  as the workspace grows; CI's `-D warnings` is the current enforcement.
- Keep crate manifests focused on direct dependencies.
- Do not add tools-only dependencies to runtime crates.
- Treat feature flags as compile-time API and size decisions.
- Commit the application workspace lockfile for reproducible builds.
- Audit dependencies and remove unused crates regularly.
- Record dependency-direction exceptions in an architecture decision record.

## Workspace rules

```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }

[dependencies]
serde.workspace = true
```

Use flexible compatible versions for reusable libraries and rely on the lockfile
for application reproducibility. Record a dependency-direction exception in an
architecture decision record. A dependency must not make the engine depend on
game content or tools.

## Review checklist

Before adding a dependency, identify its owner, required features, license and
security posture, compile-time cost, and whether a standard-library or existing
workspace solution is sufficient. Validate changes with the workspace quality
gates and run an audit tool when the project's security workflow supports it.