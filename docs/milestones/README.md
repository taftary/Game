# Milestones — versioned releases

Versions group [`plans/`](../../plans/) features into shippable steps; each
version is a folder under `plans/` (canonical rules:
[`plans/README.md`](../../plans/README.md)). Historical M0–M8 labels are
preserved for traceability (per ADR-011 in
[`../decisions/`](../decisions/): numbers are labels, not sequence).
Navigation-engine work from v0.1.0 on executes the adopted spec
[`../techstack/cosmic-navigation-engine-v0.4.md`](../techstack/cosmic-navigation-engine-v0.4.md)
("the spec").

Each planned feature gets `plans/<version>/<name>/notion.md` (needs +
Status + DoD) then `plan.md`. No implementation from notion alone.

## v0.0.1 — Foundations (current state, 2026-09-17)

Every feature done or in-review today. Historical mapping: M0 (scaffold
repair), M1 (renderer smoke + tiers + debug tooling), M5 (universe maps,
re-sequenced ahead of M2–M4 by ADR-011).

| Feature | Status | Historical label |
|---|---|---|
| [`build-foundations`](../../plans/v0.0.1/build-foundations/) | done | M0 |
| [`renderer-smoke`](../../plans/v0.0.1/renderer-smoke/) | done | M1 |
| [`hex-sphere`](../../plans/v0.0.1/hex-sphere/) | done | M1 (ADR-002) |
| [`cell-chunks`](../../plans/v0.0.1/cell-chunks/) | done | M1/M2 groundwork (ADR-010) |
| [`debug-screens`](../../plans/v0.0.1/debug-screens/) | done | M1 tooling |
| [`debug-sphere-viewer`](../../plans/v0.0.1/debug-sphere-viewer/) | done | M1 tooling |
| [`sphere-uv-debug`](../../plans/v0.0.1/sphere-uv-debug/) | done | M1 tooling |
| [`debug-player-view`](../../plans/v0.0.1/debug-player-view/) | done | M1 tooling |
| [`player-sphere-movement`](../../plans/v0.0.1/player-sphere-movement/) | done | M1/M3 groundwork |
| [`scale-hierarchy`](../../plans/v0.0.1/scale-hierarchy/) | done | docs contract (8 journey levels) |
| [`universe-maps`](../../plans/v0.0.1/universe-maps/) | done | M5 |
| [`universe-maps-3d`](../../plans/v0.0.1/universe-maps-3d/) | done | M5 |
| [`debug-ui-reorganize`](../../plans/v0.0.1/debug-ui-reorganize/) | done | M1 tooling |

Cancelled, kept for the record: [`chunk-flat-view`](../../plans/v0.0.1/chunk-flat-view/)
(visual-debug only; engine-side `render::chunk_flat` stays for player
streaming).

## v0.1.0 — Navigation core (spec §2, §3, §5, §6)

Work happens on branch `v0.1.0`; each feature lands as exactly one
commit when it reaches `done` (workflow rule:
[`plans/README.md`](../../plans/README.md) § *7. Version branch*).

| Feature | Status | Spec section |
|---|---|---|
| [`frame-hierarchy`](../../plans/v0.1.0/frame-hierarchy/) | done | §3 Coordinate & precision |
| [`free-flight-navigation`](../../plans/v0.1.0/free-flight-navigation/) | done | §2 Navigation model |
| [`time-compression`](../../plans/v0.1.0/time-compression/) | done | §2 Time compression |
| [`soi-handoff`](../../plans/v0.1.0/soi-handoff/) | done | §3 SOI handoff |
| [`scale-physics`](../../plans/v0.1.0/scale-physics/) | done | §5 Physics per scale |
| [`hierarchical-seeding`](../../plans/v0.1.0/hierarchical-seeding/) | done | §6 Procedural seeding |

## v0.2.0 — Scale rendering & visuals (spec §4, §9)

| Feature | Status | Spec section |
|---|---|---|
| [`log-depth-rendering`](../../plans/v0.2.0/log-depth-rendering/) | done | §4 Depth strategy |
| [`star-catalog-streaming`](../../plans/v0.2.0/star-catalog-streaming/) | done | §4 LOD & streaming |
| [`exposure-tone-mapping`](../../plans/v0.2.0/exposure-tone-mapping/) | done | §9.2 Dynamic range |
| [`depth-cueing`](../../plans/v0.2.0/depth-cueing/) | done | §9.1 Depth cues |
| [`zodiacal-light`](../../plans/v0.2.0/zodiacal-light/) | done | §9.4 Zodiacal model |
| [`waypoint-transitions`](../../plans/v0.2.0/waypoint-transitions/) | done | §9.3 Waypoint experience |

## v0.3.0 — Player-facing layer (spec §10 + debug tooling)

| Feature | Status | Spec section |
|---|---|---|
| [`navigation-hud`](../../plans/v0.3.0/navigation-hud/) | done | §10 HUD & overlay |
| [`autosave-persistence`](../../plans/v0.3.0/autosave-persistence/) | done | §10 Persistence |
| [`scale-debug-screens`](../../plans/v0.3.0/scale-debug-screens/) | done | spec §1 + §9.3 (dev tooling) |

## v0.3.1 — Debug shell unification (dev tooling)

Work happens on branch `v0.3.1`; each feature lands as exactly one
commit when it reaches `done` (workflow rule:
[`plans/README.md`](../../plans/README.md) § *7. Version branch*).

| Feature | Status | Spec section |
|---|---|---|
| [`unified-debug-view`](../../plans/v0.3.1/unified-debug-view/) | done | dev tooling (ADR-022) |

## v0.3.2 — Cosmic-scale player (main game notion)

Work happens on branch `v0.3.2`; the feature lands as exactly one
commit when it reaches `done` (workflow rule:
[`plans/README.md`](../../plans/README.md) § *7. Version branch*).

| Feature | Status | Spec section |
|---|---|---|
| [`cosmic-scale-player`](../../plans/v0.3.2/cosmic-scale-player/) | done | §1 W1 + §2 + dev tooling (ADR-023 extends ADR-022) |
| [`settings-seed-loader`](../../plans/v0.3.2/settings-seed-loader/) | done | dev tooling (single Settings seed editor + staged load progress) |
| [`cosmic-web-mass-rank-fix`](../../plans/v0.3.2/cosmic-web-mass-rank-fix/) | done | bugfix: densest peak now gets rarest mass (UNIVERSE_VERSION 2→3); closed at the v0.3.3 cut (deviation recorded in `plans/README.md` § 7) |
| [`cosmic-web-illustris-look`](../../plans/v0.3.2/cosmic-web-illustris-look/) | done | render-only enrichment: branching hair threads + smoke-v2 sheath + gold beads toward the Illustris target (no descriptor/hash change); closed at the v0.3.3 cut — reached the render-only ceiling, superseded by ADR-025 |

## v0.3.3 — Cosmic-web field render (ADR-025)

Work happens on branch `v0.3.3` (cut from `main` at `e4cb6fe`); each
feature lands as exactly one commit when it reaches `done` (workflow
rule: [`plans/README.md`](../../plans/README.md) § *7. Version
branch*). Goal: the cosmic surfaces read like the Illustris reference
([`../reports/images/target.jpeg`](../reports/images/target.jpeg),
described in
[`../reports/2026-09-20-cosmic-web-visual-description.md`](../reports/2026-09-20-cosmic-web-visual-description.md))
by rendering the **field** the generator already computes instead of
decorating the link graph (diagnosis:
[`../reports/2026-09-19-cosmic-web-visual-architecture.md`](../reports/2026-09-19-cosmic-web-visual-architecture.md)
§ 11). Features are listed in dependency order; PO decisions
(2026-09-20): engine export accepted, both surfaces with a vista intro,
tiered veil (sprites Low / raymarch Medium+High), v0.3.2 closed as-is.

| # | Feature | Status | Crate(s) | Scope |
|---|---|---|---|---|
| F0 | [`cosmic-capture-harness`](../../plans/v0.3.3/cosmic-capture-harness/) | done | debug | `--capture` offscreen PNG + four camera presets (`inspector`, `slab`, `demo`, `vista`) + `F12`; every later DoD's evidence; v0.3.2 baseline shots |
| F1 | [`web-field-export`](../../plans/v0.3.3/web-field-export/) | done | engine | `generate_cosmic_web_with_field` → non-hashed `WebField` sidecar (≈1M Zel'dovich tracers + overdensity, 128³ class/density grid); descriptor, hash, saves byte-identical |
| F2 | [`cosmic-tracer-splat`](../../plans/v0.3.3/cosmic-tracer-splat/) | done | debug | adaptive-kernel additive splats coloured by one density ramp; classes D/B/C emerge from density + hub proximity; tiers 300k / 1M / all; retires grain + beads |
| F3 | [`cosmic-depth-window`](../../plans/v0.3.3/cosmic-depth-window/) | done | debug | visibility fog on every cosmic draw + inspector slab mode (`S`, 10–80 Mpc) + near-ortho 20° inspector FOV; fills the `slab` preset; makes voids dark |
| F4 | [`cosmic-hub-hierarchy`](../../plans/v0.3.3/cosmic-hub-hierarchy/) | done | debug | mass-rank tiers A (1 %) / B (10 %) / C; 3-layer cores + member-galaxy scatter for A/B, warm beads for C; retires 3-per-node impostors |
| F5 | [`bloom-mip-chain`](../../plans/v0.3.3/bloom-mip-chain/) | done | engine + debug | write-once mip pyramid (13-tap down / tent up, 3–5 levels by tier, soft knee); the Intel rule becomes an executable pin; retires the 5-target blur |
| F6 | [`cosmic-gas-veil-v2`](../../plans/v0.3.3/cosmic-gas-veil-v2/) | done | debug | grid-driven gas bodies + walls: cell sprites on Low, quarter-res emission-only raymarch of a 128³ 3D texture on Medium/High; retires smoke + old veil + braid code |
| F7 | [`cosmic-vista-intro`](../../plans/v0.3.3/cosmic-vista-intro/) | done | debug | demo boots on the reference composition (outside, 25°, 40 Mpc slab), holds 2 s, dives 8 s to Chase; skippable, `V` replays; fills the `vista` preset; closes the version |

Cosmic roadmap items realised here: "volumetric bodies", "quad node
impostors" (as in-sprite ramps), "sheet-sprite render + density
export" (without the `UNIVERSE_VERSION` bump that roadmap assumed —
nothing hashed changes). Still deferred: LOD/culling hardening
(measured costs land in F2/F3/F6 DoDs), 2LPT / nested grids, SDSS
overlay.

## v0.3.4 — Cosmic-web fixes & fidelity (ADR-026)

Work happens on branch `v0.3.4` (cut from `main` at `6169532`); each
feature lands as exactly one commit when it reaches `done` (workflow
rule: [`plans/README.md`](../../plans/README.md) § *7. Version
branch*). Goal: close the three gaps left visible by v0.3.3 — nodes
drawn outside the descriptor sphere, a frame stall on every 50 Mpc
origin rebase, and a field render that still reads as blobs instead
of the reference's threads
([`../reports/images/target.jpeg`](../reports/images/target.jpeg)) —
by making the sphere a generation cut, generating tracers on the GPU
from the exported displacement grid, and moving rebase work off the
frame ([`../decisions/ADR-026.md`](../decisions/ADR-026.md)). PO
decisions (2026-09-23): generation-side clip (hash bump accepted),
no stored/refined tracer list (procedural GPU tracers instead), all
six features in scope, version named `v0.3.4`.

| # | Feature | Status | Crate(s) | Scope |
|---|---|---|---|---|
| F1 | [`cosmic-sphere-clip`](../../plans/v0.3.4/cosmic-sphere-clip/) | done | engine | peaks outside `descriptor_radius_mpc` rejected before acceptance; links/glow inherit; `UNIVERSE_VERSION` 3→4; pins + bands re-recorded; every node ≤ radius pinned |
| F2 | [`cosmic-rebase-async`](../../plans/v0.3.4/cosmic-rebase-async/) | done | debug | rebase rebuilds only origin-dependent buffers; CPU build on a worker thread (`TileLoader` pattern), buffer swap on the frame; veil volume / HDR chain / inspector buffers rebuild on reseed only; no fence wait in the loop |
| F3 | [`cosmic-gpu-tracers`](../../plans/v0.3.4/cosmic-gpu-tracers/) | done | engine + debug | `WebField.displacement` grid replaces `tracers`; splat vertex shader generates sub-tracers per cell (tier draw counts 1/2/8); per-seed cell list; retires `splat_records` + `SplatVertex` |
| F4 | [`cosmic-void-contrast`](../../plans/v0.3.4/cosmic-void-contrast/) | done | debug | shared transfer function: sub-mean → near-black, filament contrast band, soft rim fade; voids measured ≤ 1.15× backdrop |
| F5 | [`cosmic-hub-compact-cores`](../../plans/v0.3.4/cosmic-hub-compact-cores/) | done | debug | pixel-capped cores, members as the visible mass, bloom halo ≤ 3× core; ≤ 10 blazing nodes at slab framing |
| F6 | [`cosmic-vista-reframe`](../../plans/v0.3.4/cosmic-vista-reframe/) | done | debug | `vista` + `slab` presets frame an interior window (no sphere limb in frame); hub choice by composition; version headline shot |

Still deferred after this version: anisotropic (quad) splats
stretched along the collapse axis, LOD/culling hardening, 2LPT /
nested grids, SDSS overlay.

## v0.3.5 — Cosmic performance (ADR-027)

Work happens on branch `v0.3.5` (cut from `main` at the v0.3.4 merge);
each feature lands as exactly one commit when it reaches `done`
(workflow rule: [`plans/README.md`](../../plans/README.md) § *7.
Version branch*). Goal: stop tuning the cosmic path blind — timestamp
every pass, fix the dev-profile CPU tax — then run each GPU at its
specified tier instead of hard-coded High
([`../decisions/ADR-027.md`](../decisions/ADR-027.md)). PO decisions
(2026-09-23): instrument first, tier second; Low auto-selects on
`Cpu`/`VirtualGpu`, Medium on `IntegratedGpu`/`Other`, High on
`DiscreteGpu`; captures pin `--tier` (default High) so existing shot
hashes hold.

| # | Feature | Status | Crate(s) | Scope |
|---|---|---|---|---|
| F1 | [`cosmic-frame-timing`](../../plans/v0.3.5/cosmic-frame-timing/) | done | debug | timestamp query pool per pass (scene+splats, bloom, march, main) + CPU phase timers in the FPS widget + `--capture` log line; dev-profile opt-level 1; UHD 620 baseline: prepass owns ~95% |
| F2 | [`cosmic-device-tier`](../../plans/v0.3.5/cosmic-device-tier/) | done | debug | boot tier from device type with `GAME_DEBUG_TIER` override; drives `splat_k`, `VeilMode`, bloom levels; runtime cycle; `--tier` capture pin |

Still deferred after this version: splat frustum culling + sub-sample
LOD (`cosmic-splat-culling`), bricked indirect draws
(`cosmic-splat-bricks`), fill levers (`cosmic-fill-levers`) — all
gated on F1's measured numbers.

## Post-v0.3 (unscheduled)

- **Colony milestones (historical labels):** M2 descent slice, M3 surface
  walk, M4 colonies + robots core, M6 mobile hardening, M7 content +
  polish slice, M8 VR prep audit. ADR-011 execution order for these (M2 →
  M3 → M4 → M6 → M7 → M8, after M5) applies when scheduled; landing-site
  selection stays with M2.
- **Audio:** deferred to navigation-engine spec v0.5 (spec §10, ADR-006
  in [`../decisions/`](../decisions/)).
- **Post-v1 candidates:** subterranean geometry
  ([`../game/journey.md`](../game/journey.md) L8), landable moons (L4).
  (The traversable-universe-layer (L1) entry is retired: v0.3.2 lands
  the player-traversable cosmic web as the demo's starting dimension;
  region streaming, sheet rendering, and frame handoff stay follow-ups
  per ADR-023.)
- **Cosmic web roadmap** (decisions 2026-09-19, revised 2026-09-20 by
  ADR-025): the "filament ribbons" and "batched descriptor change"
  steps are replaced by v0.3.3's field render (`WebField` export is
  non-hashed, so no `UNIVERSE_VERSION` bump); volumetric raymarching
  lands tiered (Medium/High) in `cosmic-gas-veil-v2` instead of
  discrete-GPU-only. Still deferred: LOD/culling hardening (after
  v0.3.3 costs are measured), compute-grain (dropped), SDSS overlay
  (data-ingest project), 2LPT until nested grids exist.

## Open product items (spec §10 — must not be silently dropped)

Visual/rendering style (PBR vs stylized), facility anchor coordinates,
player craft definition, terrain data policy, network/content policy,
validation plan. The v0.1.0-scoped items — **player craft definition**
and **validation plan** — are decided for v0.1.0 scope (PO decisions
2026-09-17, spec §10 addendum). Provisional v0.3.0 decisions in the spec
§10 addendum unblock the current notions without closing the four remaining
open items.
