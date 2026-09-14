# Example Policy

## Summary

Examples are small specifications for the target architecture. They must be
runnable, focused, and independent from a display or GPU unless the page marks
them as platform-specific.

Each example includes:

- the problem and target boundary;
- the smallest useful implementation;
- a test or doctest;
- ownership and error decisions;
- a link to the architecture rule it demonstrates.

Production crates are not created by documentation examples. They are introduced
through the workspace migration process and tracked in architecture decision
records.

## Current baseline

Current baseline: the `docs/examples` examples package exists with headless
stubs and a `gpu`-gated `viewer` stub. The stubs grow into full
demonstrations as the engine modules land.

## Planned examples

Target: headless examples live in `docs/examples` and are compiled by
`cargo test --workspace --all-targets` once the workspace is re-scaffolded:

- `engine_api` - game code consumes engine contracts without backend details.
  Demonstrates [crate boundaries](../architecture/crate-boundaries.md).
- `validated_resource` - resource construction validates input and reports
  errors explicitly. Demonstrates [error handling](../practices/error-handling.md)
  and [type-driven design](../principles/type-driven-design.md).
- `state_transitions` - a node is split one generation and the resulting
  local generation is checked.
- `message_flow` - domain events cross an ownership boundary through a channel.
  Demonstrates the [events pattern](../patterns/events.md).
- `renderer_neutral_scene` - nodes become GPU-independent vertex data without
  opening a window. Demonstrates the renderer-neutral boundary
  ([crate boundaries](../architecture/crate-boundaries.md)).

The `viewer` example is Planned: it will be gated behind the `gpu` feature and open a Vulkan
window once implemented. It is excluded from headless CI; see [Testing and doctests](../practices/testing.md)
for the separation policy.
