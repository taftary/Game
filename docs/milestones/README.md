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
| [`free-flight-navigation`](../../plans/v0.1.0/free-flight-navigation/) | draft notion | §2 Navigation model |
| [`time-compression`](../../plans/v0.1.0/time-compression/) | draft notion | §2 Time compression |
| [`soi-handoff`](../../plans/v0.1.0/soi-handoff/) | draft notion | §3 SOI handoff |
| [`scale-physics`](../../plans/v0.1.0/scale-physics/) | draft notion | §5 Physics per scale |
| [`hierarchical-seeding`](../../plans/v0.1.0/hierarchical-seeding/) | draft notion | §6 Procedural seeding |

## v0.2.0 — Scale rendering & visuals (spec §4, §9)

| Feature | Status | Spec section |
|---|---|---|
| [`log-depth-rendering`](../../plans/v0.2.0/log-depth-rendering/) | draft notion | §4 Depth strategy |
| [`star-catalog-streaming`](../../plans/v0.2.0/star-catalog-streaming/) | draft notion | §4 LOD & streaming |
| [`exposure-tone-mapping`](../../plans/v0.2.0/exposure-tone-mapping/) | draft notion | §9.2 Dynamic range |
| [`depth-cueing`](../../plans/v0.2.0/depth-cueing/) | draft notion | §9.1 Depth cues |
| [`zodiacal-light`](../../plans/v0.2.0/zodiacal-light/) | draft notion | §9.4 Zodiacal model |
| [`waypoint-transitions`](../../plans/v0.2.0/waypoint-transitions/) | draft notion | §9.3 Waypoint experience |

## v0.3.0 — Player-facing layer (spec §10 + debug tooling)

| Feature | Status | Spec section |
|---|---|---|
| [`navigation-hud`](../../plans/v0.3.0/navigation-hud/) | draft notion | §10 HUD & overlay |
| [`autosave-persistence`](../../plans/v0.3.0/autosave-persistence/) | draft notion | §10 Persistence |
| [`scale-debug-screens`](../../plans/v0.3.0/scale-debug-screens/) | draft notion | spec §1 + §9.3 (dev tooling) |

## Post-v0.3 (unscheduled)

- **Colony milestones (historical labels):** M2 descent slice, M3 surface
  walk, M4 colonies + robots core, M6 mobile hardening, M7 content +
  polish slice, M8 VR prep audit. ADR-011 execution order for these (M2 →
  M3 → M4 → M6 → M7 → M8, after M5) applies when scheduled; landing-site
  selection stays with M2.
- **Audio:** deferred to navigation-engine spec v0.5 (spec §10, ADR-006
  in [`../decisions/`](../decisions/)).
- **Post-v1 candidates:** subterranean geometry
  ([`../game/journey.md`](../game/journey.md) L8), traversable universe
  layer (L1), landable moons (L4).

## Open product items (spec §10 — must not be silently dropped)

Visual/rendering style (PBR vs stylized), facility anchor coordinates,
player craft definition, terrain data policy, network/content policy,
validation plan. The v0.1.0-scoped items — **player craft definition**
and **validation plan** — are decided for v0.1.0 scope (PO decisions
2026-09-17, spec §10 addendum); the remaining four gate the v0.2.0 /
v0.3.0 notions moving `draft → planned`.
