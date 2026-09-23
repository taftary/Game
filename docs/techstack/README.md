# Tech stack — how PlanetCrafter is built

- **Version:** 0.52.0 (2026-09-23, v0.3.4 `cosmic-rebase-async`
  done: rebase never blocks the frame — split seed/rebase rebuilds,
  glow-only off-thread worker (CGT-010 follow-through) with latest-wins
  + join on drop; UHD 620 swap upload 22 194 pts / 0.8 MB in 1.50–2.11 ms
  (no two-frame split), headless traverse 507 Mpc / 10 rebases / max tick
  0.47 ms; ANALYST audit + SECURITY review recorded in `plan.md`;
  completion commit on branch `v0.3.4`, deviation recorded: progress
  commit `529819e` + CGT-010 changes preceded it; prior `cosmic-void-contrast`
  done per its plan, commit `19558c4`)
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
