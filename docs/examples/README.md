# Architectural Examples

Current baseline: the `planet-crafter-examples` package with headless stubs
(`engine_api`, `validated_resource`, `state_transitions`, `message_flow`,
`renderer_neutral_scene`) plus a `gpu`-gated `viewer` stub. Headless examples
run without a display; the single GPU viewer example is gated behind the
`gpu` feature and excluded from headless CI.

Target: the stubs above grow into full demonstrations as the engine lands:

- `engine_api` - game code consumes engine contracts without backend details.
- `validated_resource` - resource construction validates input and reports
  errors explicitly.
- `state_transitions` - node split generation checks.
- `message_flow` - domain events cross an ownership boundary.
- `renderer_neutral_scene` - nodes become GPU-independent vertex data.
- `viewer` - Vulkan debug viewer, `gpu`-gated and excluded from headless CI.

See [Example policy](../book/examples/index.md) for the rules new examples
must follow.
