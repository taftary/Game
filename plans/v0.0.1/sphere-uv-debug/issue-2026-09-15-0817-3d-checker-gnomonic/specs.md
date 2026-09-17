# Issue specs — 3D checker squares distorted on the sphere

Parent feature: [`../../notion.md`](../notion.md) (`done` — stays `done`;
this issue only cross-links, never rewrites parent files).

## Status

`in-progress` (fix direction locked with the user — see Fix direction).

## Symptom

Checker mode (`3`) on the 3D sphere view shows irregular squares: they
shear and vary in size across each icosahedron face, the grid direction
jumps at island borders, and seam-cell fans smear the checker across cuts.
Reference for how a sphere checker should look:
http://www.paulbourke.net/panorama/icosahedral/ (each icosahedron face is a
gnomonic/perspective projection plane; checker uniform per face, breaks
only at face borders).

## Root cause (day-one intake → confirmed in report)

`FILL_FRAG` mode 3 evaluates the checker in **net-UV space**
(`floor(v_uv * density)`). Those UVs come from the barycentric flattening
of each base face's chord triangle (`map_to_slot` in
`engine::render::uv`), which is affine-in-chord, not uniform on the curved
sphere — so squares distort on the 3D view by construction.

## Fix direction (locked)

Replace mode 3 with a **per-fragment gnomonic checker over the 20
icosahedron faces** (Bourke): select the face by `argmax dot(d, N)`,
gnomonic-project the direction onto its plane, checker in face-local 2D.
User decisions: gnomonic on **both** views (flat view becomes Bourke's
unwrapped icosahedral map — one shared shader, both views agree), and
**replace** mode 3 (no 7th mode; net-UV distortion stays readable via
LonLat + the flat view itself).

## Definition of Done

1. 3D checker: uniform square size + straight grid lines within each
   icosa face, identical scale on all 20 faces; discontinuities only at
   face borders (Bourke dog-leg); no smear across seam fans.
2. Flat view shows the same gnomonic checker (unwrapped map), consistent
   with the 3D view.
3. Density slider (2..32) re-tiles live; ≈ squares per face edge.
4. Engine mirror + drift-guard + shader compile tests green; full
   `quality.md` gates green; docs updated; lifecycle followed.
