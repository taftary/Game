# Issue specs — cosmic-device-tier / issue-2026-09-24-0944-low-tier-fps-unstable (UTC)

Parent feature: [`../notion.md`](../notion.md)

Follow-up of: [`../issue-2026-09-24-0856-high-to-low-stale-frame/specs.md`](../issue-2026-09-24-0856-high-to-low-stale-frame/specs.md)
(that issue's ghost is confirmed gone by the operator; the remaining
symptom below is load, not a stale image).

## Status

`done` (report: [`report.md`](report.md), fix plan: [`plan.md`](plan.md) — all fix todos checked, operator confirms fps better)

## Roles

Reported by (role): operator (windowed `game_debug` session, Low tier
after `F4` cycle) · Priority set by PO: _(pending)_

## Observed vs Expected

Observed: in Low tier (`tier: low (manual) [F4]`) the frame rate is
unstable on both cosmic surfaces (inspector + demo). Widget reads:
fps 54.0, frame 18.5 ms avg · 38.6 ms max (240 samples); GPU passes
avg · max: prepass 15.2 · 21.3, bloom 1.2 · 4.0, march 0.0 · 2.0,
main 0.5 · 0.7. Sparkline shows repeated yellow/red spikes. Slab (`S`)
reported as "not working well" (exact behavior still open — see below).

Expected: Low tier holds a stable frame with headroom under vsync on
the operator's box, or is documented as inspector-overdraw-bound with
a prescribed fallback (Medium).

## Reproduction steps

1. `cargo run -p game_debug` on an HDR-capable GPU.
2. Open the Cosmic Web tab (inspector) or the Game Demo tab.
3. `F4` until `tier: low (manual)`.
4. Watch the FPS-widget sparkline + FRAME HEALTH + GPU PASSES rows.

## Scope & Impact

- Debug shell only, cosmic views only, HDR mode. No `engine` / `game` /
  `tools`, no save format.
- Precedent: `cosmic-device-tier` DoD 5 already measured Low inspector
  *slower* than Medium on UHD 620 (prepass 40.9 vs 32.7 — "sprites
  overdraw ≈ march saving on inspector, recorded, not hidden"). If the
  operator's box agrees (Low prepass ≥ Medium prepass), this is that
  known overdraw, and the fix belongs to the queued post-v0.3.5 work
  (`cosmic-splat-culling`, `cosmic-splat-bricks`, `cosmic-fill-levers`).
- The 38.6 ms frame max vs 21.3 ms prepass max means the worst frames
  come from outside the timestamped GPU passes (CPU phases, present, or
  the `F4` rebuild frame lingering in the 240-sample max window).

## Logs / Evidence

Operator readings (Low, manual):

- `fps: 54.0`, `frame: 18.5 ms avg · 38.6 ms max`, `samples: 240`.
- `prepass 15.2 · 21.3`, `bloom 1.2 · 4.0`, `march 0.0 · 2.0`,
  `main 0.5 · 0.7`.
- March 0.0 avg confirms the stale-frame fix behaves (clear ~free);
  the 2.0 max is consistent with the `F4` rebuild frame in the window.
- Swapchain uses vsync Fifo (`main.rs:8738` `..Default::default()`),
  so 60 Hz frames quantize to 16.7/33.3 ms: a 15.2 ms prepass leaves
  ~1.5 ms headroom and any jitter becomes a 33 ms red spike.
- Still open (operator): GPU model, window size, release vs dev build;
  same GPU rows on Medium and High (same view); scrolled-up CPU rows
  (ui / cosmic / record); what `S` does on which tab
  (`S` is inspector-only by construction — `main.rs:9750`,
  `main.rs:10089`; in Game Demo `S` is thrust).

## Suspected area

Low sprites-veil overdraw on the prepass (212k stacked additive
sprites inspector-wide) + vsync-miss amplification. Ruled out as the
spike source: the stale-frame fix (one empty quarter-res clear + one
float select per frame — March row reads 0.0 avg).
