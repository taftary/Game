# Update notion — cosmic-scale-player / update-2026-09-19-1933 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`in-review`

## Reason for update

The cinematic refresh (`update-2026-09-18-2328`) delivered braided
filaments, grain, node impostors, and HDR bloom — but the visual still
reads as a dense white/pink wireframe mesh rather than the target
reference (Illustris-style): luminous blue/purple filaments, golden
cluster hubs, dark voids. Root causes, per the architecture report and
the ribbon draft (`update-2026-09-19-1245`): filament alpha floor too
high (voids never darken), filament brightness capped under the bloom
threshold (~0.6 < 1.0, so filaments can never glow), redshift tint so
aggressive that gold dies with depth, and node cores not gold enough.

The full ribbon overhaul stays in `update-2026-09-19-1245` (draft). This
update is the cheap grading pass first: centralized knob changes only —
no pipeline, topology, or image changes — to land the target's color
story and mood on the existing `LineList` + sprite geometry.

## Roles

Author: PO. Consulted: ARCHITECT (rendering invariants — none touched),
TECHLEAD (zero budget impact: same geometry, same passes). UX: not
player-facing.

## Scope

### In-scope

- **Filament palette** (`cosmic_web.rs` braid ramps): alpha floor
  0.18 → ~0.05 so low-density links recede and voids read dark; blue
  channel pushed past 1.0 at high density so dense filaments cross the
  bloom threshold and glow (bright spine + bloom halo on the existing
  1-px strands).
- **Node palette** (`cosmic_web.rs` `node_color` / `node_impostors`):
  mass-stratified golden cores (giants brighter, more gold); massive
  halos warmed; halo alpha lifted slightly for giants.
- **Redshift softening**: `COSMIC_REDSHIFT_PER_MPC` 0.004 → 0.002
  (saturation moves 125 → 250 Mpc); gentler tint/dim coefficients in
  `GLOW_VERT` / `WEBLINE_VERT` so gold survives at depth. Safety pins
  (`max(clip.w, 0.0)`, z-cap 0.5) stay byte-identical.
- **Grade**: `COSMIC_BACKDROP` deepened; cosmic resolve exposure and
  bloom intensity lifted via bin-local consts (engine `BloomParams`
  spec defaults untouched).
- Docs maintenance: `rendering.md`, architecture report,
  `docs/techstack/README.md` version line.

### Out-of-scope

- Ribbon filaments (GPU quads) — stays in `update-2026-09-19-1245`;
  ribbon width decided there: world-space Mpc + min-pixel clamp.
- Cluster halo quad impostors / `gl_PointSize` 256 px clamp fix (same
  deferred update).
- Galaxy sparkle layer (P3), LOD/culling (P5), volumetrics.
- Descriptor generation changes (enrichment-only; hash unchanged).
- Engine `BloomParams::spec_defaults()` and the bloom chain topology.

## Requirements delta

- Voids read dark: faint-link emission sits at/below backdrop level.
- Dense filaments visibly bloom (blue/violet); strand crossings bloom
  harder than single strands.
- Cluster hubs read golden at all depths; massive-node halos warm.
- Palette constants stay within the bands pinned by the enrichment
  tests (braid rgb ≤ 2.0, point rgb ≤ 5.0, monotonic density/mass
  grading on every channel).
- Descriptor hash unchanged; determinism tests green.
- No perf delta: identical vertex counts, draw calls, and passes.

## Definition of Done delta

- [ ] Before/after screenshots on both surfaces (Game Demo + Cosmic
      Web tab) vs. the target reference: dark voids, blue-violet glowing
      filaments, golden hubs — user visual sign-off.
- [ ] Enrichment tests green (bands respected; repaired only where a
      pinned constant legitimately moved).
- [ ] Descriptor hash unchanged; engine determinism tests green.
- [ ] All `quality.md` gates green, incl. both `--headless` checks.
- [ ] Docs synced: `rendering.md`, architecture report, techstack
      README version line; `update-2026-09-19-1245` draft updated
      (palette phase landed here; ribbon width decision recorded).
