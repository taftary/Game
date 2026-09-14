# Issue report — debug-sphere-viewer / issue-2026-09-14-1749-pentagon-highlight-gradient-blobs (UTC)

Specs: [`specs.md`](specs.md)

## Root cause

Presentation, not data: `v_tint` was a smooth (perspective-correct
linear) varying. Pentagon fans (center=1, corners=1) render solid, but
each neighboring hexagon shares two corners with a pentagon (tint=1)
while its other corners and center are tint=0 — so gold interpolates
1→0 across those two-corner wedges, deep into the hexagon face. At low
subdivisions cells are huge, the wedges cover most of each neighboring
hexagon, and the whole site reads as a gold glow centered on the
pentagon fading into blue — i.e. "reverted"-looking. Data was
exonerated at every stage:

- CPU tint buffer positionally exact (new test
  `tint_marks_only_pentagon_sites_positionally`; runtime `TINT-DUMP`
  matched: 12 pentagon centers, 60 pentagon-adjacent corners).
- Viewer mesh bit-identical to the M1-proven engine mesh at N=6.
- Fan triangles geometrically outward-facing.
- Solid-red frag probe proved shader edits were live; highlight-flag
  bypass probe proved the push constant was not the variable.

## Evidence

- Before/after: `viewer_n1.png` (gradient blobs) →
  `viewer_flat_n1.png` (crisp gold pentagons, sharp borders).
- Default N=6 view unchanged (`viewer_flat_n6.png`): at that density
  smooth vs flat tint is subpixel-indistinguishable.
- `viewer_shaders_compile` covers the `flat`-qualified sources; naga
  30 lexes `flat` (token confirmed in its GLSL frontend).

## Alternatives considered

- Remove corner tints from the mesh (shrinks the bleed): rejected —
  still a gradient inside pentagon-adjacent fans, still not crisp, and
  it would diverge from the engine mesh parity the tests guarantee.
- Per-cell duplicated vertices with uniform tint: rejected — rebuilds
  the mesh layout for a shader-level concern.
- Quantize `v_tint` in the fragment shader (`step`): rejected — the
  threshold edge would sit mid-wedge, arbitrary and view-dependent.

## Fix direction

`flat` interpolation on `v_tint` (`flat out` in `FILL_VERT`, `flat in`
in `FILL_FRAG`). Fan triangles are `(center, corner_i, corner_i+1)`
and Vulkan's default provoking vertex is the first vertex, so every
triangle takes its cell center's tint: pentagon fans uniformly gold,
hexagon fans uniformly blue, borders crisp at any subdivision. Corner
tint data stays (mesh parity with the engine preserved); normals stay
smooth for shading. Two lines changed, no state changes.
