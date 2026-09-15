# Issue plan — flat view corner collapse

Parent specs: [`specs.md`](specs.md). Investigation: [`report.md`](report.md).

## Todo

| ID | Status | Task |
|----|--------|------|
| ISS-20260915-101 | done | `map_to_slot` helper; corners barycentric-mapped (own base), centers rerouted through it |
| ISS-20260915-102 | done | Island snap for relaxation drift (negatives → 0, renormalize) |
| ISS-20260915-103 | done | `corners_land_strictly_inside_their_islands` test + 2D helper |
| ISS-20260915-104 | done | Full quality gates |

## DoD verification

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Flat UV shows tessellated strip islands (Bourke), no line-soup | done | New test: >1 distinct corner UV per island N=1..=3, all inside-or-on own slot triangle; 39 engine tests green |
| No relaxation poke-through into neighbor islands | done | Island snap in `map_to_slot`; caught live by the first strict test run (N=3 w=−0.0102 → snapped) |
| No regressions | done | `fmt --check`, `clippy -D warnings`, `test --workspace --all-targets` (53+8+39), `test --doc`, `game` + `tools --headless` exit 0. `game_debug --headless` relink blocked by the running viewer instance (os error 5); bin tests (8) compiled the new binary cleanly — re-verify after viewer restart |
| Lifecycle respected | done | Parent files untouched; this issue carries specs → report → plan |

## Risks & Next steps

- Close the issue (`done`) after the user rebuilds with the viewer CLOSED
  and confirms the flat view matches the reference.
- Residual by design: dual fans on cuts still stretch (red-flagged).
