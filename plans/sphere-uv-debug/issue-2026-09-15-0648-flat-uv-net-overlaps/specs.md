# Issue specs — flat UV net overlaps (screenshot vs reference)

Parent feature: [`../../notion.md`](../../notion.md) (`done` — stays `done`;
this issue only cross-links, never rewrites parent files).

## Status

`done` (fix implemented + verified — see [`report.md`](report.md) and
[`plan.md`](plan.md)).

## Symptom

Windowed viewer, UV view main (`U`), any shader mode: the flat UV map is a
jagged all-red overlapping blob with spikes (screenshot Image 1) instead of
the clean 5-top / 10-middle / 5-bottom triangle strip of the reference
(Image 2, Paul Bourke icosahedron net).

## Intake (day one)

- Reporter: user screenshot vs reference, 2026-09-15.
- Scope: `crates/engine/src/render/uv.rs` net layout + flat pipelines in
  `crates/debug/src/main.rs`. No panel/state changes needed.
- Repro (headless-light): any `build_debug_uv` net whose triangles overlap;
  visually: run `game_debug`, press `U`.

## Fix direction (locked)

Replace the BFS mirrored-attachment unfold (arbitrary spanning tree →
self-intersecting in 2D) with the reference 5-10-5 strip: closed-form
absolute slot triangles + a single rooted tree walk assigning each slot the
forced icosa neighbor across the shared edge. Plus: flat pipelines stop
writing depth (coplanar seam-crossing tris must not z-fight).
