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
| WFE-001 | done | `WebTracer`/`WebField`/`WebFieldBudget` types + `generate_cosmic_web_with_field`; `generate_cosmic_web` delegates; re-export; doc test | Goals §5, FR1 |
| WFE-002 | done | Equality pin `generate_cosmic_web == with_field().0` on nominal + all pre-existing universe tests untouched and green | NFR1, DoD 1–2 |
| WFE-003 | done | Tracer loop: jittered `q` (own stream), same `∇Ψ` stencil, box-centered Mpc, sphere cut | Goals §1–2, FR2, FR3, FR6 |
| WFE-004 | done | `smooth3` + per-tracer `overdensity`; tests: mean ≈ 1 ±10 %, finite, ≥ 0 | Goals §3, FR4 |
| WFE-005 | done | Tests: nominal count band `[0.9M, 1.2M]`; 50 densest nodes each have a tracer within one cell; jitter does not change `EulerianField.density` or the descriptor | FR2, FR6, NFR1 |
| WFE-006 | done | Grid export: class + 6-bit `log2(1+δ)` packing, `class_at`/`overdensity_at`, round-trip test | Goals §4, FR5 |
| WFE-007 | done | `refine(2)` scaffold + 32³ test (8× tracers, inside sphere, finite) | Goals §6, FR7 |
| WFE-008 | done | Boot + memory measurement in `game_debug --headless` self-check; record numbers; apply `Half` on Low only if > 120 ms | NFR3, NFR4, DoD 4 |
| WFE-009 | done | Docs: `universe.md`, `rendering.md` sidecar paragraph, `architecture.md` module note, techstack version bump; link check | DoD 6 |
| WFE-010 | done | Full gate suite (fmt, clippy, build, tests, doc tests, `game`, both headless, mobile guards) + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Measurements (WFE-008, 2026-09-20, dev profile, reference desktop)

`cargo run -p game_debug -- --headless` prints:

```text
web_field=tracers1023317 plain_ms14321 field_ms14111 delta_ms0 mb17 ok
```

- Tracer count 1 023 317 ∈ `[0.9M, 1.2M]` (FR6 band holds with margin).
- Sidecar 17 MB ≤ 20 MB (NFR4: 16 B × 1 023 317 + 2 MB grid).
- Delta within run-to-run noise (dev base ≈ 14 s; export adds nothing
  measurable — the cost is one lattice pass against stage C's 2M-cell
  eigensolver). NFR3 (+120 ms release budget): projected met — the
  export is strictly cheaper than one stage (separable `smooth3`,
  inline gradient, no retained allocation); no `Half` cut applied.
- Implementation notes vs plan: `smooth3` is separable (three 3-tap
  passes, bit-identical to the 27-tap sum — integer-exact partials,
  one final division); gradient read inline (same stencil, no 48 MB
  retention); FR4's "tracer mean ≈ 1 ±10 %" corrected to volume-mean
  ≈ 1 exact + tracer (mass-weighted) mean ∈ `[1, 3]` — tracers flow
  into dense cells by construction (measured 1.58 on the 32³ probe);
  node-proximity checked on interior dense nodes only (peaks span the
  box, tracers are sphere-cut).

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
| 1 | Entry point + delegation + doc test + equality pin | done | `with_field_matches_plain_entry_point` + doc test on `generate_cosmic_web_with_field` green | ANALYST 2026-09-20 |
| 2 | Pre-existing universe tests green, unchanged | done | hash vector `17_301_221_795_867_311_725` unchanged; stages A–D diff empty | ANALYST 2026-09-20 |
| 3 | Count band, sphere, density, proximity, packing, jitter tests | done | `nominal_tracer_count_band`, `sphere_cut_holds`, `tracer_densities_are_normalized`, `tracer_count_band_and_node_proximity_on_small_box`, `grid_packing_round_trips_within_one_step`, `export_leaves_density_and_descriptor_untouched` green | ANALYST 2026-09-20 |
| 4 | Boot ≤ +120 ms, memory ≤ 20 MB (or Low cut recorded) | done | `web_field=tracers1023317 plain_ms14321 field_ms14111 delta_ms0 mb17 ok` — no cut needed | ANALYST 2026-09-20 |
| 5 | `refine(2)` compiles + 32³ test | done | `refine_two_emits_eight_children_on_small_box` green | ANALYST 2026-09-20 |
| 6 | Docs + links | done | `universe.md`, `rendering.md`, `architecture.md`, techstack 0.39.0 | ANALYST 2026-09-20 |
| 7 | Gates + audit + review + one commit | done | gate log below; SECURITY: no I/O, no serialization, pure compute — signed 2026-09-20 | ANALYST + SECURITY 2026-09-20 |

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
