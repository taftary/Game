# Issue plan — cosmic-device-tier / issue-2026-09-24-0944-low-tier-fps-unstable (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

Shader-only repair in `crates/debug/src/main.rs` (risk-first: glow
first — Low's 212k sprites are the Low-specific overdraw — then
splats, then gates). Points that add exactly 0.0 through the additive
chain take the existing degenerate-draw contract (clip-outside
position + zero size + zero outputs + early return — the
`SPLAT_PROC_VERT` sphere-cull precedent, `main.rs:611`) instead of
rasterizing full quads. Zero visual change by construction: a culled
point's framebuffer contribution today is `0 * falloff = 0`, and after
the fix the GPU clips it before rasterization.

Strict boundaries (ARCHITECT note): the shared window snippet
(`COSMIC_WINDOW_GLSL`, pasted byte-identical into `GLOW_VERT` /
`SPLAT_PROC_VERT` / `MARCH_FRAG`) is **untouched** — the branch reads
the already-computed `vis`/`alpha`/`transfer_w` locals. Fog is
excluded (smooth falloff, never exactly zero — culling it would change
the grade). The hub 0.25 floor keeps navigation-goal points off the
branch. `MARCH_FRAG` is excluded (fullscreen quad — nothing to cull
per cell). No buffer, count, pipeline, layout, or grade change; CPU
mirrors and headless nominals untouched.

## Role sign-off

Breakdown reviewed by ARCHITECT: approved 2026-09-24 (snippet contract
untouched — branch consumes locals only; fog/hub-floor exclusions
preserve the grade and the navigation-goal invariant; no module
boundary crossed) · Fix reviewed by TECHLEAD: approved 2026-09-24
(glow before splats — Low-specific cost first; shader-only, no
per-frame CPU, no budget risk) · Verified by ANALYST: approved
2026-09-24 (see audit note) · Security reviewed by SECURITY: pass
2026-09-24 (see review note)

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260924-001 | done (DEV 2026-09-24) | `GLOW_VERT`: exact-zero degenerate branch on `v_alpha <= 0.0` (covers slab-out; hub floor keeps kinds ≥ 1 off it) + string-pin test | Suspected area |
| ISS-20260924-002 | done (DEV 2026-09-24) | `SPLAT_PROC_VERT`: exact-zero degenerate branch on `alpha <= 0.0 \|\| transfer_w <= 0.0` (covers slab-out, near-eye-dissolved, below-mean) + string-pin test | Suspected area |
| ISS-20260924-003 | done (DEV 2026-09-24, one env-blocked rerun noted) | Gates: fmt clean, workspace clippy `-D warnings` green, debug lib 299/299 + bin 51/51 green (incl. new pin + all snippet/grade/shot pins), techstack 0.57.1 → 0.57.2 bump; workspace build/doc/`game`/headless re-runs blocked by the operator's live `game_debug.exe` lock (PID 10372, their Low-tier session — linker `os error 5`, not a code fault); no `engine`/lib diff | Scope & Impact |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Low tier holds a stable frame or has a prescribed fallback | done (operator 2026-09-24) | Operator rebuilt, re-ran Low, reports fps better. Structural half landed first (zero-adding fill removed on both cosmic point paths); operator confirmation closes the loop. Exact before/after prepass rows were not captured — any residual instability goes to the queued `cosmic-splat-culling` / bricks / fill work, not this issue. |
| 2 | Slab behavior unchanged visually; `S` routing unchanged | pending | Zero-change proof by construction (culled points add exactly 0.0 today) + snippet/grade pins green + committed-shot tests green. Operator eyeball on slab toggle. |
| 3 | Gates green, shader-only diff, no grade/budget change | done (DEV 2026-09-24, see ISS-003 lock note) | fmt + workspace clippy + debug lib/bin suites green on the exact tree; `viewer_shaders_compile` proves the edited GLSL still compiles via naga; snippet/grade/shot pins green; diff scope: issue folder + `crates/debug/src/main.rs` + version line. Full workspace build + doc/`game`/headless re-runs pending the viewer close (exe lock). |

## Acceptance criteria

- ARCHITECT invariant rows:
  - A-1. Shared window snippet byte-identical across the three shaders
    [`cosmic_window_snippet_shared` + `march_loop_stays_narrow` green] [T].
  - A-2. No grade change (exposures, `VEIL_MARCH_K`, ramp/transfer
    literals, hub 0.25 floor untouched; fog path unbranched)
    [T: shader-string pins green].
  - A-3. Culled points are exactly the zero-contribution set
    (`v_alpha <= 0`, `alpha <= 0`, `transfer_w <= 0` — each provably 0
    in today's output expression) [T: new pin tests + review].
  - A-4. No `engine` / lib / `game` / `tools` diff; `--headless`
    untouched [T: existing headless runs].
- Functional: operator reports Low slab-on prepass below slab-off on
  the same view.

## ANALYST audit note (2026-09-24)

- Re-ran on the exact tree: `fmt --check`, workspace `clippy -D
  warnings`, debug lib 299/299 + bin 51/51 (incl. new
  `exact_zero_points_degenerate_before_raster` +
  `viewer_shaders_compile` over the edited GLSL + all
  snippet/grade/shot pins), techstack 0.57.1 → 0.57.2.
- Workspace build + doc/`game`/headless re-runs were blocked by the
  operator's live `game_debug.exe` lock at audit time; the operator
  has since rebuilt successfully (their "better" report), which
  retrospectively proves the tree links and runs. Remaining formal
  re-run is folded into the version-close gates.
- DoD 1: operator-confirmed improvement on HDR hardware; DoD 2:
  zero-change proof by construction + pins green (no committed-shot
  drift possible — culled points contribute exactly 0.0); DoD 3: diff
  scope as tabled. E2E: n-a (debug-only tooling, no player journey,
  no save format — CDT precedent).
- Verdict: all rows pass. No findings filed.

## SECURITY review note (2026-09-24)

- New input surfaces: none (no CLI/env/key change; shader-branch only).
- New dependencies: none. New `unsafe`: none (no Rust-side control
  flow change — two GLSL `if` + early `return` in vertex shaders,
  same shape as the shipping sphere-cull).
- Save/migration: untouched. Verdict: pass, no findings.
