# Notion — cosmic-frame-timing

## Status

`done` (PO sign-off 2026-09-23; UX consulted — n-a, dev-only
overlay in the debug shell, no player-facing surface; ARCHITECT
consulted — new `QueryPool` GPU resource, `unsafe` timestamp writes,
CAP-001 seam signature touch; DEV CFT-001..008 done 2026-09-23;
ANALYST audit + SECURITY review recorded in `plan.md` DoD table;
single `done` commit on branch `v0.3.5`)

## Context

The debug viewer runs hot on Demo + Inspector on Intel UHD 620, and
every cosmic grading round to date tuned blind: no GPU timing exists
anywhere (`plans/v0.3.4/cosmic-gpu-tracers/plan.md` step 5 admits
frame-ms was "not isolable"). `FpsOverlay` records wall-clock loop
interval only (`main.rs:8291`) — CPU + GPU + vsync lumped together.
Nobody can say whether the 8.58M splat invocations, the 5-level bloom
pyramid, the 48-step march, or the per-frame CPU uploads own the
frame. ADR-027 therefore makes measurement a prerequisite: **no
cosmic grade/perf change ships without a per-pass GPU ms line**.

A second, independent tax: `cargo run -p game_debug` builds
`game_debug` + `game_engine` at opt-level 0 (`Cargo.toml`
`default-members` excludes `crates/debug`; `[profile.dev.package."*"]`
covers external deps only). The per-frame UI relayout, text shaping,
buffer uploads and descriptor-set builds all run unoptimized.

## Problem & Needs

- **Blind grading:** without per-pass GPU ms, every optimization
  proposal (culling, bricks, fill levers, post trims) is argued from
  analytic bounds, never measured on the reference GPU.
- **Lumped wall clock:** the FPS widget cannot distinguish a
  GPU-bound cosmic frame from an opt-level-0 CPU frame or vsync
  quantization.
- **No baseline:** v0.3.5's follow-ups need a recorded High-tier
  baseline on the UHD 620 to compare against.

## Goals

1. **Per-pass GPU timing in the FPS widget.** Timestamp each cosmic
   pass — HDR prepass (scene glow + procedural splats), bloom
   pyramid, veil march, main pass (resolve + 3D + UI) — as rolling
   avg + max over the existing ring-buffer shape, beside the current
   fps/avg/max numbers.
2. **CPU phase timers in the same place.** UI build, `cosmic_frame`
   precompute, command recording — `Instant` spans, same ring shape.
3. **Capture log line.** `--capture` prints
   `cosmic_timing=prepass… bloom… march… main… total…` (GPU when a
   pool exists, `n/a` offscreen) plus the CPU phases, next to the
   existing `cosmic_proc=` line.
4. **Zero cost when hidden.** No query readback unless the FPS widget
   is visible or a capture runs; timestamp writes themselves are ~µs.
5. **Dev-profile hygiene.** `game_debug` + `game_engine` build at
   opt-level 1 under `cargo run`; `quality.md` records that budgets
   are measured with `--release`.
6. **Recorded baseline.** UHD 620, seed 1337, 1280×720, Demo +
   Inspector, High tier (dev + release) in the plan's DoD table.

## Non-goals

- No tier change, no shader change, no culling, no clamp change —
  measurement only (optimization is F2 and the deferred follow-ups).
- No change to `FpsOverlay` semantics (wall-clock stays as-is).
- No GPU timing in `--headless` (GPU-free by contract) or in the
  `game` / `tools` binaries.
- No new dependency.

## Users / Stakeholders

- DEV/ANALYST: per-pass numbers to aim F2 and the culling/bricks/fill
  follow-ups at, and to verify them with.
- PO: the baseline row every later v0.3.5 DoD compares against.

## Roles

Author: PO. UX consulted (required if player-facing): n-a —
dev-only debug overlay. ARCHITECT consulted (required if
cross-module): yes — new `QueryPool` resource owned by the bin,
`unsafe` timestamp writes, CAP-001 seam functions gain an optional
timer param.

## Functional requirements

- FR1 (slots): four GPU slots — `Prepass` (scene glow + splats),
  `Bloom`, `March`, `Main` — each bracketed by two timestamps in the
  same command buffer, written between render passes (never inside
  one).
- FR2 (pool): one `QueryPool` (`Timestamp`, `2 × frames_in_flight ×
  slots × 2` queries), reset per frame, results read one frame late
  (never waited on in the windowed loop).
- FR3 (widget): FPS tab gains "GPU passes" + "CPU phases" sections
  (avg + max per row, colour by the existing 20/34 ms thresholds);
  rows show `n/a` when the device reports no timestamp support.
- FR4 (capture): the `--capture` path logs the `cosmic_timing=` line;
  the offscreen path (which already fence-waits) reads its own pool
  with wait and reports real GPU ms.
- FR5 (gating): readback (`get_results`) runs only when the FPS widget
  is visible or a capture runs; timestamp *writes* always run (cost
  ~µs, keeps the code path identical).
- FR6 (build): `[profile.dev.package.game_debug]` and
  `[profile.dev.package.game_engine]` set `opt-level = 1`;
  `quality.md` notes budgets are measured with `--release`.

## Non-functional requirements

- NFR1 (invariants): write-once bloom untouched; no render-pass
  nesting change; no pipeline/layout change; capture byte-identity
  at `--tier high` preserved (timestamps don't alter draws).
- NFR2 (headless): `--headless` never touches the Vulkan loader; the
  timing module is GPU-free and unit-tested.
- NFR3 (overhead): pool + writes add no measurable frame cost;
  widget-hidden frames do zero host readback.
- NFR4 (robustness): devices without
  `timestamp_compute_and_graphics` (or period 0) run exactly as
  before, widget shows `n/a`.

## Definition of Done

1. Widget shows GPU ms per pass (avg + max) on the reference box.
2. Widget shows CPU phase ms (avg + max) on the reference box.
3. `--capture` logs a `cosmic_timing=` line (GPU ms windowed...
  offscreen path reports its own waited results).
4. Widget-hidden frames perform zero query readback (code-gated,
   test-pinned where unit-testable).
5. Dev-profile opt-levels set; `quality.md` documents `--release`
   for budgets.
6. Baseline table recorded (UHD 620, seed 1337, 1280×720,
   Demo + Inspector, High, dev + release).
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- vulkano 0.35 `write_timestamp` / `reset_query_pool` are `unsafe`:
  reset-before-write invariant documented at the call site; no new
  `unsafe` elsewhere.
- Assumes the reference box exposes timestamp queries (Intel UHD 620
  does); NFR4 covers the contrary.
- Timestamp writes between passes are valid inside and outside
  render passes in Vulkan; ours sit between `end_render_pass` and
  `begin_render_pass`.

## Open questions

- Should the pool also bracket the UI sub-draws inside Main? PO
  decision: no — Main is one slot; split later only if A shows Main
  dominating without 3D cause.
