# Issue report — cell-chunks / issue-2026-09-15-1144-hover-y-inverted (UTC)

Specs: [`specs.md`](specs.md)

## Root cause

`picking::ray_from_cursor` unprojected the cursor with `ndc_y = 2·v − 1`
(top pixel row → NDC y = −1). This app's convention — proven by the
daily-working UI (`ortho_matrix` maps pixel row 0 to NDC y = +1, pinned
by `ortho_maps_corners`) and the upright sphere render — is **NDC y =
+1 = top row**. The cursor's vertical axis was therefore inverted before
the inverse-VP unproject, so the picked cell was the top↔bottom mirror
of the cursor. The x mapping (`2·u − 1`) already matched the convention
(pixel left → NDC −1), which is why left/right hover was correct.

The convention lived nowhere in writing — `ray_from_cursor`'s docs said
only "Vulkan NDC (z ∈ [0, 1])" — so the wrong sign was a natural guess
during implementation.

## Evidence

- Symptom matches exactly: pure vertical mirror, x correct.
- `unproject_roundtrips_through_view_proj` green on the buggy code:
  middle-pixel anchor (v = 0.5 → ndc_y = 0, flip-invariant) plus a
  corner *miss* assertion (misses under both signs) — a symmetric-anchor
  blind spot, same test-design class as the tie-resolution case fixed
  during the feature.
- No other unprojection exists in the workspace (`ray_from_cursor`'s
  sole consumer is `ViewerApp::update_hover`); renderer forward path
  untouched by the bug.

## Alternatives considered

- Flip the renderer/projection instead — rejected: the sphere draws
  upright and all render tests pass; only the *inverse* mapping was
  wrong.
- Touch the x mapping too — rejected: x already matches the app
  convention (user-verified left/right correct).

## Fix direction

One-line fix in `ray_from_cursor`: `ndc_y = 1.0 − 2.0·v`, plus a doc
comment stating the app NDC-y convention explicitly. Regression test
closes the blind spot: project off-center cells (nearest +y / −y)
through the real `view_proj` → NDC → cursor via the app convention →
`ray_from_cursor` → `pick_cell` must return the same cell (fails on the
old code, passes on the fix). Strengthen the existing roundtrip test
with an off-center anchor.
