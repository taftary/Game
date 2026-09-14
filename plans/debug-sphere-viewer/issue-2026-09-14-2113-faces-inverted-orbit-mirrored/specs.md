# Issue specs — debug-sphere-viewer / issue-2026-09-14-2113-faces-inverted-orbit-mirrored (UTC)

Parent feature: [`../../notion.md`](../../notion.md)

## Status

`done`

## Observed vs Expected

Observed: the Sphere Viewer planet renders inside-out — the visible
surface is the *far* hemisphere seen from within (reported as "the
sphere mesh faces are inverted"). Vertical orbit input reads mirrored:
dragging the viewport up turns the visible surface down and dragging
down turns it up ("when i try to turn the sphere top it turn down and
down turn up").

Expected: near-side outward faces visible (convex planet read), and
grab-style orbit: the surface under the cursor follows the drag
direction.

## Reproduction steps

1. `cargo run -p game_debug`
2. Look at the fill: shading is plausible (a prior shader patch hides
   the defect) but the rendered hemisphere is the mirrored far side.
3. Drag vertically in the viewport: the surface moves opposite to the
   cursor.

## Scope & Impact

Visual + input-feel. Rendering half confined to `crates/debug` (fill
pipeline rasterization state + fill vertex shader); orbit half is one
sign in the shared `engine::render::OrbitCamera::rotate`
(`crates/engine`) — behavioral only, no API or mesh impact. No
mesh/data corruption (mesh hash cross-check intact), no game impact,
no gate impact. The same latent winding mismatch existed in
`game_tools` (`crates/tools/src/main.rs`) — invisible there because a
convex back-face-only sphere with no depth buffer still fills the same
silhouette; fixed with the same `FrontFace::Clockwise` one-liner (its
orbit feel was fixed via the shared camera).

## Logs / Evidence

- Interactive repro: mirrored vertical orbit response on drag.
- Prior symptom patch: `FILL_VERT` negates the mesh normal
  (`v_normal = -normal`) with a comment claiming the attribute points
  inward — it does not (`mesh.rs::fill_normals_are_unit_length`,
  `radial_normal` = position/radius, outward).

## Suspected area

`build_fill_pipeline` rasterization state (`CullMode::Back` + default
`FrontFace::CounterClockwise`) vs the Y-down NDC of
`OrbitCamera::projection_matrix` (glam `vulkan::perspective`): the
projection's Y-flip mirrors framebuffer-space triangle winding, so
outward CCW fans rasterize as CW and are culled as back faces.
