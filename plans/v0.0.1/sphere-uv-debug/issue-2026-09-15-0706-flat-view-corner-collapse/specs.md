# Issue specs — flat view corner collapse

Parent feature: [`../../notion.md`](../notion.md) (`done` — stays `done`;
this issue only cross-links, never rewrites parent files).

## Status

`done` (fix implemented + verified — see [`report.md`](report.md) and
[`plan.md`](plan.md)).

## Symptom

Flat UV view still unreadable after the strip-net fix
(`issue-2026-09-15-0648-flat-uv-net-overlaps`): islands render as blobs
with radiating lines instead of tessellated triangles like the Bourke
reference (http://www.paulbourke.net/panorama/icosahedral/ — "Variation":
each subdivided triangle maps inside its face).

## Root cause (day-one intake → confirmed in report)

`build_debug_uv` mapped EVERY corner of an island to the slot-triangle
centroid. True only at N=0; at N≥1 all corners per island share one UV
point, so every fan triangle degenerates into a line (center → centroid).
Bourke maps each tessellated triangle; we must barycentric-map each corner
like the centers.

## Fix direction (locked)

Route corners through the same slot-correspondence barycentric projection
as centers (`map_to_slot` helper). Corners are final-face centroids, hence
always strictly interior to their base face → strictly inside their slot
triangle (new test). Seam/island flags unchanged.
