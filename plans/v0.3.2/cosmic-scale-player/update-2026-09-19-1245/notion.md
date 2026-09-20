# Update notion — cosmic-scale-player / update-2026-09-19-1245 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`in-progress` (P1 ribbons landed; P2 quad impostors still open)

## Reason for update

The cinematic refresh (`update-2026-09-18-2328`) delivered braided filaments,
grain, node impostors, and HDR bloom — but the visual still reads as
a dense white/pink wireframe mesh rather than the target: luminous
blue/purple filaments with golden cluster hubs and dark voids. The core
gap is that filaments are 1-pixel `LineList` segments with no lateral
falloff, and their brightness never crosses the bloom threshold (max
~0.6 < 1.0), so they never glow. This update closes that gap by
replacing hard wireframe with GPU-expanded ribbon quads, upgrading the
cluster glow, and retuning the palette.

**Execution note (2026-09-19, evening): P1 ribbons landed** — user
rejected the palette-graded wireframe ("lines dominant, one flat
gold"), so P1 executed against this draft: `strand_records` (96
B/record) + instanced `TriangleList` ribbon pipeline with rim-zero
lateral falloff, plus a 3-layer node light (white pinpoint + gold mid
+ amber halo) restoring blue-white small hubs. `WebLinePush` deleted;
rb1–rb2 screenshots; 60/59.5 fps on UHD 620. P2 (quad impostors,
`virial_radius_mpc` halos) still open.

## Roles

Author: PO. Consulted: ARCHITECT (module boundaries, rendering invariants),
TECHLEAD (budgets, todo sequencing). UX: not player-facing.

## Scope

### In-scope

- **P1 — Ribbon filaments**: replace `LineList` with vertex-shader-expanded
  camera-facing ribbon quads; fragment Gaussian falloff; density-graded
  emissive so dense filaments bloom; compact strand record buffers
  (~3 MB vs ~34 MB baked vertices per surface).
- **P2 — Cluster glow upgrade**: quad impostors replacing point sprites
  (kills 256px clamp); halo diameter from `virial_radius_mpc`; mass-stratified
  golden cores.
- **P4 — Palette & grading** _(landed in
  [`../update-2026-09-19-1933`](../update-2026-09-19-1933/notion.md) —
  the palette quick pass; this update keeps only the ribbon-dependent
  retune, if any, after P1/P2 land)_: dim low-density links so voids
  read dark; tune bloom threshold/intensity/exposure; soften redshift
  so gold survives at depth.
- Docs maintenance: rendering.md, architecture report, quality.md
  (if 3rd blur scale), version line.

### Out-of-scope

- P3 — Galaxy sparkle layer (separate update).
- P5 — LOD/culling (separate update).
- Volumetric raymarching, sheet rendering, 2LPT displacement (Phase 2–3
  in `cosmic-enhancement.md`).
- Descriptor generation changes (Zel'dovich, T-web, masses — no engine
  changes; enrichment-only).
- Optional 3rd blur scale (decided after tuning P2; marked optional in plan).

## Requirements delta

- Filaments render as soft ribbons with visible lateral falloff (not 1px
  wireframe). Dense filaments bloom; low-density links recede so voids
  read dark.
- Node cores are mass-graded gold; halos sized by `virial_radius_mpc`;
  no 256px point-size clamp artifact when flying close.
- Palette matches target: deep blue/purple filaments, golden hubs,
  dark voids.
- Descriptor hash unchanged (enrichment-only); determinism tests green.
- All `quality.md` gates green; no frame-time regression on Intel
  UHD 620 @1080p.
- Bloom write-once rule preserved (Intel UHD 620 driver constraint).

## Definition of Done

- [ ] Filaments are camera-facing ribbons with soft lateral falloff;
      no 1px wireframe visible — screenshot evidence, both surfaces
      (Game Demo + Cosmic Web tab).
- [ ] Dense filaments visibly bloom; low-density links recede; voids
      read dark — before/after screenshots vs. target.
- [ ] Node cores mass-graded gold; halos world-sized from `virial_radius_mpc`;
      no 256px clamp artifact up close.
- [ ] Descriptor hash unchanged; all determinism tests green.
- [ ] All `quality.md` gates green (fmt, clippy, build, test, headless
      viewer checks).
- [ ] No frame-time regression on this machine (Intel UHD 620) @1080p
      vs. baseline; bloom write-once rule verified.
- [ ] Docs synced: `rendering.md`, architecture report,
      `cosmic-enhancement.md` roadmap, `docs/techstack/README.md`
      version line (+`quality.md` if 3rd blur scale added).

## Open questions

- ~~Screen-space vs. world-space ribbon width~~ — **decided
  2026-09-19 (PO): world-space Mpc width + min-pixel clamp**, so
  filaments keep physical thickness that tapers with distance like the
  target reference, while distant filaments stay visible.
- 3rd blur scale needed? — decide after P2 halo tuning; only if
  halos need wider bloom than current 2-scale chain provides.
