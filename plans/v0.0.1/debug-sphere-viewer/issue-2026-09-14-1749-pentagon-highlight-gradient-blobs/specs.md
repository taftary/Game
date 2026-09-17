# Issue specs — debug-sphere-viewer / issue-2026-09-14-1749-pentagon-highlight-gradient-blobs (UTC)

Parent feature: [`../../notion.md`](../notion.md)

## Status

`done`

## Observed vs Expected

Observed (user screenshot, "it is still reverted !"): at low
subdivisions the pentagon highlight renders as soft gold gradient
blobs centered on pentagon sites, fading outward into blue hexagon
faces — the highlight reads as a glow bleeding into neighbors, not as
tinted faces. (Follow-up symptom after
[`../issue-2026-09-14-1700-faces-look-inverted/`](../issue-2026-09-14-1700-faces-look-inverted/)
fixed the N=6 overlay saturation.)

Expected (notion: "pentagon highlight toggle, pentagons tinted"):
pentagon faces uniformly, crisply tinted with sharp borders.

## Reproduction steps

1. `cargo run -p game_debug`
2. Set Subdivisions to 1, Regenerate.
3. Gold blobs centered on cell centers, fading to blue at cell edges.

## Scope & Impact

Visual only, confined to the viewer fill shader in `crates/debug`.
Mesh data, engine, gates unaffected. Most visible at N=0–2 (cells are
huge); at N=6 the effect is subpixel and invisible.

## Logs / Evidence

- `viewer_n1.png`: N=1 before fix — gold-center gradient blobs.
- `viewer_tintprobe.png` / `viewer_bypass2.png`: grayscale `v_tint`
  probes — white pentagon sites fading to black edges.
- `TINT-DUMP` stderr: CPU buffer tints exactly correct
  (center_sum=12, corner_sum=60, pentagon ids 1.0).
- `viewer_flat_n1.png`: after fix — crisp solid-gold pentagons.

## Suspected area

Vertex-tint interpolation in the fill pipeline (`v_tint` smooth
varying across fan triangles).
