# Milestones (suggested order; each is a `plans/` feature)

1. **M0 — Scaffold repair (done 2026-09-14):** `docs/examples` removed from workspace; `viewer` example dropped, smoke moves to `tools`. Workspace must build green on Linux/Win/mac.
2. **M1 — Renderer smoke + tiers (done 2026-09-14, `plans/renderer-smoke`):** `vulkano` boot in `tools` (Instance → Surface → swapchain, capped at Vulkan 1.1 + portability enumeration), seeded sphere planet (`engine::render::SeededPlanet`, tier N=3/4/6), Low/Med/High tiers as code, orbit camera, naga-compiled planet pipeline, `--headless` CI gate. Written against the Vulkan 1.1 device floor (ADR-007 in [`../decisions/`](../decisions/)); CI guards mobile compilation from here (see [`../techstack/quality.md`](../techstack/quality.md)).
3. **M2 — Descent slice:** orbit → atmosphere → sky → soil state machine with fades + chunk streaming stub + landing-site selection ([`journey.md`](../game/journey.md) levels L5–L7 transition; site selection moved here from M5 by ADR-011 in [`../decisions/`](../decisions/)).
4. **M3 — Surface walk:** character/rover controller + heightfield collision on one planet (L7).
5. **M4 — Colonies + robots core:** place 3 buildings, spawn robots, extract + haul loop, save/load.
6. **M5 — Universe v1 (executes next, ahead of M2–M4 — ADR-011 in [`../decisions/`](../decisions/)):** galaxy/system maps, 2+ planet types, travel ([`journey.md`](../game/journey.md) levels L1–L4; L1 backdrop-only, L4 companions visual-only; landing-site selection moved to M2).
7. **M6 — Mobile hardening:** touch controls, dynamic resolution, suspend/resume, perf budgets on reference devices.
8. **M7 — Content + polish slice:** POIs, hazards, codex, audio minimums, English UI lock.
9. **M8 — VR prep audit:** stereo-readiness, input abstraction, UI world-space review — no VR implementation.

Each milestone gets `plans/<name>/notion.md` (needs + Status + DoD) then `plan.md` per `plans/README.md`. No implementation from notion alone.

*Next step: start M5 content (universe maps: galaxy/system maps + travel + planet types) via `plans/universe-maps/` notion → plan — re-sequenced ahead of M2–M4 by ADR-011 in [`../decisions/`](../decisions/). Milestone numbers are labels, not sequence: execution order is M0, M1, M5, M2, M3, M4, M6, M7, M8. The `debug-sphere-viewer` (implemented 2026-09-14, `plans/debug-sphere-viewer`) consumes `engine::render` (orbit camera + planet triangulation).*

Post-v1 candidates (unscheduled, no milestone number): subterranean
geometry ([`journey.md`](../game/journey.md) L8), traversable universe
layer (L1), landable moons (L4).
