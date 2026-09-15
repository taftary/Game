# Issue report — flat UV net overlaps

Parent specs: [`specs.md`](specs.md). Status moved `draft → in-review`
here; fix plan in [`plan.md`](plan.md).

## Investigation

`crates/engine/src/render/uv.rs::unfold_net` attached each base face to
the first already-placed neighbor (BFS discovery order). That spanning
tree is arbitrary: mirrored equilateral attachment along it self-intersects
in 2D, so islands piled on top of each other. The flat pipeline then drew
them coplanar (depth write on) at identical z → z-fighting, and dual fans
whose center/corner islands differ stretched across the pile (the spikes).
Red-everywhere is the seam overlay firing on those stretched fans plus the
pile-up. 3D views were unaffected (uv only feeds checker/seam modes there).

No panel/state/input changes were needed — purely net math + flat depth
mode. The `mesh_hash` pin is untouched (uv stays hash-excluded).

## Fix (implemented)

- `uv.rs`: deleted the BFS unfold. The 20 slots are now absolute
  closed-form equilateral triangles in the reference 5-10-5 strip
  (`slot_pos`); one rooted tree (`WALK`, 19 edges, slot 0 = base face 0)
  assigns each slot the forced icosa neighbor across the shared parent
  edge (`strip_slots`). Center UVs route barycentric weights through the
  slot's vert correspondence (template order ≠ face-triple order).
- `crates/debug/src/main.rs`: flat + flat-line pipelines no longer write
  depth (test stays `Less`, cleared 1.0) — seam-crossing tris overdraw in
  index order instead of fighting.
- Tests: `strip_walk_covers_each_face_once` (bijection, bit-exact shared
  edges, real icosa adjacency per tree edge), `strip_triangles_never_overlap`
  (pairwise strict-interior SAT over all 190 pairs), existing
  range/islands/seams/determinism suites unchanged and green.
- Docs: `rendering.md` net wording corrected (BFS → strip), notion flat
  pipeline line corrected (no-depth → test-on/write-off).

## Verification

- `cargo test -p game_engine --lib`: 38 passed (2 new).
- Full gates below in `plan.md` DoD table.
