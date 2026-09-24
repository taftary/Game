# Plan — fps-widget-scroll

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only (lib `app.rs` + `ui.rs`, bin
`main.rs`). No `engine` / `game` / `tools` diff. New `App` field
`fps_scroll: f32` is plain UI offset state (same ownership class as
`widget_focused`); wheel routing gains a widget-first branch before
the existing `in_viewport` zoom; `build_fps_tab` becomes a
sticky-header + virtualized scroll region with cull-on-emit (the
Console tab's capacity-clip precedent, extended to partial rows +
plot bars). **Invariant rows:** projection un-flipped, CCW
front-face, ENU frame, picking NDC, bloom write-once, capture
byte-identity — all untouched (no draw, pipeline, layout, or pass
change; UI solids/text only). No new dependency, no new `unsafe`,
no save/migration touch. **Phase order is dependency-safe:**
pure math + state first (unit-testable, headless-safe), builder
second, input routing third, docs/gates last.

### Phase 1 — State + pure math (FPS-001, FPS-002)

`App::fps_scroll` + clamp/scroll helpers (`app.rs`); content-height,
scroll-max, scrollbar-thumb helpers (`ui.rs`). Risk-first: every
later todo depends on these numbers agreeing with the builder.

### Phase 2 — Virtualized builder (FPS-003, FPS-004)

Sticky glance header + culled scroll region + compact 72 px plot +
scrollbar track/thumb/hint in `main.rs`. Culling rule: emit an item
iff its rect intersects the visible region (plot bars culled
individually).

### Phase 3 — Input routing (FPS-005)

Widget-first wheel branch (Fps tab only); viewport zoom path
byte-identical otherwise.

### Phase 4 — Tests + docs + gates (FPS-006, FPS-007)

Unit + headless builder tests, milestones/techstack docs, full gates,
audit, review.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| ID | Status | Task | Ref notion § |
| FPS-001 | done | `app.rs`: `fps_scroll: f32` field (default 0) + `clamp_scroll_offset` / `scroll_fps_by(delta, max)` / `clamp_fps_scroll(max)` helpers; unit test `fps_scroll_clamps_into_range` (negative→0, overshoot→max, NaN→0, degenerate max→0) | FR2, NFR4 |
| FPS-002 | done | `ui.rs`: `FPS_PLOT_H = 72`, `FPS_SCROLLBAR_W/GUTTER`, `fps_sticky_height(lh)` (section + 5 rows incl. always-on GPU total), `fps_scroll_view_height(body_h, lh)`, `fps_scroll_content_height(lh)`, `fps_scroll_max` (inputs clamped, never negative), `fps_scrollbar` thumb (min 16 px, parks top/bottom), `fps_wheel_delta_px` (3 rows/notch, up = toward header, NaN-safe); `PanelRows::cursor_y`; unit test `fps_scroll_geometry_clamps_and_pins_layout` incl. R-1 builder/helper agreement pin | FR1, FR2, FR5, FR6, NFR4 |
| FPS-003 | done | `main.rs` `build_fps_tab`: sticky header (section + fps/frame/samples/tier/total, `n/a` fallback) + scroll region (hint + GPU section/rows + CPU section/rows + footer + 72px plot, all 120 samples kept) with intersect-culling incl. per-bar plot culling; `intersect_rect` helper; `build_widget_fps` passes `app.fps_scroll` | FR1, FR2, FR3, FR6 |
| FPS-004 | done | Scrollbar track (`C_TRACK`) + thumb (`C_KNOB`, via `fps_scrollbar`) + `wheel: scroll · GPU/CPU detail` hint row at the top of the scroll column | FR5 |
| FPS-005 | done | `MouseWheel` widget-first branch: cursor over widget + visible + Fps tab → `scroll_fps_by(fps_wheel_delta_px(notches, lh), max)` (max from the same helpers the builder uses), `update_hover`, return; viewport zoom path byte-identical otherwise | FR4 |
| FPS-006 | done | Tests: lib `ui`/`app` unit tests + bin `fps_widget_scroll_reaches_tail` (scroll-0 header + sweep proves every section surfaces + max shows footer/plot) and `fps_widget_scroll_never_bleeds` (all solids/texts inside widget at 0/mid/max); existing widget-fps needle test updated for scroll-0 (tail asserted culled, not missing) | DoD 1–4 |
| FPS-007 | done (code+docs+gates+audit+review; single `done` commit pending user go-ahead — no commit without explicit request) | Docs (`docs/milestones/README.md` v0.3.5 F3 `in-progress` row, techstack bump 0.56.0→0.57.0) + full gates + ANALYST audit + SECURITY review below | DoD 5 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-24 — debug-crate-local,
Console-clip precedent, invariant rows above, no ADR needed) ·
Todos approved by: TECHLEAD (2026-09-24 — risk-first: state/math
before builder before routing; budgets checked — O(rows) rect math,
no GPU/readback change; gates per `quality.md`) · UX acceptance
rows: n-a (dev-only overlay) · DoD verified by: ANALYST (2026-09-24 — per-row audit below)
· Security reviewed by: SECURITY (2026-09-24 — pass, no findings;
review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Every row + sparkline readable via scroll | done | Bin `fps_widget_scroll_reaches_tail`: sweep 0..=max surfaces `prepass`, `CPU PHASES`, `last 120 frames`; max offset shows the 72 px plot bg (culled at scroll 0). All 120 samples still drawn (`recent_ms(FPS_SPARKLINE)` path untouched). | DEV 2026-09-24; ANALYST 2026-09-24 (pass — reproduced in bin suite 48/48) |
| 2 | Sticky glance header at scroll 0 | done | Scroll-0 view contains `FRAME HEALTH`, `fps:`, `samples:`, `total`, `GPU PASSES`, `wheel: scroll`; sweep asserts `FRAME HEALTH` at every step. GPU total always emitted (`n/a` fallback, no conditional row → layout stable). | DEV 2026-09-24; ANALYST 2026-09-24 (pass) |
| 3 | No bleed outside body at any offset | done | Bin `fps_widget_scroll_never_bleeds`: every solid inside the 400×280 widget rect and every text run inside it at scroll 0/mid/max (UI pass has no scissor; cull-by-non-emission + per-bar intersection). | DEV 2026-09-24; ANALYST 2026-09-24 (pass) |
| 4 | Wheel scroll vs viewport zoom, no regression | done | Widget-first branch keyed on `widget.contains(cursor)` + visible + Fps tab; viewport branch byte-identical (only moved below the new branch). Wheel math pinned by lib `fps_scroll_geometry_clamps_and_pins_layout` (`fps_wheel_delta_px` sign/NaN). Full bin suite 48/48 + lib 299/299 green. | DEV 2026-09-24; ANALYST 2026-09-24 (pass) |
| 5 | Gates + audit + review + one commit | in-review (gates+audit+review done; commit pending) | `fmt --check`, `clippy --workspace --all-targets --all-features -D warnings`, `build --workspace`, `test --workspace --all-targets` (40+299+48+311+3+5 green), `test --doc` (6+85 green), `game` run, `game_debug --headless`, `game_tools --headless --tier low`, mobile guards (android + ios `cargo check`) — all green 2026-09-24. ANALYST + SECURITY signed below; single `done` commit awaits explicit user request (never committed without asking). | DEV 2026-09-24; ANALYST 2026-09-24; SECURITY 2026-09-24 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Projection/winding/ENU/picking untouched (no camera/projection/marker diff) [R: diff review].
- A-2. Bloom write-once description unchanged [T: existing pin tests].
- A-3. Capture byte-identity at High unchanged (no draw/state change) [T: capture gate ×2 if GPU box; else headless + diff review].
- A-4. `--headless` loads no Vulkan symbols [T: existing headless run].
- A-5. No new `unsafe`, no new dependency, no save-format touch [R: SECURITY review].

UX acceptance rows (affordance, dev-only): scrollbar thumb visible
whenever content overflows; `wheel: scroll` hint on the first
screen; sticky header never scrolls away.

## ANALYST audit note (2026-09-24)

- Re-ran: full gate list above on the working tree (bin 48/48 incl.
  2 new scroll tests + updated chrome test; lib 299/299 incl. 2 new
  scroll tests; engine 311; game 40; tools 5; doc 91).
- DoD 1: sweep test proves reachability headlessly; interactive
  wheel feel (notch step = 3 rows) left to the operator's `F6` check
  on their box (limitation L-1, same class as F1's).
- DoD 3: bleed test covers 1280×720 at 0/mid/max; smaller windows
  clamp via `fps_scroll_max`/`cursor` guards (unit-pinned).
- E2E: n-a (debug-only tooling, no player journey, no save format).
- Verdict: all rows pass with L-1 noted. No findings filed.

## SECURITY review note (2026-09-24)

- New input surfaces: wheel-over-widget (cursor-gated offset math,
  NaN-safe, clamped — no parse, no allocation beyond existing rows).
- New dependencies: none. New `unsafe`: none. Save/migration:
  untouched. `App::fps_scroll` is plain `f32` UI state, never
  serialized.
- Verdict: pass, no findings.

## Risks & Next steps

- R-1 (builder/helper drift): content-height helper and builder must
  use the same row sequence — mitigate by one shared section/row
  order + unit test asserting helper matches a headless build count.
- R-2 (wheel-vs-zoom fight): widget-first branch keyed on
  `widget.contains(cursor)` + Fps tab; viewport path untouched.
- R-3 (partial-plot bleed): plot bars culled individually against the
  visible region, never just the plot background.
- Next: scrollbar drag as a follow-up update if operators want it;
  per-row collapse only if content grows again.
