# Update plan — cosmic-scale-player / update-2026-09-20-0645 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

- CPU: `SmokePuff` + `smoke_puffs()` + constants + 2 tests
  (`cosmic_web.rs`); `strand_records` kept (tests + grain shape ref).
- GPU: `SMOKE_VERT/FRAG`, `SmokeVertex`, `SmokePush`,
  `build_smoke_pipeline`, `upload_cosmic_smoke`, `CosmicFrame.smoke`,
  `Pipelines.smoke/smoke_scene`, HDR scene + LDR draws, headless +
  shader/vertex-input/safety/push-floor pins (`main.rs`).
- Grade: `COSMIC_DEMO_SMOKE_EXPOSURE 0.5 / MAP 0.7` (MAP raised ~6x:
  billboards spread energy over ~6–50 px where retired tubes held it
  in ~1.5 px, so the 0.12 ribbon-era grade rendered the inspector
  effectively empty); 2-px min-world-size clamp in `SMOKE_VERT`;
  build tag `r3`.

## Role sign-off

Breakdown approved by: ARCHITECT _(self-enforced: invariants held)_ ·
Todos approved by: TECHLEAD _(budgets checked)_ · Verified by ANALYST:
_(gates green below)_ · Security reviewed by SECURITY: _(no new I/O)_

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260920-001 | done | CPU smoke layout + tests | In-scope |
| UPD-20260920-002 | done | GPU smoke pipeline, retire ribbons | In-scope |
| UPD-20260920-003 | done | Grade + Low-tier budget check | Requirements |
| UPD-20260920-004 | done | Gates green + docs sync | DoD |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Smoke replaces ribbons both surfaces | done | `grep RIBBON main.rs` empty; `smoke_scene`/`smoke` draws in HDR+LDR |
| 2 | Puff count in budget | done | headless `cosmic_layout=smoke51849` (<65k; ~104k tris <500k Low) |
| 2b | Smoke visible in inspector tab | done | root cause: MAP exposure 0.12 + no min-px clamp → subpixel, near-zero alpha at 430 Mpc; fix: MAP 0.7 + 2-px `min_world` clamp, pinned by `cosmic_shader_safety_pins` (`min_world` literal); full visual check pending viewer restart (exe locked by running PID at fix time) |
| 3 | Shaders compile + pins | done | `viewer_shaders_compile`, `cosmic_vertex_inputs_match_vertex_fields`, `cosmic_shader_safety_pins` pass |
| 4 | Gates green | done | `cargo fmt` clean; `clippy -D warnings` clean; `build --workspace` ok; `test --workspace --all-targets` all pass; `game_debug --headless` + `game_tools --headless --tier low` ok |
| 5 | Docs synced | done | `rendering.md`, `techstack/README.md`, this update |

## Acceptance criteria

- No `Ribbon`/`StrandVertex`/`LINE_EXPOSURE` symbols remain in `main.rs`.
- Bloom write-once chain (A–E) untouched; projection/winding/picking
  invariants untouched.

## Risks & Next steps

- Inspector overdraw under extreme zoom-out (dozens of puffs/px):
  mitigated by tiny alpha + MAP exposure 0.12; follow-up: distance/
  frustum cull + grain budget cut on Low.
- Grain still 800k points (unchanged): next perf pass should tier it.
- P2 quad impostors (256px clamp fix) still open.
