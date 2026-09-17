# Notion — Cosmic-to-Subterranean Scale Hierarchy

## Status

`done` (all DoD rows evidenced in [`plan.md`](plan.md) DoD verification;
ANALYST audit + SECURITY review passed 2026-09-16, no findings)

## Context

`docs/game/journey.md` defines a **6-layer** scale contract (Galaxy → Star
system → Planet globe → Orbit → Atmosphere → Sky → soil) and calls the
scale/camera journey "the hardest technical problem". Only the planet-globe
layer and below have code (`HexSphere`, `OrbitCamera`, `PlayerCamera`,
`ChunkStreamer`); M2 (descent slice, levels 5–7 transition) is next and
M5 (Universe v1) covers the top layers. Two spans of the player's mental
model are unowned by any doc: **above the galaxy** (a universe container)
and **below the surface** (subterranean; only "caves-as-markers" exists in
`gameplay.md`). Moon/ring companions at the planetary-system span are also
unnamed. The proposal: adopt an **8-level** cosmic→subterranean hierarchy
as the canonical contract, adapted to the project's compressed game-feel
distances ("Real astronomical units are **not** simulated" —
[`docs/game/journey.md`](../../../docs/game/journey.md)).

## Problem & Needs

- The scale contract stops at "sky → soil"; there is no documented home
  for universe-above-galaxy, planetary companions (moons/rings), or
  anything below the surface, so future work (M5, post-v1 underground)
  has nothing to anchor to.
- The 6-layer list and the player-facing "cosmic to ground" pillar
  (`docs/game/README.md`) under-sell the journey the game promises.
- Each level needs explicit **v1 ownership** (implemented / backdrop /
  markers-only / future) so scope stays honest while the full span is
  documented.

## Goals

1. `journey.md` carries the canonical 8-level table: level number, name,
   real-world reference range, compressed game extent, v1 representation,
   key entities, domain category.
2. A drill-down diagram maps levels ↔ the 4 domain categories ↔ v1
   travel states.
3. Level 8 gets a subterranean deep-dive (shallow / mid-depth / deep).
4. Satellite docs (`game/README`, `scope`, `universe`, `gameplay`,
   `milestones`, techstack version, risks) stay consistent with the
   8 levels.

## Non-goals

- No code, no behavior change (docs-only feature).
- No state-machine renames (`GalaxyMap` stays the v1 top state); levels
  1 and 8 get no v1 state.
- No milestone renumbering (M1 is referenced by shipped plans).
- No landable moons, no subterranean geometry, no traversable universe
  layer in v1.
- No new ADR (game-design contract, not a techstack lock).
- No `AGENTS.md` change (it documents no scale layers).
- No Notion-app deliverable (user chose project docs as canonical home).

## Users / Stakeholders

- Primary: players building a mental model of one continuous journey
  from cosmos to underground (future).
- Secondary: developers/designers anchoring M2/M5/post-v1 work to named
  levels instead of inventing new ones.
- UX note (consulted): no UI added, no control bindings change; existing
  zoom/map affordances in `controls.md` still anchor to the unchanged
  v1 states. Consistent with the camera journey by construction — this
  feature *is* the journey doc.

## Roles

Author: PO. UX consulted (required if player-facing): yes — no UI change,
controls unchanged, journey extended not contradicted. ARCHITECT consulted
(required if cross-module): yes — contract spans game/techstack/milestones
docs; no module boundary or invariant touched (docs-only).

## Functional requirements

1. `journey.md ## Scales` is an 8-row table with columns: level, name,
   real-world reference, game extent, v1 representation, key entities,
   domain category. Old 6-row table removed.
2. `journey.md` contains a text drill-down diagram: 8 levels grouped by
   the 4 domain categories, each level annotated with its v1 travel
   state (or "no v1 state" for levels 1 and 8).
3. `journey.md` contains a Level 8 deep-dive subsection with the three
   sub-tiers (shallow 0–100 m / mid-depth 100 m–10 km / deep 10 km+)
   and per-tier v1 ownership.
4. `game/README.md` (intro + pillar), `scope.md` (v1 out),
   `universe.md` (companions, POI-marker origin), `gameplay.md`
   (caves-as-markers link), `milestones/README.md` (level annotations +
   post-v1 candidates), `techstack/README.md` (version bump), and
   `risks/README.md` (subterranean-transition risk) all agree with the
   8 levels.

## Non-functional requirements

- Every DoD criterion checkable by ANALYST with evidence (grep output,
  resolved paths, gate logs) — no "feels complete".
- Every link touched resolves; no doc contradicts another (AGENTS.md
  docs-maintenance rule).

## Definition of Done

- [x] journey.md has exactly the 8 level rows with all agreed columns;
  the old 6-row table is gone.
- [x] Drill-down diagram present: levels ↔ domains ↔ v1 states.
- [x] Level 8 deep-dive subsection with the 3 sub-tiers and v1
  ownership per tier.
- [x] All 6 satellite docs + techstack version + risks line consistent
  (stale-ref grep empty).
- [x] Techstack version bumped per AGENTS.md.
- [x] Every touched link resolves.
- [x] Notion + plan complete with Roles / sign-off rows filled.

## Constraints & Assumptions

- Docs-only: `cargo` gates must stay green but no code or tests change;
  E2E/save round-trip are out-of-scope with reason (no behavior change).
- Parent/child relations are implicit via level order + the diagram
  (Markdown has no relational roll-ups).
- `journey.md`'s "compressed, game-feel distances" rule is preserved via
  the reference-vs-game-extent column split.
- Status vocabulary per `plans/README.md`; `done` only with all DoD
  rows evidenced.

## Open questions

- None blocking. Resolved during design: no ADR needed (ARCHITECT:
  no boundary, lock, or invariant touched); ANALYST/SECURITY sign-off
  follows the `debug-ui-reorganize` precedent (audit + attack-surface
  review, docs-only → E2E n-a with reason).
