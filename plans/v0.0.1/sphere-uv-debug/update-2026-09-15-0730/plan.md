# Update plan — sphere-uv-debug / update-2026-09-15-0730 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

1. Engine `FlatUnwrap` duplication builder + invariant tests.
2. Engine UV wireframe clipper + tests.
3. Debug wiring (state field, uploads, flat index binding, headless).
4. Docs + full gates.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260915-001 | done | `FlatUnwrap` builder in `render/uv.rs` (dup key `(src,island)`, single-island tris, non-seam byte-equal) + tests | In-scope |
| UPD-20260915-002 | done | `build_wireframe_uv_clipped` in `render/uv.rs` + tests | In-scope |
| UPD-20260915-003 | done | Debug: `SphereViewerState.flat`, `upload_flat` from `source`, flat draw binds `flat_indices`, remove debug `build_wireframe_uv`, headless `uv_flat_tris=` | In-scope |
| UPD-20260915-004 | done | rendering.md, techstack version, gates + evidence | In-scope |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Flat invariant tests | done | `flat_unwrap_never_spans_islands` (N=0..3, every tri single-island, uv∈[0,1]², originals-first, source valid) + `flat_unwrap_is_deterministic` — 45 engine tests green |
| 2 | Wire clip tests | done | `clipped_wire_never_crosses_islands` (per-piece endpoints inside own island, exact starts, emission order), `clip_segment_to_triangle_cases`, `clipped_wire_is_deterministic`; degenerate-fallback keeps 4-points-per-cross-edge alignment |
| 3 | Non-seam byte-equal | done | `flat_unwrap_keeps_non_seam_fans_byte_equal` (simulated stream position, full index stream accounted) |
| 4 | Headless + gates + docs | done | `game_debug --headless`: `uv_islands=20 uv_seam_verts=1902 uv_flat_tris=257340 uv_flat_verts=132560`; `fmt --check`, `clippy -D warnings`, `build --workspace`, `test --workspace --all-targets` (53+8+45), `test --doc`, `game` + `tools --headless` all green; rendering.md + techstack 0.5.1 |

## Acceptance criteria

- Flat Checker/Seams view: clean Bourke strip, no cross-net red bands;
  Seams toggle paints thin island-local cut portions.
- `cargo test --workspace --all-targets` + all quality gates green.

## Risks & Next steps

- Clip helper is the fiddliest part; tested per-island invariants.
- Follow-up idea (not this update): optional 3D-view duplicated checker.
