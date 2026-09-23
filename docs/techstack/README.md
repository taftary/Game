# Tech stack — how PlanetCrafter is built

- **Version:** 0.53.0 (2026-09-23, v0.3.4 `cosmic-hub-compact-cores`
  done: px-capped hub cores (pin 4 / A 10 / B 6 px in-shader + CPU
  mirror), members `150+250·l`/`30+40·l` emissive `2+2·l` α 0.95 ≤ 100k,
  world halos retired for conditional suffusion (`1.0·r_vir` α 0.04,
  ≥ 12 px), bloom inputs tiered (FR4 members deviation recorded);
  UHD 620 slab core 8 px / blazing A 5 CPU in-slice / halo 1.17× /
  95 dots + 5 Tier A crops, demo spawn 7 px + 262 dots / approach 10 px;
  ANALYST audit + SECURITY review recorded in `plan.md`; single `done`
  commit on branch `v0.3.4`; prior `cosmic-rebase-async` done
  (`246a979`), `cosmic-void-contrast` done (`19558c4`))
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
