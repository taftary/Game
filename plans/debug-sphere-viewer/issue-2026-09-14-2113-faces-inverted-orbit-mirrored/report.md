# Issue report — debug-sphere-viewer / issue-2026-09-14-2113-faces-inverted-orbit-mirrored (UTC)

Specs: [`specs.md`](specs.md)

## Root cause

Front-face determination happens in **framebuffer coordinates**, after
the viewport transform (Vulkan spec). The chain:

1. `build_fill` fans are CCW seen from outside in world space —
   proven by `fill_faces_point_outward` and bit-identical to the
   M1-proven engine mesh (`matches_engine_indexed_mesh_at_high_tier`).
2. `OrbitCamera::projection_matrix` is glam
   `camera::rh::proj::vulkan::perspective`: right-handed, **Y-down NDC**
   (documented in glam; pinned by `projection_uses_vulkan_ndc`). The
   baked-in Y negation mirrors every triangle's screen-space winding.
3. The viewer viewport uses a positive height (standard), which
   preserves that mirrored winding into framebuffer space.
4. The fill pipeline culls `CullMode::Back` with Vulkano's default
   `FrontFace::CounterClockwise` — so the outward (now
   framebuffer-CW) fans are classified as back faces and culled.

Result: only the far hemisphere's inner surfaces draw — the "faces
inverted" symptom.

A previous patch (`v_normal = -normal` in `FILL_VERT`, claiming "the
normal attribute points inward") masked the lighting on the wrongly
culled faces — the attribute is outward (`position / radius`); the
concave read came from shading the far side's outward normals. The
hack is reverted so the near side shades correctly once culling is
fixed.

**Second root cause (found by user test after the culling fix):** the
mirrored vertical orbit was *not* (only) a symptom of the inside-out
rendering — `OrbitCamera::rotate` added `+dy` (window pixels, y
down-positive) to pitch, so dragging up pitched the camera *down*.
Convention chosen with the user: FPS-style non-inverted — drag up
orbits toward the sphere's top. Fixed in the shared engine camera
(`pitch -= dy · 0.01`); the `game_tools` smoke shares `OrbitCamera`
and gets the same corrected feel. Horizontal yaw was never mirrored
(the Y-flip only affects vertical winding) and is unchanged.

## Evidence

- `mesh.rs` tests: `fill_faces_point_outward`,
  `fill_normals_are_unit_length`, `matches_engine_indexed_mesh_at_high_tier`
  (all green — model-space geometry was never the problem).
- glam 0.33.7 source `camera/rh/proj.rs`: `vulkan` = "NDC Z range
  [0, 1], Y-down".
- Vulkano 0.35.2 `RasterizationState::default()`: "counterclockwise
  front face".
- Numeric check of drag-up at the default framing (pitch 0.35 → 0.20):
  far-side feature screen elevation 14.06° → 12.43° (down), near-side
  feature 10.43° → 13.00° (up).
- User windowed test after the culling fix: faces correct, vertical
  orbit still inverted → independent input-sign root cause.

## Alternatives considered

- Flip triangle order in `build_fill` / engine mesh: rejected — the
  mesh is provably correct in world space, is bit-identical to the
  engine mesh by contract, and the defect is rasterizer-side.
- Negative viewport height + Y-up (OpenGL-style) projection: rejected
  — touches the shared engine camera and every viewport for zero
  benefit over the idiomatic front-face fix.
- Keep `v_normal = -normal`: rejected — it lights the wrong
  (far-side) faces; with correct culling it would invert the near
  side's shading.

## Fix direction

1. One-line rasterizer correction in `build_fill_pipeline`:
   `front_face: FrontFace::Clockwise` (framebuffer winding is mirrored
   by the Y-down projection), plus revert of the shader normal hack
   (`v_normal = normal`) with corrected comments.
2. `OrbitCamera::rotate` pitch sign flip (drag up ⇒ pitch up) in
   `crates/engine/src/render/camera.rs`; clamp test updated to the new
   sign. Both windowed binaries share the camera, so both are fixed.
