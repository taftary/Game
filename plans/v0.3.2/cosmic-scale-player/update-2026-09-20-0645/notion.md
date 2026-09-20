# Update notion — cosmic-scale-player / update-2026-09-20-0645 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`done`

## Reason for update

User rejected line-dominant look: current build renders filaments as
geometry (1-px `LineList`, then camera-facing ribbon tubes), while the
target ([Image 1]) is gaseous blue smoke/dust with golden hubs and dark
voids. Replace the ribbon `TriangleList` pipeline with a perf-first
smoke-billboard display on both cosmic surfaces (Game Demo + Cosmic Web
tab), tuned for Low-tier mobile 30fps.

## Roles

Author: PO. Consulted: ARCHITECT (pipeline replacement, invariants),
TECHLEAD (budgets, Low-tier fill-rate).

## Scope

### In-scope

- CPU `smoke_puffs()` in `crates/debug/src/cosmic_web.rs` (per-link
  `length × density` counts, 1–6/link, 65k cap; world-size 2–6.5 Mpc;
  alpha 0.035–0.09; hub warming baked; `SMOKE_STREAM` domain).
- GPU `SMOKE_VERT/SMOKE_FRAG` instanced billboards in
  `crates/debug/src/main.rs` (billboard frame = 1 cross + normalize, no
  trig in vertex; 4x4 hash dust in fragment, no sin/exp per pixel;
  bounded redshift + 1→6 Mpc near-eye fade kept).
- Retire `RIBBON_VERT/FRAG`, `RibbonPush`, `StrandVertex`,
  `upload_cosmic_braid`, `COSMIC_*_LINE_EXPOSURE`,
  `RIBBON_HALF_WIDTH_MPC` (strand math stays for grain only).
- Per-surface smoke exposures (`DEMO 0.5 / MAP 0.12`); bloom write-once
  chain untouched.

### Out-of-scope

- Raymarched volumetrics, occluding (sorted-alpha) dust, sheet
  rendering, descriptor generation changes, grain budget cuts (follow-up),
  P2 quad impostors (still open).

## Requirements delta

- Filaments render as soft smoke puffs with radial falloff + dust hash,
  no line/tube geometry — both surfaces.
- Nominal: ~52k puffs (~104k tris, ~2.5MB/surface) vs ~1.2M ribbon tris;
  vertex trig eliminated; Low budget (<500k tris) holds with margin.
- Descriptor hash unchanged (enrichment-only); all `quality.md` gates green.

## Definition of Done delta

- [x] Smoke replaces ribbons on demo + inspector; no ribbon/line draws.
- [x] Nominal puff count in budget; headless layout prints smoke count.
- [x] Shaders compile under naga; vertex-input + safety pins updated.
- [x] `fmt, clippy -D warnings, build, test --workspace --all-targets`,
  both headless checks green.
- [x] Docs synced (`rendering.md`, techstack version line, this update).
