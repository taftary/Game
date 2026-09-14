# Issue report — debug-sphere-viewer / issue-2026-09-14-1700-faces-look-inverted (UTC)

Specs: [`specs.md`](specs.md)

## Root cause

Overlay saturation, not inverted geometry. At N=6 the wireframe holds
122,880 `LineList` segments over a ~145k px disc — cells are subpixel,
so nearly every pixel carries opaque line ink and the whole disc reads
as line color `(0.75, 0.87, 1.0)`. The correctly shaded fill renders
underneath but is buried.

Winding was exonerated by measurement, not assumption:

- `matches_engine_indexed_mesh_at_high_tier`: viewer `build_fill`
  output is bit-identical (all vertices + indices) to the M1-proven
  `SeededPlanet::to_indexed_mesh` at N=6.
- `fill_faces_point_outward`: every fan triangle's geometric normal
  agrees with the outward radial direction (N=0..=2).
- N=2 screenshot: correct gradient, culling, tint, depth, alignment.

## Evidence

- New regression tests (both green, kept in `crates/debug/src/mesh.rs`).
- Before/after screenshots (`viewer_issue.png` → `viewer_clean2.png`).
- `tools_ref.png` reference showing the same mesh family shading
  correctly without an overlay.

## Alternatives considered

- Default wireframe off: rejected — notion mandates default on.
- Dimmer opaque lines: rejected — at full coverage every pixel still
  reads as line color; brightness is not the problem, coverage is.
- Zoom-dependent line fade: rejected — complexity for a dev tool with
  no zoom-level requirements in the notion.

## Fix direction

Translucent wireframe: alpha in the line fragment color + alpha
blending on the line pipeline, so face shading always shows through
the veil at any subdivision. Toggle semantics and default-on unchanged.
