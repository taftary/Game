# Issue plan — settings-seed-loader / issue-2026-09-18-2248-loader-topmost-stay-settings (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

1. **Topmost modal (TECHLEAD: reuses the dropdown's proven
   isolation; no record-loop or pipeline change).** Move the loader
   composition from `items` to the `drop` buffer in
   `compose_overlay_ui`; dim the full window (`win_w × win_h`)
   instead of the viewport only; panel stays viewport-centered.
2. **Stay in place (TECHLEAD: deletion only).** Remove the
   `select_screen(MilkyWay)` call from the `Finalize` step (keep
   fade + notify); update the `--seed` boot comment to the uniform
   stay rule (Game Demo shows the seeded web).
3. **Tests (TECHLEAD: extends the existing overlay test style).**
   Pin drop-buffer isolation + full-window dim in
   `loader_modal_overlays_seed_bar_and_step`; the dropdown
   isolation test guards the shared buffer.
4. **Docs + gates.** `controls.md` stay wording; `cargo fmt`,
   `clippy -D warnings`, `cargo test --workspace --all-targets`,
   `game_debug --headless` (temp target dir while the live viewer
   holds the exe lock); no-touch check of the fixed hunks against
   commit `16a0a43`.

## Role sign-off

Fix reviewed by TECHLEAD: approved 2026-09-18 (drop-buffer reuse of
the dropdown's proven isolation — no pipeline/record change;
stay-in-place is deletion-only; tests mirror the existing overlay
style; no budget risk — same vert count, different buffer) ·
Verified by ANALYST: 2026-09-18 (per-row audit below; gates green on
this tree) · Security reviewed by SECURITY: 2026-09-18 (pass — no
new input parsing, deps, `unsafe`, or IO; buffer move only).

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260918-001 | completed | Loader modal into `drop` buffer + full-window dim | Observed 1 |
| ISS-20260918-002 | completed | `Finalize` stays on current screen; boot comment | Observed 2 |
| ISS-20260918-003 | completed | Modal test: drop isolation + full-window dim | Scope & Impact |
| ISS-20260918-004 | completed | `controls.md` wording; gates green; no-touch check | Scope & Impact |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Loader topmost over all screens; full-window dim | done | `loader_modal_overlays_seed_bar_and_step`: modal texts/solids in `drop` buffer only, main buffer has no `LOADING`, dim rect == full window; dropdown isolation test still green |
| 2 | Loads stay on the current screen (`--seed` on Game Demo) | done | `select_screen` removed from `Finalize` (`rg` zero `select_screen` in loader path); boot comment updated; headless `--seed` path unchanged |
| 3 | Regression tests pin both; gates green | done | `cargo fmt --check` ok; `clippy --workspace --all-targets --all-features -D warnings` ok; `cargo test --workspace --all-targets` ok; `game_debug --headless` ok (temp target dir — live viewer holds the exe lock) |
| 4 | Docs match behavior; parent files untouched | done | `controls.md` stay wording; notion/`plan.md` untouched (this folder carries the delta per §6); no hunk of `16a0a43` modified (hunk-range check) |

## Acceptance criteria

- AC-1: Settings Load mid-flight shows zero row-text bleed (unit:
  modal absent from main buffer, present in drop buffer).
- AC-2: completing a load from Settings leaves the shell on
  Settings (fade + notify still fire); `R` on Milky Way unchanged
  in place; boot stays on Game Demo.
- AC-3: no hunk of commit `16a0a43` (target-highlight issue) is
  modified.
