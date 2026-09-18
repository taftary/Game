# Issue report — settings-seed-loader / issue-2026-09-18-2248-loader-topmost-stay-settings (UTC)

Specs: [`specs.md`](specs.md)

## Status

`in-review` (investigated 2026-09-18; fix direction below)

## Root cause

1. **Z-order (confirmed).** `ui_items_to_vertices`
   (`crates/debug/src/main.rs:3107`) batches a buffer as
   solids → tris → text quads, and the record loop draws the solid
   prefix first (`use_tex: 0.0`) and the text suffix after
   (`use_tex: 1.0`) — see the `SAFETY: solids are the first
   N vertices` comment at the draw site. Painter order *inside* one
   buffer therefore cannot put a solid (loader dim/panel/track)
   above text (Settings rows). The loader was appended to the main
   `items` buffer, so every Controls label won. The Dimensions
   dropdown solved the identical problem with buffer isolation: the
   `drop` buffer is uploaded separately and drawn after all other UI
   ("own buffer, drawn after every other UI surface … topmost by
   command order"), with the
   `overlay_compose_isolates_dropdown_in_own_buffer` regression test
   pinning it.
2. **Screen jump (confirmed, by design).** `exec_load_step` runs
   `select_screen(Screen::Dimensions(WaypointId::MilkyWay))` in the
   `Finalize` step, inherited from the old instant
   `load_galaxy_seed`. PO reverses this: loads never switch screens.

## Fix direction (TECHLEAD-reviewed)

- Compose the loader into the `drop` buffer (topmost by command
  order) instead of `items`; extend the dim from the viewport rect
  to the full window so nav bar + docks dim too. Panel stays
  viewport-centered (no layout change). No pipeline/record-loop
  change: the drop buffer already draws solids-then-text, which is
  internally correct for the modal.
- Delete the `select_screen` call in `Finalize` (keep `trigger_fade`
  + `finish_load` notify). Nothing else depended on the jump: `R`
  on Milky Way is already there; `SystemMapView::load` clears
  `transit`/`travel_offer`; boot keeps its comment updated (stays
  on Game Demo, which shows the seeded web).
- Tests: extend `loader_modal_overlays_seed_bar_and_step` to pin
  drop-buffer isolation (modal in `drop`, absent from `items`) and
  the full-window dim; existing dropdown isolation test guards the
  shared buffer.
- Docs: `docs/game/controls.md` stay-in-place wording. Parent
  notion/`plan.md` untouched (§6 cross-links only).

## Blast radius

`compose_overlay_ui` loader block only + `Finalize` step + boot
comment + one test + `controls.md`. No engine/`game` touch, no
pipeline change, no new input surface (the seed field parsing is
unchanged and already `u64`-gated).
