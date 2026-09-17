# Issue specs — cell-chunks / issue-2026-09-15-1144-hover-y-inverted (UTC)

Parent feature: [`../../notion.md`](../notion.md)

## Status

`done` (report: [`report.md`](report.md), fix plan: [`plan.md`](plan.md) — all fix todos checked)

## Observed vs Expected

- Observed: hovering a chunk in the **top** of the 3D sphere viewport
  highlights the chunk at the **bottom**, and hovering the bottom
  highlights the top. Left/right hover is correct.
- Expected: the highlighted chunk is the one under the cursor on both
  axes (parent DoD 2: "Hovering any 3D sphere cell … shows the correct
  chunk id").

## Reproduction steps

1. `cargo run -p game_debug` (windowed viewer, default N=4).
2. Hover a chunk near the top limb of the sphere → the bottom-limb
   chunk highlights and the CHUNK panel shows the bottom id.
3. Hover near the bottom limb → the top chunk highlights.
4. Hover left/right limbs → correct chunks highlight.

## Scope & Impact

- Hover readout only (highlight + CHUNK panel id), main viewport and
  sphere thumb — both go through the same cursor→ray mapping.
- Click-to-pin inherits the same wrong cell (it pins `hovered`); no
  separate pin bug.
- Renderer, mesh, engine, and UV views are unaffected (the sphere draws
  upright; only the *inverse* cursor mapping is wrong).

## Logs / Evidence

- User report 2026-09-15 (windowed viewer): vertical inversion, x-axis
  correct.
- `ortho_matrix` + `ortho_maps_corners` test (`crates/debug/src/main.rs`):
  pixel row 0 (top) ↔ NDC y = +1 — the app-wide convention the
  unprojection violated.
- Existing `unproject_roundtrips_through_view_proj` passed despite the
  bug: it anchors on the middle pixel (flip-invariant) and a corner
  miss (misses under both signs) — see report.

## Suspected area

`game_debug::picking::ray_from_cursor` (`crates/debug/src/picking.rs`):
`ndc_y = 2·v − 1` sends the top cursor row to NDC y = −1 (the bottom
under this app's convention).
