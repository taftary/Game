# Issue plan — cell-chunks / issue-2026-09-15-1144-hover-y-inverted (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

### Fix

Flip the NDC-y sign in `picking::ray_from_cursor` (`crates/debug/src/picking.rs`)
and document the app convention at the site so the sign is never a guess
again. No renderer, engine, or parent-feature changes.

### Tests

- New `projected_cell_cursor_picks_same_cell_top_and_bottom`: cells
  nearest world +y / −y round-trip project→cursor→pick to themselves
  (must fail pre-fix — verified by stashing — and pass post-fix).
- Strengthen `unproject_roundtrips_through_view_proj` with an off-center
  anchor; add a small thumb-like rect case (same code path, second
  consumer).

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260915-001 | done | Flip `ndc_y` to `1.0 − 2.0·v` in `ray_from_cursor` + convention doc comment | Suspected area |
| ISS-20260915-002 | done | Regression test (full project→cursor→pick roundtrip, both rects) + off-center anchor in roundtrip test | Logs / Evidence |
| ISS-20260915-003 | done | quality.md gates green (fmt/clippy/build/test/doc/debug-headless) | Scope & Impact |
| ISS-20260915-004 | done | techstack README 0.6.0 → 0.6.1 bump; specs/report/plan Status → `done` | — |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Hover highlights the chunk under the cursor on both axes, main viewport + thumb | done | One-line sign fix in `picking::ray_from_cursor`; both sphere rects flow through it via `update_hover`. Reporter re-verifies in the windowed viewer (human acceptance). |
| 2 | Regression tests fail on old code, pass on the fix | done | `projected_cells_cursor_roundtrip_to_self` (every camera-facing cell at N=3, both rects — true visibility filter `eye·center > R²` after diagnosing the limb false-positive on cell 3) + strengthened `unproject_roundtrips_through_view_proj` off-center anchor: both FAIL on the stashed old sign (mirrored picks, e.g. cell 1 → 550) and PASS on the fix. 12/12 picking tests green. |
| 3 | quality.md gates green | done | `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-targets` (133 tests: 69 debug-lib + 10 debug-bin + 54 engine), `cargo test --doc --workspace` (5 doctests), `cargo run --bin game`, `cargo run -p game_debug -- --headless` (`pick_selftest=chunk0 ok`), `cargo run -p game_tools -- --headless --tier low` — all exit 0. |
| 4 | Issue files complete, techstack version bumped, links resolve | done | `specs.md` + `report.md` + `plan.md` present, all Status `done`; `techstack/README.md` 0.6.0 → 0.6.1. No parent-feature rewrites (cross-links only). |

## Acceptance criteria

- Unit proof: new tests fail with the old sign (stash check) and pass
  with the fix; full `quality.md` gate list green.
- Human proof: reporter re-hovers top/bottom/left/right limbs in the
  windowed viewer — highlight + CHUNK id follow the cursor on both axes.
- `git status` shows only: the issue folder, `crates/debug/src/picking.rs`
  (fix + tests), `docs/techstack/README.md` (version line).
