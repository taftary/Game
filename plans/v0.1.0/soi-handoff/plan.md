# Plan — soi-handoff

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Radii + eligibility + blend math (`engine::handoff`)

Owning module: new `engine::handoff` (pure math over `glam` `f64`,
headless-testable; consumes `frames` transition API + `physics` unit
constants, both pure). No new dependencies, no `unsafe`, no I/O.

- `laplace_soi_radius(a, m, M)` / `hill_radius(a, m, M)` (ADR-014):
  same units in/out. Regression: Earth–Sun → 9.24e5 km / 1.50e6 km
  (±1%), ratio in [0.5, 0.7] (PO tolerance table).
- `handoff_eligible(a_primary, a_secondary)`: secondary ≥ 1e-3 primary
  (shared constant with `time-compression`, per PO decision).
- `BlendBand { r_soi }` + `weight(distance)`: 0 at/above 1.05 × r_soi
  (primary-dominant), 1 at/below 0.95 × r_soi (secondary-dominant),
  smoothstep between — zero derivative at both edges, no linear ramp.
- `blend(a_primary, a_secondary, weight)`: lerp by the smoothstep
  weight (the weight IS the smoothing; lerping vectors by it is exact).
- Edge-continuity tests: blended accel equals primary-only at the
  outer edge and secondary-only at the inner edge (fp-exact by
  construction: w = 0/1).

### Phase 2 — State transform + monitor

- `transform_to_secondary(pos_pri, vel_pri, body_pos_pri, body_vel_pri,
  unit_ratio)`: position C⁰ / velocity ~C¹ mechanics — the transform is
  linear (exact up to fp); the "~" acknowledges patched conics
  approximate N-body, not our math. Round-trip test vs the inverse.
- `HandoffMonitor`: hysteresis event source for HUD + autosave.
  Thresholds from spec §10: `Approaching` on rising past weight 0.2;
  `Entered` at weight 1 (completed handoff → ADR-004 trigger via the
  caller's `FrameTransition`);
  `Exited` on falling below 0.15 (hysteresis gap kills noisy repeats).
  Payload carries body id + weight: everything `navigation-hud` needs
  (UX acceptance = payload completeness, notion `Roles`).
- Noise test: weight oscillating around 0.2 → exactly one
  `Approaching`; full in-out trajectory → ordered
  Approaching → Entered → Exited, no repeats (PO hysteresis criterion).

### Phase 3 — Crossing integration test + fly-to precedence

- Full pipeline test (Earth–Sun SOI, Verlet, real μ constants):
  eligibility → weight → blended accel → steps → monitor events →
  `commit_to_child` at `Entered` → converted state round-trips →
  continued integration stays sane. This is the DoD C⁰/~C¹ evidence.
- ARCHITECT answers the notion's open question (band-edge during an
  active select-to-focus fly-to): **blending always governs
  acceleration; the fly-to plan subscribes to `Entered` and replans**.
  Rationale: dynamics must never depend on plan state (spec §2: no
  scripted pacing leaks); plans adapt to dynamics, never vice versa.
  `free-flight-navigation` implements the subscription.

### Phase 4 — Bindings

- ADR-014 `draft` → binding. Milestones → `done`; version bump.
- No HUD rendering (non-goal); no time-compression coupling beyond the
  shared 10⁻³ constant (non-goal, must-not-desync recorded for the
  `time-compression` plan).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| HOF-001 | done | Laplace + Hill radii with Earth–Sun regression (±1%, ratio band) | Goals 4, DoD 2 |
| HOF-002 | done | Eligibility (10⁻³) + band weight (smoothstep, edge derivatives zero) | Goals 2–3, FR |
| HOF-003 | done | Blend fn + edge-continuity tests (outer = primary, inner = secondary) | FR, NFR |
| HOF-004 | done | State-vector transform + round-trip test | FR, DoD 1 |
| HOF-005 | done | `HandoffMonitor` hysteresis + noise/order tests + HUD-complete payload | FR, DoD 3 |
| HOF-006 | done | Full crossing integration test (Verlet + real commit + round trip) | DoD 1, NFR |
| HOF-007 | done | Fly-to precedence rule recorded (blending wins, plan replans) | Open questions |
| HOF-008 | done | ADR-014 → binding; milestones; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV: same list as `frame-hierarchy`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new
pure module consuming frames + physics; adjacency to the real commit
API in the integration test; fly-to precedence decided (dynamics over
plans); no invariant touch — projection/ENU untouched)_ · Todos
approved by: TECHLEAD _(signed 2026-09-17 — math first
(radii/weight/blend), monitor second, integration last; every todo has
a named test; no budget risk (offline math + one monitor update per
tick))_ · UX acceptance rows: _(payload completeness for HUD:
weight + 0.2/0.15 thresholds + body id — verified in HOF-005 test;
rendering belongs to `navigation-hud`)_ · DoD verified by: ANALYST
_(signed 2026-09-17 — every row re-checked; gate suite re-run green:
fmt, clippy `-D warnings`, workspace build, workspace tests
32+118+18+144, doc tests 6+30, `game` / `game_debug --headless` /
`game_tools --headless --tier low` runs; audit findings below fixed
in-DEV, no shipped defect, no issues filed)_ · Security reviewed by:
SECURITY _(signed 2026-09-17 — no new input surface, dependency,
`unsafe`, serialization, or file I/O; pure math; no findings)_.

## Audit findings (fixed in-DEV, recorded for process)

- H1 — spec §3 Hill/Laplace ratio inverted (spec bug, not code):
  Hill/Laplace = 0.693·(m/M)^(−1/15) > 1 at every planetary mass
  ratio (Earth 1.62×). Corrected in spec + ADR-014 + notion + plan;
  implementation formulas were always right.
- H2 — lerp edge-inexactness (1 ulp): `blend` now short-circuits
  exact edges, making "outer = primary / inner = secondary" real.
- H3 — eligibility exact-boundary float: test values moved off the
  fp-meaningless boundary.
- H4 — crossing scenario gave the ship comoving velocity against a
  static Earth (flew off in +y): pure radial approach now.
- H5 — process: a `lib.rs` edit dropped `pub mod render`; filtered
  `cargo test -p` runs hid it, full-workspace gates caught it. Rule
  reaffirmed: gate list runs workspace-wide, always.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Handoff integration test: crossing ship shows C⁰ position / ~C¹ velocity continuity within documented tolerance | done | `ship_crossing_earth_soi_is_continuous_and_commits`: Verlet Earth approach → Entered → real `commit_to_child` matches direct transform < 1 m (C⁰), velocity round-trips < 1e-9 km/s (~C¹), 100 post-handoff steps sane | ANALYST _(signed 2026-09-17)_ |
| 2 | Laplace vs Hill helpers with regression values (spec §3 note, corrected: Laplace ≈ 0.5–0.7 × Hill for planet–Sun) | done | `earth_sun_radii_match_reference_values`: Laplace 924,000 km ±1%, Hill 1,496,000 km ±1%, ratio 0.62 ∈ [0.5, 0.7] | ANALYST _(signed 2026-09-17)_ |
| 3 | Hysteresis event log demonstrably free of noisy repeats | done | `monitor_orders_events_without_repeats`: straddling noise → single Approaching; ordered Approaching → Entered → Exited; clean re-entry | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, serialization, or I/O — pure math | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine handoff::` green: radii regression,
  eligibility, smoothstep edges, edge continuity, transform round
  trip, hysteresis noise/order, full crossing.
- No position jump / velocity kick / HUD flicker BY CONSTRUCTION:
  edge-exact weights + hysteresis gap + ordered events (asserted).
- Doc tests for all new public `engine::handoff` APIs.
- `time-compression` desync risk recorded (shared constant, separate
  state machines — verified in that feature's plan).

## Risks & Next steps

- Patched conics approximate: "~C¹" is honest — exact N-body needs the
  full force model (never planned; cheapest-correct per ADR-018).
- `time-compression` must not desync blending: shared 10⁻³ constant +
  independent state machines; cross-check in that feature.
- Next: `time-compression` (integrator contract + occupancy API),
  then `free-flight-navigation` (consumes handoff events + KG solver +
  craft envelope; implements fly-to replan on `Entered`).
