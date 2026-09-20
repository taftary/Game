# Plan — web-field-export

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes (module + invariant annotations): all work inside
`crates/engine/src/universe/web/` — new module `field_export.rs`
(sidecar struct + build), one-line delegation in `mod.rs`, re-export in
`universe/mod.rs`. Dependency direction unchanged (`universe` imports
nothing from `render`). **Invariant rows:** determinism of hashed
paths (stages A–D untouched, pinned by the existing hash vector);
`generate_cosmic_web` byte-identical (new equality pin);
`UNIVERSE_VERSION` unchanged; no serialization (`WebField` derives no
save trait). ADR-025 is the governing decision.

### Phase 1 — Sidecar types + entry point (`field_export.rs`, `mod.rs`)

`WebTracer`, `WebField`, `WebFieldBudget { Full, Half }`;
`generate_cosmic_web_with_field` runs A → B → C → D exactly as today,
then calls `export_field(&potential, &euler, &classified, params,
seed, budget)` and returns both. `generate_cosmic_web` = wrapper that
drops `.1`. Doc test on a 32³ box. Equality pin test on nominal.

### Phase 2 — Tracer export (`field_export.rs`)

Second loop over the lattice: jittered `q` (stream
`"cosmic_web/tracer"`, Irwin–Hall-3, σ ≈ 0.3 cell) → `x = q − D₊·∇Ψ`
(same central differences as `displace.rs:47-55`) → box-centered Mpc
→ sphere cut → `overdensity` from the 3³-smoothed NGP grid. Budget
`Half` keeps even `(x+y+z)` parity cells. Tests: count band, sphere
cut, density mean ≈ 1, node proximity, jitter never touches
`EulerianField`/descriptor.

### Phase 3 — Grid export (`field_export.rs`)

`smooth3` (periodic box mean, reuse `field::blur_axis` pattern with
radius 1) → `log2(1+δ)` quantized to 6 bits over `[−4, +6]` → packed
with the 2-bit class. Helpers + round-trip test (pack → unpack within
one quantization step; class exact).

### Phase 4 — `refine(2)` scaffold

Trilinear `∇Ψ` at 8 sub-cell offsets → 8 sub-tracers per lattice
tracer; 32³ unit test only. Not wired to any tier here.

### Phase 5 — Measurement + docs + gates

Boot delta (`Instant` around both entry points in the `game_debug
--headless` self-check; printed as `web_field=… tracers … ms … MB`);
docs sweep; full gate list; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| WFE-001 | pending | `WebTracer`/`WebField`/`WebFieldBudget` types + `generate_cosmic_web_with_field`; `generate_cosmic_web` delegates; re-export; doc test | Goals §5, FR1 |
| WFE-002 | pending | Equality pin `generate_cosmic_web == with_field().0` on nominal + all pre-existing universe tests untouched and green | NFR1, DoD 1–2 |
| WFE-003 | pending | Tracer loop: jittered `q` (own stream), same `∇Ψ` stencil, box-centered Mpc, sphere cut | Goals §1–2, FR2, FR3, FR6 |
| WFE-004 | pending | `smooth3` + per-tracer `overdensity`; tests: mean ≈ 1 ±10 %, finite, ≥ 0 | Goals §3, FR4 |
| WFE-005 | pending | Tests: nominal count band `[0.9M, 1.2M]`; 50 densest nodes each have a tracer within one cell; jitter does not change `EulerianField.density` or the descriptor | FR2, FR6, NFR1 |
| WFE-006 | pending | Grid export: class + 6-bit `log2(1+δ)` packing, `class_at`/`overdensity_at`, round-trip test | Goals §4, FR5 |
| WFE-007 | pending | `refine(2)` scaffold + 32³ test (8× tracers, inside sphere, finite) | Goals §6, FR7 |
| WFE-008 | pending | Boot + memory measurement in `game_debug --headless` self-check; record numbers; apply `Half` on Low only if > 120 ms | NFR3, NFR4, DoD 4 |
| WFE-009 | pending | Docs: `universe.md`, `rendering.md` sidecar paragraph, `architecture.md` module note, techstack version bump; link check | DoD 6 |
| WFE-010 | pending | Full gate suite (fmt, clippy, build, tests, doc tests, `game`, both headless, mobile guards) + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — additive change inside
`engine::universe::web`; hashed stages untouched; sidecar never
hashed/saved; ADR-025 governs) · Todos approved by: TECHLEAD
(2026-09-20 — risk-first: equality pin (WFE-002) lands before any
export code so a regression is caught on the first commit; boot
measurement (WFE-008) has a recorded cut (`Half`) instead of a
renegotiation) · UX acceptance rows: n-a · DoD verified by: ANALYST
_(pending)_ · Security reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Entry point + delegation + doc test + equality pin | pending | test names + `cargo test -p game_engine --lib universe::web` output | ANALYST _(pending)_ |
| 2 | Pre-existing universe tests green, unchanged | pending | `git diff --stat` on `mod.rs` tests = 0 lines changed; hash vector value | ANALYST _(pending)_ |
| 3 | Count band, sphere, density, proximity, packing, jitter tests | pending | test list green | ANALYST _(pending)_ |
| 4 | Boot ≤ +120 ms, memory ≤ 20 MB (or Low cut recorded) | pending | `web_field=` headless log line | ANALYST _(pending)_ |
| 5 | `refine(2)` compiles + 32³ test | pending | test name | ANALYST _(pending)_ |
| 6 | Docs + links | pending | file list, link check | ANALYST _(pending)_ |
| 7 | Gates + audit + review + one commit | pending | gate log, commit hash | ANALYST + SECURITY _(pending)_ |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `web_hash` vectors, `UNIVERSE_VERSION`, content IDs, save
  envelope: unchanged [T: existing pins + WFE-002 equality pin].
- A-2. Stages A–D source lines unchanged except the `mod.rs`
  delegation (`git diff` on `field.rs`, `displace.rs`, `classify.rs`,
  `descriptor.rs` = empty).
- A-3. `WebField` has no `Serialize`/save derive and is not reachable
  from `engine::save`.
- A-4. `engine::universe` imports nothing from `engine::render`; `game`
  crate diff empty.
- A-5. Export jitter uses its own domain-separated stream and is
  consumed in canonical lattice order (replay pin).
- A-6. Tracer frame = node frame (FR2 proximity test).

## Risks & Next steps

- R-1 (boot): a second 2M-cell loop with jitter RNG ≈ 60–100 ms
  single-threaded; if the measured delta exceeds 120 ms, `Half` on Low
  (recorded), and the render features already tier tracer counts
  downstream.
- R-2 (lattice imprint survives jitter): σ 0.3 cell may still show a
  faint grid in deep voids on the `slab` preset. First mitigation is
  σ 0.3 → 0.45 (constant, no scope change); second is a per-tracer
  alpha floor in the splat shader (render feature). Judged from
  `cosmic-tracer-splat` shots, not here.
- R-3 (density speckle): 3³ smoothing may under-resolve node cores
  (peak density flattened). Acceptable — hub cores come from
  `cosmic-hub-hierarchy` impostors, not from tracer color.
- Next: `cosmic-tracer-splat` consumes `tracers`; `cosmic-gas-veil-v2`
  consumes `grid`; `cosmic-hub-hierarchy` uses tracers within `r_vir`
  for member-galaxy placement.
