# Milestones (suggested order; each is a `plans/` feature)

1. **M0 — Scaffold repair (done 2026-09-14):** `docs/examples` removed from workspace; `viewer` example dropped, smoke moves to `tools`. Workspace must build green on Linux/Win/mac.
2. **M1 — Renderer smoke + tiers:** `vulkano` boot in `tools` (Instance → Surface → swapchain), seeded sphere planet, Low/Med/High tiers, orbit camera. Written against the Vulkan 1.1 device floor (ADR-007 in [`../decisions/`](../decisions/)); CI guards mobile compilation from here (see [`../techstack/quality.md`](../techstack/quality.md)).
3. **M2 — Descent slice:** orbit → atmosphere → sky → soil state machine with fades + chunk streaming stub.
4. **M3 — Surface walk:** character/rover controller + heightfield collision on one planet.
5. **M4 — Colonies + robots core:** place 3 buildings, spawn robots, extract + haul loop, save/load.
6. **M5 — Universe v1:** galaxy/system maps, 2+ planet types, travel, landing-site selection.
7. **M6 — Mobile hardening:** touch controls, dynamic resolution, suspend/resume, perf budgets on reference devices.
8. **M7 — Content + polish slice:** POIs, hazards, codex, audio minimums, English UI lock.
9. **M8 — VR prep audit:** stereo-readiness, input abstraction, UI world-space review — no VR implementation.

Each milestone gets `plans/<name>/notion.md` (needs + Status + DoD) then `plan.md` per `plans/README.md`. No implementation from notion alone.

*Next step: verify green `cargo build --workspace` + `cargo test --workspace --all-targets`, then start M1 (`tools` renderer smoke) via `plans/` notion → plan.*
