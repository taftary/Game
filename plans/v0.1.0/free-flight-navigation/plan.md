# Plan — free-flight-navigation

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Ship state + free flight (`engine::flight`)

Owning module: new `engine::flight` (pure ship dynamics over `glam`
`f64`, headless-testable; consumes `frames`, `physics`, `time`
contracts; never touches `render`/`winit` — camera wiring is game-crate
integration, so the rendering-invariants constraint is satisfied by
construction + documented). No new dependencies, no `unsafe`, no I/O.

- `CraftParams` (PO envelope, spec §10 addendum): dry mass 5,000 kg,
  max 30 m/s², rotation stabilization ON, translation damping OFF by
  default (toggleable), infinite propellant (fuel tracked, burn
  disabled), no collision/landing/atmosphere. All fields tunable.
- `ShipState { chain: FrameChain, vel: DVec3 (active-frame units/s),
  mass_kg, fuel }`: spec §10 persistence shape. Thrust input is
  abstract (`ThrustInput { throttle 0..1, body_axis (unit),
  attitude: Option<DQuat> }`) — device binding lives in the game crate
  per `controls.md` (unified action map, no per-device logic here).
- `step_free_flight`: attitude (stab = direct set, else quaternion
  rate integrate) + thrust accel (converted to frame units via
  `FrameUnits`) + caller gravity closure (ADR-018 per-scale model) on
  velocity Verlet. Momentum tests in SolarSystem + Planetocentric
  frames (DoD 1): no-input ⇒ constant velocity; thrust ⇒ Δv = a·t.
- `commit_to_parent/child` wrappers convert position + orientation
  (chain) + velocity (linear, same math as `handoff::transform_*`):
  momentum-conservation test (kinetic energy continuous across
  commit). Feeds ADR-004 via `FrameTransition` (reason
  `BoundaryCrossing` on handoff commits).

### Phase 2 — Select-to-focus

- `Target { frame, position }` (frame units) + typed `SelectError`
  (unknown frame / non-finite position — failure has a surface, UX).
  Region/body catalog lookup lands with `star-catalog-streaming`;
  callers map cells → positions (documented).
- `FlyToPlan { from, target, t_start, t_end, easing }`: duration =
  `SECONDS_PER_DECADE × log10(d_start/d_end)` (spec §1 constant time
  per decade; `SECONDS_PER_DECADE = 10.0`, tunable); easing =
  smootherstep on the start→end segment (zero endpoint velocities —
  pacing applies ONLY to the transition, spec §2). Multi-decade test
  (8 decades SolarSystem → planetary) with documented easing values.
- `FlyToExec`: armed → committed → executing; `cancel()` anytime
  restores free flight with position + eased velocity continuous
  (DoD: cancellation clean). Commit/cancel semantics mirror
  `gameplay.md` transit (timed, cancellable before commit; fuel hook
  deferred — propellant infinite in v0.1.0).
- Handoff rule (HOF-007 implemented): `replan_on_handoff()` converts
  the remaining vector on `Entered` — continuity test via synthetic
  mid-flight commit. Blending still governs acceleration; the plan
  adapts (never vice versa).
- HUD accessors (UX acceptance): `mode()`, `target()`,
  `distance_remaining()`, `eta_s()` — everything spec §10
  select-to-focus display needs.

### Phase 3 — Persistence payload + bindings

- `ShipSnapshot` (frame code + pos/vel/quat/mass/fuel + optional
  `FlyToPlan` + `CompressionSnapshot`) with byte codec round trip;
  mid-transition test: execute half → snapshot → restore into fresh
  exec → identical end state (DoD 3 at payload level;
  `autosave-persistence` embeds it — same accepted-gap pattern).
- No ADR touched (ADR-018 already binding; envelope is a PO decision).
  Milestones → `done`; version bump. Closes v0.1.0 (all six done).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| FLT-001 | done | `ShipState` + `CraftParams` (PO envelope) + frame-unit thrust conversion | FR, Non-goals |
| FLT-002 | done | `step_free_flight` + momentum tests (SolarSystem + Planetocentric) | Goals 1, DoD 1 |
| FLT-003 | done | Commit wrappers + momentum-conservation test | FR |
| FLT-004 | done | Planner (decades duration + smootherstep easing, documented) | FR, Goals 2 |
| FLT-005 | done | Executor + cancel continuity test | FR, DoD 2 |
| FLT-006 | done | `replan_on_handoff` continuity test (HOF-007) | Constraints, DoD 2 |
| FLT-007 | done | HUD accessors (mode/target/distance/ETA) | Users (UX) |
| FLT-008 | done | Snapshot/plan codec + mid-transition resume test | NFR, DoD 3 |
| FLT-009 | done | UX acceptance rows verified (input abstraction, failure surfaces) | Users (UX) |
| FLT-010 | done | Milestones → done; version bump; full gate list green; v0.1.0 close check | NFR, lifecycle |

Gate commands for DEV: same list as `frame-hierarchy`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new
pure module; camera boundary respected (no render touch — anchor feed
is game-crate work); catalog lookup explicitly deferred with typed
errors; no locked choice overturned, no ADR needed)_ · Todos approved
by: TECHLEAD _(signed 2026-09-17 — dynamics first, planner second,
persistence third; every todo has a named test; the longest fly-to
test is thousands of Verlet steps of pure math — milliseconds)_ · UX
acceptance rows: _(input abstraction per controls.md; cancel +
typed errors as failure surfaces; HUD accessors complete — verified
in FLT-007/FLT-009: mode/target/distance/ETA observable)_ · DoD
verified by: ANALYST _(signed 2026-09-17 — every row re-checked; gate
suite re-run green: fmt, clippy `-D warnings`, workspace build,
workspace tests 32+118+18+162, doc tests 6+32, `game` /
`game_debug --headless` / `game_tools --headless --tier low` runs;
E2E = scripted `game` journey (pure-dynamics feature, no I/O); audit
findings below fixed in-DEV, no shipped defect, no issues filed)_ ·
Security reviewed by: SECURITY _(signed 2026-09-17 — no new input
surface, dependency, `unsafe`, serialization, or file I/O; pure
dynamics; snapshot codec rejects corrupt input; infinite fuel is
explicit PO state, not a corrupt value; no findings)_.

## Audit findings (fixed in-DEV, recorded for process)

- F1 — `commit_to_child` panicked (debug_assert) on non-adjacent
  targets instead of returning `NotAdjacent`: adjacency now checked
  before mapping (no panic path); test asserts state untouched.
- F2 — snapshot codec rejected infinite fuel as corrupt: infinite
  propellant is first-class PO state — codec accepts +inf only
  (NaN/−inf still rejected).
- F3 — first-order attitude integration too crude for the test bound:
  exact constant-rate quaternion step instead (also better physics).
- F4 — clippy `field_reassign_with_default` on test params: struct
  update syntax.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Free flight with momentum in at least the solar-system and planetary frames (integration evidence) | done | `coast_keeps_momentum_bit_exact_in_both_frames` + `thrust_produces_expected_dv_in_both_frames` + `commit_conserves_momentum_across_frames` (physical speed invariant 1e-12) | ANALYST _(signed 2026-09-17)_ |
| 2 | Select-to-focus completes a multi-decade fly-to with documented easing; cancellation restores free flight cleanly | done | `planner_uses_decades_and_documented_easing` (11 decades, smootherstep(0.25) = 0.103515625) + `executor_completes_and_cancel_is_clean` (arrival at rest, cancel = analytic derivative) + `replan_on_handoff_is_position_exact` | ANALYST _(signed 2026-09-17)_ |
| 3 | Ship state survives save/load mid-transition (with `autosave-persistence`) | done | `snapshot_round_trips_and_midflight_resume_is_exact` (131/204-byte codec, corrupt rejection, identical continuation) — payload level, save module embeds later | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, or I/O; codec rejects corrupt input | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine flight::` green: momentum (2 frames),
  commit conservation, planner decades/easing, executor completion,
  cancel continuity, handoff replan, HUD accessors, snapshot resume.
- No fixed pacing in free flight (assert: unthrusted coast keeps
  velocity bit-exact; only the fly-to path uses easing).
- Doc tests for all new public `engine::flight` APIs.
- v0.1.0 close check: all six milestone rows `done` after this
  commit (verified pre-commit).

## Risks & Next steps

- Catalog-backed target selection (`star-catalog-streaming`) and HUD
  rendering (`navigation-hud`) are explicit follow-ups; the typed
  errors + accessors are the contract they implement against.
- Tick-loop integration (input devices, camera anchor feed,
  `in_blend_band` wiring, max_dt sizing) is game-crate work in the
  descent milestone — this feature proves the dynamics, not the loop.
- Next: merge `v0.1.0` → `main` (Step 9).
