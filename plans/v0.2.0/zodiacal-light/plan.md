# Plan — zodiacal-light

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Pure analytic kernel (`engine::render::zodiacal`)

Owning module: new `engine::render::zodiacal` (pure math, headless-testable,
no GPU — same shape as `render::depth`). One new dependency use (`glam`
`DVec3`, already in the tree). No `unsafe`, no I/O, no input surfaces.

- `ZodiacalParams { mu0, a_beta, beta0_deg, a_theta, k }` +
  `spec_defaults()` (23.0, 1.5, 20°, 0.5, 0.7 per spec §9.4) + validated
  `new()` (`None` on non-finite / non-positive scales).
- `surface_brightness_mag(beta_rad, theta_rad, params)`: the spec §9.4
  magnitude-offset formula. Term bounds hold by construction
  (latitude term ∈ [0, Aβ], forward term ∈ [Aθ/(1+2k), Aθ]); the final μ
  is clamped to `[MU_BRIGHTEST, MU_FAINTEST] = [20.0, 24.0]` as a
  bug-catching rail (wider than the model's own [21.0, 22.8] range —
  documented as guard, not physics).
- `radiance_relative(mu) = 10^(-0.4·mu)`: linear proportional radiance.
  Absolute zero-point calibration is exposure's job; this satisfies the
  spec §9.4 rule (linear **before** tone mapping) at the interface.
- Invariant row: touches no camera/projection/picking code — pure
  functions over caller-supplied directions. No rendering invariant
  at risk.

### Phase 2 — Ecliptic frame + elongation (J2000)

- `ECLIPTIC_OBLIQUITY_DEG = 23.43928` (J2000 mean ecliptic).
- `ecliptic_coords(dir_equatorial: DVec3) -> (beta, lambda)` for unit
  vectors; `solar_elongation(dir, sun_dir) -> f64` (clamped acos).
- Contract boundary (documented, not implemented): the module starts at
  equatorial J2000 unit vectors; engine world ↔ equatorial mapping is
  owned by `star-catalog-streaming` (catalog frames). No placeholder
  world transform is invented here.
- Known-answer test: J2000 ecliptic north pole
  (RA 270°, Dec +66.5607°) → β ≈ +90°.

### Phase 3 — Stable source interface + validation hook

- `ZodiacalSource` trait (`radiance(beta, theta) -> f64`); `AnalyticZodiacal`
  (params-driven) implements it; a constant `StubMapZodiacal` in tests
  proves the swap. `exposure-tone-mapping` / `depth-cueing` depend on
  the trait, never the struct — the future HEALPix map replacement is a
  new impl, no interface churn (DoD-3).
- Validation hook (DoD-2, "documented before release"): module docs pin
  the reference comparison — Leinert et al. 1998 diffuse-sky values via
  the S10→mag/arcsec² conversion — plus the procedure (photographic
  comparison of pole + elongation falloff). Full comparison is pre-release
  work per spec §9.4, not this feature.
- Known model limit (recorded, not fixed): the spec formula saturates
  near the Sun (θ → 0 gives μ ≈ 21, real F-corona far brighter); valid
  range documented as θ ≳ 15°.

### Phase 4 — Docs + lifecycle

- ADR-021 `draft` → binding (this feature lands its zodiacal section;
  exposure/cueing implement their sections under it next).
- `docs/techstack/rendering.md`: one-line sky-model note pointing at the
  module (no table — the falloff grid lives pinned in module docs +
  tests).
- Milestones v0.2.0 table `zodiacal-light` → `done`;
  `docs/techstack/README.md` version bump (0.22.0).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| ZDL-001 | done | `ZodiacalParams` + `spec_defaults()` + validated `new()` + μ clamp rail | Goals 2–3, Functional requirements |
| ZDL-002 | done | Offset formula; pinned pole (≈22.7) + ecliptic-quadrature (≈21.2) values; monotonic falloff tests | DoD 1 |
| ZDL-003 | done | `radiance_relative` linear output + exposure-handoff contract doc | Goals 4 |
| ZDL-004 | done | J2000 obliquity + `ecliptic_coords` + `solar_elongation` + pole known-answer test | Functional requirements |
| ZDL-005 | done | `ZodiacalSource` trait + `AnalyticZodiacal` + stub swap test | DoD 3 |
| ZDL-006 | done | Validation hook doc (Leinert reference + procedure) | DoD 2 |
| ZDL-007 | done | ADR-021 → binding; rendering.md note; milestones table; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`,
`cargo run -p game_tools -- --headless --tier low`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new pure
module `engine::render::zodiacal`; dependency direction legal (glam
only, reads nothing from frames/assets/navigation); no locked choice
changed (spec §9.4 formula implemented as written); no binding
invariant touched (no camera/projection/picking code); trait boundary
protects the future map replacement; foundations before surfaces
(kernel → frame → interface))_ · Todos approved by: TECHLEAD _(signed
2026-09-17 — todos risk-first (formula + pinned values before frame
math and docs), each independently verifiable with a named test; no
budget risk (pure f64 math, no draw calls, no tick cost, no memory);
no GPU/binary surface added — correct, since no sky renderer exists to
wire to; gate list linked above)_ · UX acceptance rows: _(n-a — pure
source term, no UI; recorded in notion.md `Roles`)_ · DoD verified by:
ANALYST _(signed 2026-09-17 — every row re-checked, gate suite re-run
green on this tree; DoD-1 = `spec_falloff_pins_pole_and_quadrature`
(pole 22.69, quadrature 21.21, antisolar 22.78, all ±0.05) +
`falloff_is_monotonic_toward_plane_and_sun` +
`clamp_rail_never_triggers_inside_model_range`; DoD-2 = validation hook
present in module docs (Leinert 1998 + S10 conversion + procedure),
full comparison correctly deferred to pre-release per spec §9.4; DoD-3 =
`interface_accepts_stub_map_asset_unchanged` (one consumer fn, both
impls, no call-site change); no-binary-change acceptance confirmed via
headless outputs below)_ · Security reviewed by: SECURITY _(signed
2026-09-17 — no new input surface, dependency (`glam` already in tree),
`unsafe`, serialization, file/network I/O, or GPU code; pure f64 math
with NaN-safe rails; no findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Model implemented with spec initial parameters; ecliptic-pole and low-elongation captures match expected falloff | done | Pinned falloff values + monotonicity sweep (`render::zodiacal::tests`) | ANALYST _(signed 2026-09-17)_ |
| 2 | Validation against reference observations (COBE/DIRBE or HST background models) documented before release (spec §9.4) | done | Validation hook in module docs (Leinert 1998 reference + S10 conversion + procedure) | ANALYST _(signed 2026-09-17)_ |
| 3 | Swap test: interface accepts a stub map-based asset unchanged | done | `interface_accepts_stub_map_asset_unchanged` (trait consumer over analytic + stub) | ANALYST _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine render::zodiacal::` green: params
  validation, formula pins, monotonic falloff, J2000 pole known-answer,
  elongation pins, trait swap test.
- Pole value within 0.05 of the hand-computed 22.69 (β=90°, θ=90°);
  ecliptic quadrature within 0.05 of 21.21 (β=0°, θ=90°).
- Doc tests for all new public APIs (`quality.md` test policy).
- No binary/GPU changes: `game_tools --headless --tier low` output
  byte-identical apart from timing.

## Risks & Next steps

- Near-Sun saturation (θ ≲ 15°): spec-formula limit, documented in
  module docs; exposure-tone-mapping must not trust the source term
  inside that cone (recorded handoff constraint).
- Absolute radiance zero point is deliberately relative here; the
  exposure feature owns photometric calibration — interface carries
  the contract, not the constant.
- World ↔ equatorial mapping stays with `star-catalog-streaming`; if
  that feature's frame choice disagrees with J2000-mean here, the
  adapter lives there, never as an edit to this module.
- Next feature: `exposure-tone-mapping` (consumes this trait).
