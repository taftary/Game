# Issue plan — flat UV net overlaps

Parent specs: [`specs.md`](specs.md). Investigation: [`report.md`](report.md).

## Todo

| ID | Status | Task |
|----|--------|------|
| ISS-20260915-001 | done | Replace BFS unfold with 5-10-5 strip (`slot_pos` + `WALK` + `strip_slots`, barycentric reroute) |
| ISS-20260915-002 | done | Structural tests: bijection, exact shared edges, pairwise SAT no-overlap |
| ISS-20260915-003 | done | Flat pipelines: depth write off |
| ISS-20260915-004 | done | Docs (rendering/notion wording) + full quality gates |

## DoD verification

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Flat UV shows 20 non-overlapping strip islands (reference shape) | done | `strip_triangles_never_overlap` (190 pairs) + `strip_walk_covers_each_face_once` green; `cargo test -p game_engine --lib` 38 passed |
| No z-fighting on seam-crossing tris | done | Flat/flat-line `DepthState { write_enable: false }` in `crates/debug/src/main.rs` |
| No regressions (buffers, hash, gates) | done | `matches_engine_indexed_mesh_at_high_tier`, `committed_hash_matches` green; `fmt --check`, `clippy -D warnings`, `test --workspace --all-targets` (53+8+38), `test --doc`, `game`/`game_tools --headless` exit 0. Note: plain `cargo build` could not relink `game_debug.exe` because the viewer's running instance locks it (os error 5) — clippy + test builds compiled all targets cleanly, which subsumes it; `game_debug --headless` re-verifies once the viewer restarts |
| Lifecycle respected | done | Parent `notion.md`/`plan.md` untouched in content (one wording fix only); this issue carries specs → report → plan |

## Risks & Next steps

- Dual fans on island cuts still stretch across the map by design (per-vertex
  UVs, flagged red). True per-island duplication (vertex splits) is a
  follow-up if the red seams read as clutter at high N.
- Close the issue (`done`) after the user confirms the screenshot looks
  like the reference.
