# Tech stack — how PlanetCrafter is built

- **Version:** 0.33.0 (2026-09-18, cosmic-scale player per ADR-023:
  stage-0 generated cosmic web, Game Demo rebuilt around the player
  (marker + Chase/Orbit/FirstPerson camera + fly-to), Cosmic Web
  inspector as the fourth absorbed view, real pill/console feed;
  shell, registry (+1 action), and invariants preserved)
- **Engine decision:** custom Vulkan engine in `crates/engine` (`vulkano`, no `wgpu`)
- **Graphics API:** Vulkan directly via [`vulkano`](https://crates.io/crates/vulkano)
- **Main dependencies:** `vulkano` + `winit` + `naga` + `fontdue` + `glam` + `hecs` + `tracing` (see [`stack.md`](stack.md))

Locked for v1 unless an ADR (see [`../decisions/`](../decisions/)) overturns it.

## Contents

- [`stack.md`](stack.md) — language, renderer, graphics API, windowing, shaders, text, math, and why `vulkano`.
- [`architecture.md`](architecture.md) — scaffold state, workspace layout, module boundaries.
- [`rendering.md`](rendering.md) — Vulkan backend, render passes, quality tiers, renderer smoke.
- [`simulation.md`](simulation.md) — fixed-step tick, physics, game time, determinism.
- [`persistence.md`](persistence.md) — versioned save format and migration.
- [`assets.md`](assets.md) — content pipeline, formats, fonts, hot-reload.
- [`quality.md`](quality.md) — performance budgets, test gates, test policy (canonical gate list).
- [`cosmic-navigation-engine-v0.4.md`](cosmic-navigation-engine-v0.4.md) — adopted navigation-engine technical specification (v0.4): scale hierarchy, frames, rendering, physics, seeding, environment visuals, product decisions. Source of truth for milestones v0.1.0+ ([`../milestones/`](../milestones/)); spec-derived ADRs in [`../decisions/`](../decisions/).
