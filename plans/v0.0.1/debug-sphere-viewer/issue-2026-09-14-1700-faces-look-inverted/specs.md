# Issue specs — debug-sphere-viewer / issue-2026-09-14-1700-faces-look-inverted (UTC)

Parent feature: [`../../notion.md`](../notion.md)

## Status

`done`

## Observed vs Expected

Observed: the default Sphere Viewer look (N=6, wireframe on) renders
the planet as a flat, pale, almost featureless ball — no visible face
shading, reported as "faces are reverted in sphere!".

Expected (notion wireframe + goals): shaded dual-cell faces with a
wireframe overlay on top — faces readable, overlay decorative.

## Reproduction steps

1. `cargo run -p game_debug`
2. Look at the default view (N=6, both toggles on). Faces are
   unreadable; the disc reads as solid line-color.

## Scope & Impact

Visual only, confined to `crates/debug`. No mesh/data corruption (the
mesh hashes match the committed engine hash), no engine/game impact,
no gate impact. Default view only — lower subdivisions render
correctly.

## Logs / Evidence

- `viewer_issue.png`: N=6 default — uniform pale disc, no gradient.
- `tools_ref.png`: `game_tools` smoke (N=4, same mesh family) — rich
  day/night gradient, gold pentagons. Reference for "correct".
- `viewer_n2.png`: viewer at N=2 — correct per-face shading, gold
  pentagons, aligned wireframe. Proves fill/winding/culling/depth all
  work; the defect scales with subdivision density.
- `viewer_clean2.png`: after fix — N=6 gradient visible through the
  wireframe veil.

## Suspected area

Face winding / backface culling (viewer fan) vs wireframe overlay
density at N=6 (122,880 segments over a ~145k px disc).
