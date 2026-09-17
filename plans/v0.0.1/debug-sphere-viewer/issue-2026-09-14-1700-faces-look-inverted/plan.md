# Issue plan — debug-sphere-viewer / issue-2026-09-14-1700-faces-look-inverted (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

`LINE_FRAG` emits `(0.75, 0.87, 1.0, 0.45)` and the line pipeline gets
`AttachmentBlend::alpha()` (draw order fill → lines already correct, so
no other state changes). Two regression tests pin the exonerated
geometry so a real winding regression can never hide behind this issue
again. Full gates + windowed screenshot verify.

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260914-001 | done | Translucent wireframe: line frag alpha 0.45 + alpha blend on line pipeline | Reproduction steps |
| ISS-20260914-002 | done | Regression tests: `matches_engine_indexed_mesh_at_high_tier`, `fill_faces_point_outward` | Suspected area |
| ISS-20260914-003 | done | Full gates (fmt/clippy/test) + windowed before/after screenshots | Logs / Evidence |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | N=6 default view shows shaded faces through the wireframe overlay | done | `viewer_clean2.png`: day/night gradient + veil; stats unchanged (40,962 cells, `9f087a31`) |
| 2 | Toggle semantics and default-on wireframe unchanged | done | No `app.rs`/`sphere_viewer.rs` changes; `switching_preserves_viewer_state` still green |
| 3 | Geometry exonerated by measurement and pinned by tests | done | `matches_engine_indexed_mesh_at_high_tier`, `fill_faces_point_outward` green |
| 4 | No regressions anywhere in the workspace | done | 47 lib + 7 bin tests green, clippy `-D warnings` clean, fmt clean |

## Acceptance criteria

- Default N=6 view reads as a shaded planet with a wireframe veil
  (screenshot), not a flat pale ball.
- `cargo test -p game_debug --all-targets` green; workspace gates
  unaffected (`engine`/`game` untouched by this fix).
