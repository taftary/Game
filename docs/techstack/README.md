# Tech stack — how PlanetCrafter is built

- **Version:** 0.54.0 (2026-09-23, v0.3.4 `cosmic-vista-reframe`
  done: interior-window headline — `vista`/`slab` captures pose from
  the same `vista_pose` eye/target (slab keeps 20° + 30 Mpc at the hub
  depth), FR1 16:9 shrink fallback before legacy, no limb in frame
  (radial scans vista 0.181 / slab 0.152 ≤ 0.20); six readings 5 met /
  1 partial (depth cue partial — thin opening slice); nominal dive
  28.4°/s, focal NDC (0.096, 0.000), captures byte-identical ×2
  (vista `450450D9…` 1.52 MB, slab `714E9837…` 0.60 MB);
  ANALYST audit + SECURITY review recorded in `plan.md`; single `done`
  commit on branch `v0.3.4`, version closes (merge `v0.3.4 → main`);
  prior `cosmic-hub-compact-cores` done)
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
