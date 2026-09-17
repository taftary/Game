# Plan — time-compression

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Occupancy rule + slew clock (`engine::time`)

Owning module: new `engine::time` (pure state machines + `f64` clocks,
headless-testable; consumes `frames::FrameId`, no other deps). No new
dependencies, no `unsafe`, no I/O.

- `Occupancy { frame, in_blend_band, depth_fraction }`:
  `depth_fraction` = distance-to-primary / significant radius (10⁻³
  rule, PO decision — the caller computes it; they know the bodies).
- `target_ratio(occ)`: `in_blend_band` ⇒ 1.0 (real-time THROUGH
  handoffs — the anti-desync rule, NFR); Planetocentric/LocalEnu ⇒
  1.0 (precision piloting); else `min(ceiling(frame),
  max(1, fraction³))` — continuous at fraction = 1, smooth everywhere.
  Ceilings: SolarSystem 1e4, StellarNeighborhood 1e6,
  Galactocentric/LocalGroup 1e8, Cosmological 1e9.
- `CompressionMode::{RealTime, Compressed}`: RealTime iff target == 1.
  No flapping hazard: ratio is a continuous function, frame changes
  arrive via explicit commits.
- `CompressionClock`: exponential slew toward target (`TAU = 2 s`
  real time) — dt_physics never jumps, including across frame commits.
  `advance(state, dt_real, accel, max_physics_dt)`: sim time advances
  by taking `ceil(ratio·dt_real / max_dt)` fixed-size Verlet substeps.
  Variable-dt symplecticity is not claimed (it doesn't exist); the
  slew is adiabatic and substeps keep every physics dt within budget.
- "No global multiplier path": ratio is display/persist data only.
  The sole advance path derives substeps internally; the test proves
  `advance` ≡ manual substepping (no shortcut scaling).

### Phase 2 — Validation + snapshot

- Real-time period test: circular orbit at ratio 1 → measured period
  within 0.1% analytic (PO table).
- Slew stability test: orbit integrated while slewing 1 → 100 with
  substeps — energy bounded (documents the adiabatic contract).
- `CompressionSnapshot { mode, ratio, sim_time }` + fixed 24-byte
  LE codec (frozen save payload for `autosave-persistence`, same
  accepted-gap pattern as `FrameTransition`/`SeedMetadata`):
  bit-exact round trip + corrupt-input rejection. This is DoD 3 at the
  payload level — no save module exists yet.
- HUD query surface: `mode()`, `ratio()`, `sim_time()` (UX acceptance).

### Phase 3 — Bindings

- ADR-015 `draft` → binding. Milestones → `done`; version bump.
- Desync analysis recorded: shared 10⁻³ constant + in-band forcing +
  slew limits; handoff weight feeds `in_blend_band` (wired by
  `free-flight-navigation`).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| TMC-001 | done | Occupancy → target rule (in-band forcing, ceilings, continuity tests) | Goals 1–2, FR |
| TMC-002 | done | Slew clock (convergence, monotonicity, bounded dt derivative) | FR (smooth ramp) |
| TMC-003 | done | Substep `advance` + manual-substep equivalence (no-multiplier proof) | FR, NFR |
| TMC-004 | done | Sim-time accumulation determinism (pinned sequence) | Goals 3 |
| TMC-005 | done | Real-time orbital period within 0.1% analytic | DoD 2 |
| TMC-006 | done | Slew stability (energy bounded across 1 → 100 transition) | NFR (ADR-018) |
| TMC-007 | done | Snapshot codec round trip + corrupt rejection | DoD 3, Goals 3 |
| TMC-008 | done | Desync guard: in-band forces 1.0 at every frame/fraction | NFR (ADR-014) |
| TMC-009 | done | ADR-015 → binding; milestones; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV: same list as `frame-hierarchy`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new
pure module; occupancy rule + slew + substeps decompose the "smooth,
no jumps" requirement completely; snapshot shape frozen for
`autosave-persistence`; desync analysis explicit; no invariant
touch)_ · Todos approved by: TECHLEAD _(signed 2026-09-17 —
rule first, clock second, validation third; every todo has a named
test; substep counts bounded by max_dt (tick-budget safe); TMC-006
documents adiabatic limits honestly)_ · UX acceptance rows:
_(mode + ratio + sim_time queryable with stable semantics — verified
in TMC-007/HUD-surface test; rendering belongs to `navigation-hud`)_ ·
DoD verified by: ANALYST _(signed 2026-09-17 — every row re-checked;
gate suite re-run green: fmt, clippy `-D warnings`, workspace build,
workspace tests 32+118+18+152, doc tests 6+31, `game` /
`game_debug --headless` / `game_tools --headless --tier low` runs;
E2E = scripted `game` journey (pure-math feature, no I/O); no
findings, no issues filed)_ · Security reviewed by: SECURITY _(signed
2026-09-17 — no new input surface, dependency, `unsafe`,
serialization, or file I/O; pure state machines; codec rejects corrupt
input per the persistence contract; no findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | State machine implemented with unit tests: occupancy change → correct mode; no global multiplier path exists | done | `occupancy_table_drives_mode` (deep/in-band/ramp/ceilings) + `advance_equals_manual_substepping` (bit-exact, no shortcut scaling) | ANALYST _(signed 2026-09-17)_ |
| 2 | Orbital period in real-time mode matches analytic value within 0.1% | done | `realtime_orbit_matches_analytic_period` (crossing-detected T within 0.1% of 2π) + `slew_transition_keeps_energy_bounded` | ANALYST _(signed 2026-09-17)_ |
| 3 | Compression state round-trips through save/load | done | `snapshot_round_trips_and_rejects_corrupt` (bit-exact 24-byte codec + BadLength/BadMode/BadValue rejection + restore path) — payload level, save module embeds later | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, or I/O; codec rejects corrupt input | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine time::` green: occupancy table, slew
  properties, substep equivalence, sim-time pin, period 0.1%, slew
  stability bound, codec round trip + rejection, in-band forcing.
- Physics dt per substep ≤ max_dt ALWAYS (asserted in `advance`
  tests); ratio trajectory Lipschitz in real time.
- Doc tests for all new public `engine::time` APIs.
- PO tolerance 0.1% used for DoD 2 (spec §10 addendum numbers now
  filled — "TBD" resolved).

## Risks & Next steps

- Slew + substeps cost ticks under high compression (bounded by
  max_dt choice — the tick owner (`free-flight-navigation` loop)
  sizes it; budgets in `quality.md` apply at integration).
- Snapshot codec is version-stamped by shape: any field change needs
  a format version + migration note for `autosave-persistence`.
- Next and last: `free-flight-navigation` (consumes frames + kepler +
  integrator + handoff events + compression clock + craft envelope;
  wires `in_blend_band` from handoff weight; implements fly-to replan
  on `Entered`).
