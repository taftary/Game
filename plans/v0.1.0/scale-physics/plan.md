# Plan — scale-physics

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Units + potential (risk-first: analytic gradients)

Owning module: new `engine::physics` (pure math over `glam` `f64`,
headless-testable, deterministic; no GPU, no I/O — same purity bar as
`engine::frames`). No new dependencies. No `unsafe`. This feature does
**not** create `engine::sim` (colonies/tick) — pure models only; tick
wiring lands with `time-compression` / `free-flight-navigation`.

- `units`: `FrameUnits` per `frames::FrameId` — length/time/mass scales
  + exact `g_local` derived from SI constants (`G`, `M_SUN`, `M_EARTH`,
  `AU_M`, `DAY_S`). Spec §5 says solar-system `G ≈ 1`; the exact value
  in AU/day/solar-masses is `k² = 2.95912208e-4` (Gaussian constant) —
  exactness wins over the prose example; recorded as a
  spec-clarification below, not an ADR (no locked choice overturned).
- `potential`: Miyamoto-Nagai disk + NFW halo with spec §5 formulas and
  `a = 6.5 kpc, b = 0.26 kpc, r_s = 20 kpc`. Masses are not in the spec:
  provisional reference `M_disk = 6.5e10 M☉`, `M_vir = 1.0e12 M☉`
  (consistent with `r_s = r_vir/c`, `c = 12`); pinned by the
  `v(8 kpc) ∈ [200, 250] km/s` test, tunable later. Analytic
  acceleration (−∇Φ) included — needed by the integrator and later by
  `soi-handoff` blending.
- Invariant row: none of the rendering/determinism/save invariants are
  touched (no render, no I/O, deterministic pure functions).

### Phase 2 — Integrator + Kepler solver

- `integrator`: velocity Verlet (kick-drift-kick) generic over an
  acceleration closure + Euler baseline for the stability comparison.
- `kepler`: closed-form two-body solver (elements ⇄ state vector,
  Newton-solved Kepler equation, period `2π√(a³/μ)`). Ship dynamics
  consumer is `free-flight-navigation`; patched-conic blending consumer
  is `soi-handoff`.

### Phase 3 — Ephemeris interface + proper motion + populations

- `ephemeris`: `EphemerisTable` trait
  (`position(body, days_since_j2000) → (DVec3 /*AU*/, Validity)`) +
  built-in `KeplerianTable` (JPL Keplerian elements + rates, embedded;
  validity 1800–2050 CE, out-of-window flagged). Full VSOP87-series and
  DE440 providers land with `star-catalog-streaming` (v0.2.0) — the
  trait is the handoff. ARCHITECT answers the notion's open question:
  precision mode is a **runtime** `EphemerisMode::{Standard}`
  (`#[non_exhaustive]`, `HighPrecision` added with the provider).
- `proper_motion`: linear Gaia extrapolation from DR3 epoch 2016.0,
  ±1000-year validity window + `BeyondWindow` flag (ADR-020).
- `populations`: procedural-body orbital-element distributions
  (Kepler-inspired priors: log-flat periods, Rayleigh eccentricity,
  isotropic inclination) sampled from `core::SeededRng`. The
  domain-separated seed stream wires in with `hierarchical-seeding`
  (next feature) — recorded handoff, not scope creep.
- `static_field`: cosmic-web row = `StaticDensityField` marker
  ("no live gravity by design") + static-contract test.

### Phase 4 — Docs + bindings

- `docs/techstack/simulation.md`: per-frame unit table (DoD 3).
- ADR-018 `draft` → binding. ADR-020 stays `draft` (binds when
  `star-catalog-streaming` lands too — its own status line says so).
- Milestones table → `done`; version bump.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| PHY-001 | done | `FrameUnits` per frame + exact `g_local` (solar `k²`, planetary `μ⊕` spot-checks) | FR (unit tables), DoD 3 |
| PHY-002 | done | MN disk + NFW halo potential + analytic acceleration; rotation-curve tests (self-consistency 1%, flatness, `v(8kpc)`) | FR, DoD 1 |
| PHY-003 | done | Velocity Verlet + Euler baseline; 1000-orbit stability test (`< 1e-4` vs ≥100× divergence) | Goals 2, DoD 2 |
| PHY-004 | done | Kepler solver (elements ⇄ state, period within 0.1%) | FR, DoD 1 |
| PHY-005 | done | `EphemerisTable` trait + `KeplerianTable` provider + runtime `EphemerisMode`; closure/apsides tests | FR, DoD 1 |
| PHY-006 | done | Proper-motion extrapolation + validity window + warning flag | FR, DoD 1 |
| PHY-007 | done | Procedural orbit distributions from seed (deterministic snapshot test) | Goals 1, Constraints, DoD 1 |
| PHY-008 | done | `StaticDensityField` marker + static-contract test | Goals 1, DoD 1 |
| PHY-009 | done | `simulation.md` per-frame unit table | DoD 3 |
| PHY-010 | done | ADR-018 → binding (ADR-020 stays draft); milestones; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`): same list as
`frame-hierarchy` (`plan.md` there).

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — phases map to
`engine::physics` submodules, pure-math only; spec-clarifications (G
prose, provisional masses, Keplerian stand-in provider) recorded above,
no locked choice overturned; no invariant touch)_ · Todos approved by:
TECHLEAD _(signed 2026-09-17 — risk-first (gradients + stability
before providers); each todo independently verifiable with a named
test; no budget risk (no draw calls/tick cost — offline math);
1000-orbit test is milliseconds of pure arithmetic)_ · UX acceptance
rows: _(n-a — no player-facing surface)_ · DoD verified by: ANALYST
_(signed 2026-09-17 — every row re-checked; gate suite re-run green:
fmt, clippy `-D warnings`, workspace build, workspace tests
32+118+18+126, doc tests 6+28, `game` / `game_debug --headless` /
`game_tools --headless --tier low` runs; E2E n-a beyond the scripted
`game` journey (pure-math feature, no I/O); audit findings below were
fixed in-DEV, no shipped defect, no issues filed)_ · Security reviewed
by: SECURITY _(signed 2026-09-17 — no new input surface, dependency,
`unsafe`, serialization, or file I/O; pure math over `glam` + std; no
findings)_.

## Audit findings (fixed in-DEV, recorded for process)

- A1 — inconsistent mass constants: rounded M☉/M⊕ with CODATA G
  shifted solar periods at 3e-5. Fixed by deriving masses as μ/G from
  standard gravitational parameters. Lesson: cross-check constants
  against each other, not just against themselves.
- A2 — `MAS_PER_RAD` 1000× slip (µas value): self-consistent unit
  tests passed; the doctest's absolute physical anchor (1 arcsec =
  4.848e-6 rad) caught it. Lesson: every unit module needs at least
  one absolute anchor, and it is now policy in this plan's tests.
- A3 — ephemeris closure floors: JPL rate tables embed real
  perturbations (pure-Kepler T fails at 1e-4) and elements evolve per
  orbit (outer planets drift to 1e-2). Test asserts closure within the
  table's own secular-drift bound — principled, documented in-test.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Each spec §5 table row has an implemented model + regression test (rotation curve flatness, proper-motion displacement, ephemeris reference positions, Kepler period) | done | `cargo test -p game_engine physics::` — 23 passed: potential (finite-diff gradient, flatness in [0.85, 1.15], `v(8kpc)` in [200, 250] km/s, closed-form 1%), Kepler period 365.25 d ±0.1%, ephemeris 8-planet drift-bound closure + Earth apsides + window flags, proper-motion 1% + window, populations snapshot, static contract | ANALYST _(signed 2026-09-17)_ |
| 2 | Symplectic integrator demonstrated stable vs Euler baseline | done | `verlet_holds_energy_over_a_thousand_orbits` (drift < 1e-4) + `euler_visibly_diverges_on_the_same_orbit` (Euler ≥100× Verlet, direct comparison) | ANALYST _(signed 2026-09-17)_ |
| 3 | `simulation.md` documents the per-frame unit table | done | `docs/techstack/simulation.md` § *Per-frame unit systems* (6 rows + local-G column) | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, serialization, or I/O — pure math | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine physics::` green: potential (3 tests),
  integrator stability, Kepler period (0.1%), ephemeris closure,
  proper-motion displacement (1%) + window flag, populations snapshot,
  static contract, unit spot-checks.
- PO tolerance table (`Resolved for v0.1.0`) met row-by-row.
- Doc tests for all new public `engine::physics` APIs.
- No `engine::sim` created; no render/save/input touched.

## Risks & Next steps

- Provisional masses (`M_disk`, `M_vir`) + Kepler-inspired priors are
  reference values, tunable when catalog data lands; the tests pin
  behavior, not truth.
- `KeplerianTable` is a documented stand-in (arcminute-class,
  1800–2050 CE); VSOP87/DE440 providers land with
  `star-catalog-streaming`. `EphemerisMode` is `non_exhaustive` for that.
- `hierarchical-seeding` (next) wires domain-separated seeds into
  `populations`; until then it samples `core::SeededRng` directly.
- Next: `soi-handoff` consumes `potential::acceleration` + the 10⁻³
  rule; `time-compression` consumes the integrator contract;
  `free-flight-navigation` consumes `kepler` + craft envelope.
