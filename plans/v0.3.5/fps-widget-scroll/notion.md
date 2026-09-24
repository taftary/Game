# Notion — fps-widget-scroll

## Status

`in-review` (PO sign-off 2026-09-24; UX consulted — n-a, dev-only
overlay in the debug shell, no player-facing surface; ARCHITECT
consulted — `App` state + input routing + UI builder boundary,
no render pipeline change; DEV FPS-001..007 done 2026-09-24;
ANALYST audit + SECURITY review recorded in `plan.md` DoD table;
single `done` commit pending — not committed without explicit user
request, per repo rule)

## Context

The dev widget (`crates/debug`, 400×280 fixed overlay) gained two
timing sections in `cosmic-frame-timing` (v0.3.5 F1: four GPU pass
rows + three CPU phase rows) on top of the existing frame-health
rows, tier line, and 120 px sparkline. The widget body is ~244 px
tall; the FPS tab now wants ~430+ px. The tail of the tab (CPU
rows, footer, sparkline) draws past the widget background and is
not reachable: no scroll, no clip exists (the Console tab avoids
this by dropping lines to fit; the FPS tab has no equivalent).

## Problem & Needs

- **Unreachable data:** rows past the body edge are drawn outside
  the widget and cannot be read; there is no way to scroll.
- **Glanceability lost:** the numbers an operator opens the widget
  for (fps, frame avg/max, GPU total, sparkline shape) no longer
  fit on one screen together with the new detail rows.
- **Representation debt:** the tab lays out every row unconditionally
  into a fixed overlay with no overflow policy, so any future row
  re-breaks it.

## Goals

1. **Every FPS row reachable.** All GPU/CPU rows, the footer, and
   the sparkline are readable inside the widget via wheel scroll.
2. **Glanceables stay put.** fps, frame avg/max, tier, and GPU total
   are visible at scroll 0 without interaction.
3. **Nothing bleeds.** No emitted text/solid lands outside the widget
   body rect at any scroll offset (the UI pass has no scissor; the
   builder must cull, like the Console tab clips by construction).
4. **No behavior change elsewhere.** Timing semantics, capture log
   line, viewport wheel zoom, and all other tabs unchanged.

## Non-goals

- No change to `FpsOverlay` / `FrameTiming` semantics (rings, avg/max,
  `n/a` states stay as-is).
- No GPU scissor / pipeline / render-pass change; no bloom, projection,
  winding, or picking touch.
- No scrollbar-drag interaction in v1 (wheel-only; drag is a follow-up).
- No widget resize / move; the 400×280 chrome contract holds.
- No player-facing surface; nothing enters the release binary.

## Users / Stakeholders

- DEV/ANALYST: per-pass numbers must be readable on the reference box
  to aim and verify the deferred culling/bricks/fill follow-ups.
- PO: the v0.3.5 performance story stays usable (measurement you cannot
  read is not measurement).

## Roles

Author: PO. UX consulted (required if player-facing): n-a —
dev-only debug overlay (`docs/roles/ux.md` exemption; scrollbar +
wheel-hint affordance noted anyway). ARCHITECT consulted (required
if cross-module): yes — new `App` scroll state, wheel-routing
precedence, UI builder virtualization; no engine/game/tools diff.

## Functional requirements

- FR1 (sticky header): at any scroll offset the body top shows
  FRAME HEALTH + `fps` + `frame avg·max` + `samples` + `tier` +
  GPU `total` line.
- FR2 (scroll region): GPU pass rows (4), CPU phase rows (3),
  footer, and sparkline live in a virtual column taller than the
  remaining body; wheel over the widget moves a clamped px offset.
- FR3 (culling): rows/solids/bars fully outside the visible region
  are never emitted (no bleed past the body rect).
- FR4 (wheel precedence): wheel with the cursor over the widget body
  scrolls the FPS tab (Fps tab only) instead of zooming the viewport
  camera; wheel elsewhere behaves exactly as before.
- FR5 (scrollbar): a visible thumb + `wheel: scroll` hint show that
  more content exists and where the offset sits.
- FR6 (compact plot): the sparkline keeps all 120 samples but draws
  at a reduced height so header + first detail rows share the first
  screen with it reachable one scroll away.

## Non-functional requirements

- NFR1 (invariants): projection, winding, ENU frame, picking, bloom
  write-once, capture byte-identity all untouched (UI-builder-local
  change only).
- NFR2 (headless): `--headless` path untouched; new math is pure and
  unit-tested.
- NFR3 (overhead): scroll adds O(rows) rect math per frame; zero host
  readback change (timing gating from F1 holds).
- NFR4 (robustness): degenerate sizes clamp to zero, never negative;
  scroll offset clamps `0..=max(0, content - view)` on every frame.

## Definition of Done

1. Every FPS row + sparkline readable inside the widget via wheel
   scroll on the reference window (1280×720).
2. Sticky glance header (fps / frame / tier / GPU total) visible at
   scroll 0.
3. No emitted item lands outside the widget body at any offset
   (test-pinned).
4. Wheel over widget scrolls; wheel over viewport still zooms
   (no regression, test-pinned where unit-testable).
5. Full gate list green; ANALYST + SECURITY signed; single `done`
   commit on branch `v0.3.5`.

## Constraints & Assumptions

- `UiItems` has no scissor: culling by non-emission is the only clip
  mechanism (Console-tab precedent).
- Assumes 1280×720 reference window for DoD screenshots/tests; body
  math clamps for smaller windows.
- Wheel mapping follows the existing `MouseScrollDelta` convention
  (`LineDelta` notches, `PixelDelta / 50`).

## Open questions

- Notch step (rows per wheel notch) tuned by feel on the operator box.
- Scrollbar drag deferred — confirm wheel-only is acceptable for v1.
