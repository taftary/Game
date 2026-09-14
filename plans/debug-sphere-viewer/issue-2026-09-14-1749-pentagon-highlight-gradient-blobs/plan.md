# Issue plan — debug-sphere-viewer / issue-2026-09-14-1749-pentagon-highlight-gradient-blobs (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

Add `flat` to the `v_tint` varying in both fill shaders (first-vertex
provoking = fan center → per-cell crisp tint). New positional tint
regression test. Full gates + before/after screenshots at N=1 (where
the symptom was visible) and N=6 (default view must not regress).

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260914-004 | done | `flat out`/`flat in` on `v_tint` in `FILL_VERT`/`FILL_FRAG` (+ explanatory comment) | Suspected area |
| ISS-20260914-005 | done | Regression test `tint_marks_only_pentagon_sites_positionally` (center/corner tint by position, not count) | Logs / Evidence |
| ISS-20260914-006 | done | Gates (fmt/clippy/test) + N=1 and N=6 verification screenshots | Logs / Evidence |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Pentagon highlight renders as crisp tinted faces at low N | done | `viewer_flat_n1.png`: solid-gold pentagons, sharp borders, shaded hexagons |
| 2 | Default N=6 view does not regress | done | `viewer_flat_n6.png`: same shaded ball + wireframe veil; stats 40,962 / `9f087a31` |
| 3 | Tint data exonerated and pinned by tests | done | `tint_marks_only_pentagon_sites_positionally` green alongside prior parity/outward tests |
| 4 | No workspace regressions | done | 48 lib + 7 bin tests green, clippy `-D warnings` clean, fmt clean; `engine`/`game` untouched |

## Acceptance criteria

- N=1 view shows solid tinted pentagon faces with crisp borders
  (screenshot-verified).
- `cargo test -p game_debug --all-targets` green; all quality gates
  unaffected.
