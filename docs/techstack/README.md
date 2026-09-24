# Tech stack — how PlanetCrafter is built

- **Version:** 0.57.2 (2026-09-24, v0.3.5
  issue-2026-09-24-0944-low-tier-fps-unstable fix: exact-zero
  degenerate cull in `GLOW_VERT` / `SPLAT_PROC_VERT` — zero-adding
  points clip before rasterization instead of paying full quads, zero
  visual change; prior 0.57.1
  issue-2026-09-24-0856-high-to-low-stale-frame fix: Sprites mode
  records a black march-target clear + resolves march at gain 0, so
  `F4` High → Low no longer ghosts the previous tier's march over the
  Low scene; prior `fps-widget-scroll` in-progress: sticky glance
  header + wheel-scrollable GPU/CPU detail + 72 px sparkline in the
  fixed 400×280 FPS widget, cull-by-non-emission, scrollbar + hint;
  prior `cosmic-device-tier` done: boot tier from device
  (`IntegratedGpu` → Medium proven live on UHD 620), one tier drives
  splat_k/veil/bloom, `F4` cycle, `--tier` capture pin (default High,
  byte-identical `A5AA81…`); measured UHD 620 (seed 1337, 1408×768):
  inspector High 124.5 → Medium 38.6 ms (3.2×), demo High 195.9 →
  Medium 59.3 ms (3.3×), Low 44.3 (sprites overdraw ≈ march saving
  on inspector))
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
