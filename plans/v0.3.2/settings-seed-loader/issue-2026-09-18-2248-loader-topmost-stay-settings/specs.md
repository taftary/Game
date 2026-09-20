# Issue specs — settings-seed-loader / issue-2026-09-18-2248-loader-topmost-stay-settings (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`done` (fix landed + verified 2026-09-18: TECHLEAD reviewed,
ANALYST DoD-verified, SECURITY passed per `plan.md`; uncommitted on
branch `v0.3.2` — folds into the feature's single commit)

## Roles

Reported by (role): PO (user, windowed smoke test + screenshot).
Priority set by PO: should-fix (blocks the feature commit).

## Observed vs Expected

1. **Loader under Settings text.** During a staged load on the
   Settings screen, the Controls rows' label text renders *over* the
   loader panel (screenshot: `Re-roll galaxy seed [R]`,
   `Top-down snap [Home]`, `Camera preset: …` bleed through
   `LOADING UNIVERSE · seed 1` / `38% · Loading solar system`).
   Expected: the loader is topmost over everything — the same class
   of bug the Dimensions dropdown had (fixed via its own topmost
   buffer).
2. **Screen jump after load.** A load started from Settings lands on
   the Milky Way dimension tab. Expected (PO decision 2026-09-18,
   superseding notion Goals §3): a load stays on the current screen;
   `--seed` boot stays on the Game Demo tab.

## Reproduction steps

1. Run the windowed viewer, `F3` (Settings).
2. Type a seed, click Load (or Enter).
3. While the modal shows: row text bleeds through the panel (defect 1).
4. When the load completes: the shell is on the Milky Way tab, not
   Settings (defect 2).

## Scope & Impact

- Debug shell only (`game_debug` binary); no engine/`game` touch.
- Defect 1 is visual (z-order); the load itself completes correctly.
- Defect 2 changes specified behavior: notion Goals §3 / FR /
  DoD-3 say "lands on the Milky Way tab". This issue records the PO
  reversal; parent files stay untouched per `plans/README.md` §6.

## Logs / Evidence

- User screenshot (Settings screen, 38% · Loading solar system,
  Controls text over the panel).
- `ui_items_to_vertices` (`crates/debug/src/main.rs`) emits all
  solids first, then all text quads; the draw loop draws solids
  (`use_tex=0`) before text (`use_tex=1`) — every glyph beats every
  solid inside one buffer. The dropdown buffer is drawn after the
  main UI (`main.rs` dropdown comment: "topmost by command order").

## Suspected area

- `compose_overlay_ui` loader block (appends to the main `items`
  buffer instead of the topmost `drop` buffer; dim covers the
  viewport only).
- `exec_load_step(LoadStep::Finalize)` (`select_screen(MilkyWay)`).
