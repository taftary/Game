# Notion — log-depth-rendering

## Status

`done` (all DoD criteria checked in `plan.md`, ANALYST + SECURITY signed 2026-09-17)

## Context

Milestone v0.2.0 (scale rendering & visuals), spec
[`cosmic-navigation-engine-v0.4.md`](../../../docs/techstack/cosmic-navigation-engine-v0.4.md)
§4. Decided by ADR-016 in [`../../docs/decisions/`](../../../docs/decisions/).
26 decades of scale cannot fit one depth range; log-depth alone is
necessary but not sufficient.

## Problem & Needs

- Standard depth buffers z-fight hopelessly across multi-decade scenes.
- A single global near/far for all scales is impossible; bands must
  composite.

## Goals

1. Log-depth buffer within a draw pass: `log(z)` written in the vertex
   shader (Outerra-style).
2. Multi-pass depth compositing: distant scale bands (cosmic web,
   galaxy, distant stars) render depth-cleared first, skybox-like;
   near-field geometry renders after with tight near/far planes;
   results composite.
3. Per-pair-of-scales decision table for which layers share a depth
   pass — no global depth range.

## Non-goals

- Frame/precision architecture (owned by `frame-hierarchy`).
- Content streaming (owned by `star-catalog-streaming`).
- Atmospheric/environmental shading (owned by `depth-cueing`).

## Users / Stakeholders

- Players: no z-fighting, no pop artifacts across scales.
- Developers: a documented pass structure per scale band.

## Roles

Author: PO (2026-09-17). UX consulted (required if player-facing): n-a at
planned (no UI). ARCHITECT consulted (required if cross-module): yes —
touches the render pipeline contract (`docs/techstack/rendering.md`).
PO sign-off for `draft → planned` (2026-09-17): checklist green — problem
stated without prescribing implementation; non-goals explicit; every DoD
criterion verifiable with evidence; open questions listed as questions
(band boundaries per tier, deferred to plan Phase with ADR-007 data). No
spec §10 open item blocks this feature (visual style, facility anchor,
terrain data policy, network/content policy all deferred per-feature per
v0.2.0 kickoff agreement).

## Functional requirements

- Vertex-shader log-depth variant of the existing pipelines
  (naga-compiled, per `rendering.md`).
- Depth-band pass scheduler: band list per active scale, depth-clear +
  composite order.
- Documented band-sharing table (per pair of scales).

## Non-functional requirements

- Rendering invariants preserved: NDC +1 = top row; RH projection,
  Z ∈ [0, 1], no Y-flip; `FrontFace::CounterClockwise` + `CullMode::Back`
  (`docs/techstack/rendering.md` — flip projection or front-face, never
  both).
- Quality gates + `--headless` smoke stay green.

## Definition of Done

- [ ] Log-depth active with before/after z-fight evidence at a
  multi-decade test scene.
- [ ] Multi-band composite renders correct occlusion across at least 3
  bands (screenshot/headless assert).
- [ ] Band-sharing table documented in `rendering.md`.

## Constraints & Assumptions

- ADR-016 draft becomes binding here.
- Vulkan 1.1 device floor (ADR-007) — no extensions beyond it.

## Open questions

- Exact band boundaries per tier (needs device-tier perf data,
  ADR-007).
