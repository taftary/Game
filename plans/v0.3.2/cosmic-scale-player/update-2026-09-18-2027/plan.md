# Update plan — cosmic-scale-player / update-2026-09-18-2027 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

ARCHITECT notes (module + invariant annotations; `engine::flight`
behavior untouched; fly-to + clock + camera + marker contracts intact):

### UPD-A — Cruise controller (`debug::cosmic_player`)

Replace the free-flight branch in `CosmicPlayerState::step_free` with a
cruise integrator. New `CruiseParams { t_cross_s, tau_s }` (runtime params,
not constants). Convention pin: `ship.vel` at Cosmological is Mpc per
**sim-second** — the same unit `FlyToExec` hands back, so cancel is
continuous by construction (fixes the latent sim-s/Gyr mismatch).
Integration `pos += v · sim_dt`, easing `v ← v + (v_t − v)(1−e^{−dt_real/τ})`
with `v_t` recomputed from the current clock ratio each tick. Drop
`step_free_flight`/`CraftParams`/`ThrustInput` use here (fly-to types stay).
Invariant rows: "cruise motion is real-time-anchored and depth-scaled —
screen-speed authority at any occupancy", "zero live gravity still
enforced via the static-field closure".

### UPD-B — Demo intent + scale scan (`debug::cosmic_demo`)

`thrust_input()` → `cruise_input()` (body-axis direction + `any()` drive the
UPD-A target; same `HeldThrust` flags, same key routing). Extend the
nearest-node O(n) scan to expose the scale length
`L = max(dist, SCALE_FLOOR_RVIR × r_vir)`; `cruise_speed_by(factor)` scales
`t_cross` (notch ×1.5, clamped). `tick` keeps cancel-on-cruise-input first.
Invariant row: "fly-to engage/cancel/arrival semantics byte-identical —
only the freed-step motion law changes".

### UPD-C — Shell wiring (`debug::main`, `actions`)

`Shift`+wheel in the demo adjusts `t_cross` (wheel alone stays camera
zoom); hint line rewritten; new speed HUD row in `build_demo_ui` fed from
player velocity + params (demo-side only, `game::hud` untouched); Controls
registry +1 cruise-speed action, re-pin the size test. Invariant row:
"no `Journey`/transit/dimension behavior changes — demo-surface only".

### UPD-D — Engine docs (`engine::flight`, docs-only)

Fix the stale `ShipState.vel` ("per SI second") and `step_free_flight`
("real seconds") doc comments for non-SI-time frames (the reading the
WS2 code already implements); no behavior change.

### UPD-E — Docs sweep

`docs/game/controls.md` demo paragraph; amendment note in
`docs/decisions/ADR-023.md`; version bump in
`docs/techstack/README.md`; parent plan files untouched, cross-linked.

## Role sign-off

Breakdown approved by: ARCHITECT _(done 2026-09-18 — module/invariant
annotations in the delta breakdown above)_ · Todos approved by: TECHLEAD
_(done 2026-09-18)_ · Implemented by DEV: _(done 2026-09-18 — gates
green, evidence in DoD table)_ · Verified by ANALYST: _(pending)_ ·
Security reviewed by SECURITY: _(pending)_

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260918-001 | done | Write update notion + plan (this folder), roles filled | Requirements delta |
| UPD-20260918-002 | done | UPD-A: cruise branch in `step_free` + `CruiseParams` + unit pin + module docs | Requirements delta ¶1,4 |
| UPD-20260918-003 | done | UPD-A tests: cruise reach, 5-s visible-motion regression, coast/replay, hand-back continuity; delete the 2 removed-behavior thrust tests | DoD 1,3,4,6 |
| UPD-20260918-004 | done | UPD-B: `cruise_input`, scale-length scan, `cruise_speed_by`; update `cosmic_demo` tests (select/engage/cancel flow stays) | DoD 1,2 |
| UPD-20260918-005 | done | UPD-C: Shift+wheel, hint line, speed HUD row, registry +1 action | DoD 2,5 |
| UPD-20260918-006 | done | UPD-D: engine doc-comment fixes only | Scope (in) |
| UPD-20260918-007 | done | UPD-E: controls.md, journey.md, ADR-023 amendment, techstack version bump, parent cross-links | DoD 8 |
| UPD-20260918-008 | done | Quality gates green (fmt/clippy/build/test/doc/headless ×2); DoD evidence in table | DoD 7 |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | WASD displaces ≫ ulp within 5 s at spawn (headless regression) | done | `cosmic_player::cruise_from_spawn_moves_mpc_scale_in_seconds` (moves ~4.2 Mpc, converges 1 Mpc/s); `cosmic_demo::tick_cruises_visibly_and_tracks_camera`; headless `cruise_selftest=moved1.56Mpc ok` |
| 2 | Depth-scaled, Shift+wheel-adjustable cruise speed (tests) | done | `cruise_target_speed_scales_with_scale_length` (2× scale ⇒ 2× speed); `scale_length_is_floored_near_nodes` (exact r_vir floor + `cruise_speed_by` clamp 2–600 s); `Shift`+wheel + Controls-row parity in `main.rs` |
| 3 | Coast preserves momentum; fixed-step replay bit-identical | done | `coast_preserves_velocity_and_replays_bit_exact` (`assert_eq!` on vel + full `ShipState` replay) |
| 4 | Fly-to unchanged; cancel hand-back unit-consistent (test) | done | `committed_executor_drives_to_rest_at_target`, `cancel_*` (2 pre-existing, green unmodified); `cancel_mid_leg_hands_back_the_eased_state_unit_consistent` (`assert_eq!` vs `plan.velocity_at/position_at`) |
| 5 | Live speed HUD row; hint + Controls updated | done | `speed: … Mpc/s · cruise …s` row in `build_demo_ui`; hint line rewritten; `Action::CruiseSpeed` in registry + Controls list (automatic via `all_actions`) |
| 6 | Removed-behavior tests deleted; no other regressions | done | `thrust_matches_the_frame_unit_convention` + `single_step_matches_fine_substeps` deleted; workspace suite 523 passed / 0 failed |
| 7 | Quality gates green | done | `cargo fmt --check` clean; `clippy --workspace --all-targets --all-features -- -D warnings` clean; `build --workspace` ok; `test --workspace --all-targets` 523/0; `test --doc --workspace` 83/0; `game_debug --headless` ok; `game_tools --headless --tier low` ok |
| 8 | Docs sweep; parent untouched/cross-linked; one commit on `v0.3.2` | partial | `controls.md` + `journey.md` demo paragraphs, ADR-023 amendment + consequences, techstack `0.33.1`; parents untouched (update links up only, per precedent); commit pending ANALYST + SECURITY sign-off |

## Acceptance criteria

- In the running demo, holding `W` at spawn visibly moves the `YOU` marker
  within ~1 s, steering turns the flight direction, release coasts.
- `Shift`+wheel changes the cruise pace shown in the speed row.
- Click + `E` fly-to and cancel behave exactly as v0.3.2 shipped.

## Risks & Next steps

- Tuning (landed values, 2026-09-18): `t_cross` default 20 s, range
  [2, 600] s, notch ×/÷1.5; τ = 0.75 s; scale floor 1.0×r_vir.
  Headless default seed: 5 s of W moves 1.56 Mpc (unit tests pin >1 Mpc
  with scale 20 at ratio 1 ⇒ ~4.2 Mpc). Screen-speed authority
  (real-time-anchored motion) is the deliberate deviation from the
  sim-time thrust model; rationale documented in the ADR-023 amendment.
- Follow-up (out of scope): applying the cruise pattern to future lower
  frames, if Mpc-style scales ever need it there.
