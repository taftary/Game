# Issue report — flat view corner collapse

Parent specs: [`specs.md`](specs.md). Status `draft → done` here; fix
plan in [`plan.md`](plan.md).

## Investigation

Reading http://www.paulbourke.net/panorama/icosahedral/ settled it:
Bourke maps each *tessellated* triangle ("Variation" section — faces split
into 4 repeatedly, every small triangle placed inside its face). Our
`build_debug_uv` mapped every corner of an island to the slot-triangle
centroid — exact only at N=0. At N≥1 all corners per island share one UV
point, so every fan degenerates into a center→centroid line: the line-soup
the user saw even after the strip fix. The strip itself was already proven
overlap-free; only the corner mapping was wrong.

A first strict test (`> 1e-4` inside) caught a second real effect:
relaxation + spherical projection drift boundary-face centroids up to ~1%
across base edges (N=3 corner 4: weight −0.0102). Fix snaps drifted
weights back onto the island (negatives → 0, renormalize); interior points
pass through untouched.

## Fix (implemented)

- `uv.rs`: new `map_to_slot` helper (barycentric vs unit 3D triangle +
  island snap + slot-corner weight routing); corners use it with their own
  base face, centers use it with the smallest incident island. Module docs
  updated (Bourke tessellation mapping).
- Tests: `corners_land_strictly_inside_their_islands` (inside-or-on per
  slot triangle + >1 distinct UV per island at N=1..=3 — the anti-collapse
  proof) with a 2D-barycentric helper.

## Verification

- `cargo test -p game_engine --lib`: 39 passed (1 new).
- Full gates in `plan.md` DoD table.
- IMPORTANT rebuild note: `game_debug.exe` cannot relink while the viewer
  runs (Windows os error 5) — close the viewer first, then
  `cargo run -p game_debug`, then `U`.
