# Issue plan — cosmic-device-tier / issue-2026-09-24-0856-high-to-low-stale-frame (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

Bin-only repair in `crates/debug/src/main.rs` (risk-first: pure
branch selector + clear helper first, then the two recording call
sites, then grade hardening). No camera / projection / picking /
marker invariant is touched; the bloom write-once invariant is
restored, not changed — every frame writes the march target exactly
once (march draw in March mode, black clear in Sprites mode) and the
resolve keeps reading scene + bloom-top + march in both modes, so the
existing `describe_veil_chain(levels, true, …)` pin becomes true again
with no description change. No new image, no pipeline/layout change,
no grade change inside any tier, no `engine` / `game` / `tools` diff.

Phases:

1. Pure selector + clear helper + unit tests (the regression lock —
   the bug was a missing `else` no pin covered).
2. Windowed + capture call-site rewire (both arms go through one
   `record_veil_march_or_clear`, so the `else` cannot be forgotten
   again).
3. Defense in depth (`march_gain = 0` in Sprites) + gates/docs/status.

## Role sign-off

Breakdown reviewed by ARCHITECT: approved 2026-09-24 (bin-only repair;
write-once invariant restored, not changed — no new image, no
pipeline/layout change, `describe_veil_chain(levels, true, …)` holds
again) · Fix reviewed by TECHLEAD: approved 2026-09-24 (risk-first
order, both call sites through one choke point, budgets untouched —
Low is the specified tier) · Verified by ANALYST: approved 2026-09-24
(see audit note) · Security reviewed by SECURITY: pass 2026-09-24 (see
review note)

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260924-001 | done (DEV 2026-09-24) | Pure `veil_march_draws(VeilMode)` selector + `clear_veil_march(builder, hdr)` helper (empty `begin_post_pass(march_fb) → end`, no draw, black clear) + `record_veil_march_or_clear` choke point + unit tests (Sprites → Clear, March{32,48} → March) | Suspected area |
| ISS-20260924-002 | done (DEV 2026-09-24) | Windowed path (`draw_main`, was `main.rs:10658`) routes through `record_veil_march_or_clear` so Sprites frames clear the march target before the resolve | Reproduction steps |
| ISS-20260924-003 | done (DEV 2026-09-24) | Offscreen capture path (was `main.rs:2410`) routes through the same helper so `--tier low` captures are defined even on allocator-reused pages | Reproduction steps |
| ISS-20260924-004 | done (DEV 2026-09-24) | Pure `march_gain_for(&VeilMode)` (March → `VEIL_MARCH_RESOLVE_GAIN`, Sprites → `0.0`) + use at both resolve call sites + unit test; stale-proof even if a future path skips the clear | Observed vs Expected |
| ISS-20260924-005 | done (DEV 2026-09-24, pending operator/GPU + role sign-offs) | Gates green (see DoD 4) + techstack 0.57.0 → 0.57.1 bump + stale comments corrected; specs stays `in-review` until operator `F4` check + ARCHITECT/TECHLEAD/ANALYST/SECURITY sign | Scope & Impact |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | `F4` High → Low (and Medium → Low) draws the clean Low tier with no ghost, stable across frames, on both cosmic surfaces (inspector + demo) | done (operator 2026-09-24) | Ghost confirmed gone by the operator on HDR hardware. "Stable across frames" for the ghost mechanism: Sprites frames clear the march target (defined black) and resolve it at gain 0 — no unwritten image is ever sampled again. Residual Low fps instability is load, not ghosting, and continues as [`low-tier-fps-unstable`](../issue-2026-09-24-0944-low-tier-fps-unstable/specs.md). |
| 2 | March → March transitions (High ↔ Medium) unchanged — march still drawn every frame | done (DEV 2026-09-24) | `March` arm still calls `record_veil_march` unchanged; `march_gain_for(March{32,48}) == VEIL_MARCH_RESOLVE_GAIN` pinned by `sprites_resolve_adds_no_march`; full workspace tests green (no timing regression measurable headless). |
| 3 | Write-once bloom pin holds and models Sprites mode (march target written once per frame, never read by its writer) | done (DEV 2026-09-24) | `assert_write_once` + all veil-chain tests green; new `sprites_mode_clears_instead_of_skipping_march` + `sprites_resolve_adds_no_march` green (debug bin 50/50). The `describe_veil_chain(levels, true, …)` assert is true again: the clear is the once-per-frame march write in Sprites mode. |
| 4 | Gates green, bin-only diff, no grade/budget change | done (DEV 2026-09-24) | `fmt --check`, `clippy --workspace --all-targets --all-features -D warnings`, `build --workspace`, `test --workspace --all-targets` (40+0+299+50+311+3+5, all ok), `test --doc --workspace` (6+0+85+0 ok), `game` run ok, `game_debug --headless` ok, `game_tools --headless --tier low` ok. `git diff --stat` scope: issue folder + `crates/debug/src/main.rs` + `docs/techstack/README.md` (verify at commit). No grade literals touched (`VEIL_MARCH_K`, exposures, ramp/transfer pins green). |

## Acceptance criteria

- ARCHITECT invariant rows:
  - A-1. Every HDR cosmic frame writes the march target exactly once
    (march draw or black clear) before the resolve reads it
    [`assert_write_once` + new unit tests green] [T].
  - A-2. No grade change inside any tier (resolve exposures, bloom
    intensity, `VEIL_MARCH_K`, transfer/ramp literals untouched;
    Sprites resolve adds exactly 0) [T: shader-string pins green].
  - A-3. `F4` stays a Chrome-only key; no content-key shadowing, no new
    button/surface [T: actions registry test green].
  - A-4. No `engine` / `game` / `tools` diff; `--headless` loads no
    Vulkan symbols [T: existing headless runs].
- Functional: operator presses `F4` High → Low on HDR hardware and the
  Low view matches the fresh `--tier low` capture look with no stale
  overlay; resize-in-Low and boot-in-Low are clean for the same reason.
- `git status` shows only: this issue folder,
  `crates/debug/src/main.rs` (fix + tests),
  `docs/techstack/README.md` (version line).

## ANALYST audit note (2026-09-24)

- Re-ran: `fmt --check`, workspace `clippy -D warnings`, `build
  --workspace`, `test --workspace --all-targets`
  (40+0+299+50+311+3+5, all ok incl. the 2 new Sprites tests),
  `test --doc --workspace` (6+0+85+0 ok), `game` run ok,
  `game_debug --headless` ok, `game_tools --headless --tier low` ok.
- DoD 1: ghost-gone confirmed by the operator on HDR hardware (this
  automation cannot press `F4` — same class as CDT L-2, now closed by
  the operator report); structural guarantee re-checked (every frame
  writes march once, Sprites resolves at 0).
- DoD 2–4: code + test evidence as tabled. E2E: n-a (debug-only
  tooling, no player journey, no save format — CDT precedent).
- Verdict: all rows pass. Residual Low fps instability filed as
  [`low-tier-fps-unstable`](../issue-2026-09-24-0944-low-tier-fps-unstable/specs.md),
  not a rejection of this fix.

## SECURITY review note (2026-09-24)

- New input surfaces: none (no CLI/env/key change — `F4`, `S`,
  `--tier` untouched).
- New dependencies: none. New `unsafe`: none (one `begin`/`end`
  render-pass pair, no draw — the same `begin_post_pass` mechanism
  every pyramid pass uses).
- Save/migration: untouched. Verdict: pass, no findings.
