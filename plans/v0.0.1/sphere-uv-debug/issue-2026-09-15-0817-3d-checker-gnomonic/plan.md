# Issue plan — sphere-uv-debug / issue-2026-09-15-0817-3d-checker-gnomonic (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

1. Engine `render::checker` gnomonic table + `gnomonic_uv` + GLSL block
   generator + invariant tests.
2. `FILL_FRAG` mode 3 gnomonic rewrite (shared fill + flat pipelines).
3. Debug drift-guard test + doc-comment updates.
4. Issue docs + `rendering.md` + techstack version + full gates.

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260915-201 | done | Engine `render::checker` module + tests | Fix direction |
| ISS-20260915-202 | done | `FILL_FRAG` mode 3 gnomonic checker + const block | Fix direction |
| ISS-20260915-203 | done | Drift-guard test + comment updates | DoD 4 |
| ISS-20260915-204 | done | `rendering.md`, techstack version bump | DoD 4 |
| ISS-20260915-205 | done | Full quality gates + evidence | DoD 4 |
| ISS-20260915-206 | done | Round 2: tree-propagated checker frames (`walk_tree_edges` export, Rodrigues fold, continuity test, regenerated GLSL block) | DoD 1 |
| ISS-20260915-207 | done | Round 3: cube-domain checker — `CheckerTable` (normals/axes/flips), 8^6 rotation+flip search (density-independent perfect fault matching), `checker_local/square/parity`, full test suite | DoD 1 |
| ISS-20260915-208 | done | Round 3 shader: 6-face select + EAC `atan` remap + flip parity, regenerated const block, compile + drift-guard tests | DoD 1 |
| ISS-20260915-209 | done | Round 3 docs (`report.md`, `rendering.md`, techstack 0.5.4) + full gates | DoD 4 |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Uniform squares / straight lines per face, same scale ×20, breaks only at borders, no seam smear | superseded by round-3 criterion: **only squares everywhere** — done (math) / visual pending user | `checker_alternates_consistently_across_all_cube_edges` (constant difference at all stations, exactly-1-fault per vertex, 4-edge matching — every density 2..32); user confirms on rebuilt viewer with `3` |
| 2 | Flat view shows same gnomonic checker | done | shared `FILL_FRAG` for fill + flat pipelines, same `v_normal` path |
| 3 | Density slider live, ≈ squares per edge | done | `pc.density` reused × `GNO_TWO_OVER_PI`; range 2..32 untouched |
| 4 | Tests + gates + docs + lifecycle | done (viewer-open subset) | gates below |

## Acceptance criteria

- Reviewer rebuilds with the viewer CLOSED, opens N=4, presses `3`:
  3D sphere shows a uniform per-face checker; `U` shows the flat
  unwrapped gnomonic checker; density slider re-tiles both live.
- `cargo test --workspace --all-targets` + all quality gates green.

## Risks & Next steps

- naga 30 const-array/dynamic-index support — compile test decides;
  fallback: switch-chain accessors.
- Close the issue (`done`) after the user confirms the visual on the
  rebuilt viewer.

## Gate evidence (2026-09-15, round 3 — viewer open, test-path green)

- `cargo fmt --check`: clean (after `cargo fmt`).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  clean (one fix: `needless_range_loop` in `cube_edge_faces`).
- `cargo test -p game_engine --all-targets`: 51 passed (6 checker tests
  incl. `checker_alternates_consistently_across_all_cube_edges` — the
  round-3 proof at every density 2..=32).
- `cargo test -p game_debug --lib`: 53 passed; `--bin game_debug`: 9
  passed (`viewer_shaders_compile` + `gnomonic_glsl_matches_engine` on
  the cube block with flips + EAC constants).
- `cargo test -p game_tools -p game -p game_tests`: green;
  `cargo test --doc --workspace`: green (1 doctest).
- `cargo run --bin game`: exit 0; `cargo run -p game_tools --
  --headless --tier low`: `hash=a0175dd0c40690a8` — exit 0.
- BLOCKED by the running viewer (os error 5 relink): `cargo build
  --workspace`, `cargo test --workspace --all-targets`, `cargo run -p
  game_debug -- --headless`. Rerun after the viewer closes.

## Gate evidence (2026-09-15, round 2 — viewer closed, all green)

- `cargo fmt --check`: clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  clean.
- `cargo build --workspace`: clean (viewer closed → relink ok).
- `cargo test --workspace --all-targets`: 51 engine (6 checker tests incl.
  new `walk_tree_frames_are_continuous_across_shared_edges`) + 53 debug
  lib + 9 debug bin (incl. `gnomonic_glsl_matches_engine` +
  `viewer_shaders_compile` on the regenerated block) + tools/game/tests
  green.
- `cargo test --doc --workspace`: 1 doctest green.
- `cargo run --bin game`: exit 0.
- `cargo run -p game_debug -- --headless`: `uv_islands=20
  uv_seam_verts=1902 uv_flat_tris=257340 uv_flat_verts=132560` — exit 0.
- `cargo run -p game_tools -- --headless --tier low`:
  `hash=a0175dd0c40690a8` — exit 0.

## Round-1 evidence (viewer open — partial, superseded by round 2 above)

- Same gates green except `build --workspace` /
  `test --workspace --all-targets` / `game_debug --headless`, which were
  blocked by the running viewer's exe lock (Windows os error 5).
