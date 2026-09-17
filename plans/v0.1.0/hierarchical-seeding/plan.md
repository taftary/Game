# Plan — hierarchical-seeding

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Derivation kernel (`engine::seeding`)

Owning module: new `engine::seeding` (pure, deterministic, no I/O —
same purity bar as `engine::frames` / `engine::physics`). No new
dependencies, no `unsafe`.

- `RegionId { frame: FrameId, cell: [i64; 3] }`: frame encoded as an
  explicit discriminant + body id (no `Debug`-format dependence);
  cell semantics stay with the generators (HEALPix tiles, chunk ids).
- `region_seed(master: u64, region: RegionId) -> RegionSeed` via the
  existing `core` kernel (`SeededRng::stream` = FNV-1a domain mix +
  SplitMix64 avalanche, pinned by reference-vector tests). This answers
  the notion's open hash-algorithm question at plan time: **no xxHash /
  no new dep** — derivation is not hot, and a new hash would add audit
  surface for zero behavioral gain. Domain grammar (the stable
  contract): `seed/v{SEED_VERSION}/frame/{d}/body/{b}/cell/{x},{y},{z}`.
- `RegionSeed` is **opaque** (no raw-`u64` accessor): the only way out
  is `layer(name: &str) -> SeededRng` — "raw region seeds never used
  directly" enforced by the type system, not by convention.
- `SEED_VERSION: u32` stamps the algorithm; any grammar change bumps it
  (old saves fork explicitly, never silently).
- Invariant rows: determinism contract (`core` docs) inherited;
  quantized outputs before hashing/storing (populations already does).

### Phase 2 — Override + metadata shapes

- `ExclusionZone { center: [i64; 3], radius_cells: u64 }` +
  `is_suppressed` / `suppressed_by_any` (u128 distance math, no
  overflow): the check generators run **before** the procedural hash.
  The catalog lookup feeding zones lands with `star-catalog-streaming`.
- `SeedMetadata { master_seed, seed_version, catalog_version }`
  (frozen shape for `autosave-persistence`, same pattern as
  `FrameTransition`): catalog version `None` = procedural-only.
- Golden-angle lattice primitives stay with the per-layer generators
  (notion non-goals), not this contract.

### Phase 3 — Tests + bindings

- Committed snapshot vectors for `region_seed` (algorithm-stability
  tripwire, same convention as `universe::hash` committed vectors).
- Decorrelation: no shared stream prefixes across layers + Pearson
  |r| < 0.1 over 10k samples.
- Determinism soak: draw from a layer stream, rebuild from scratch,
  identical (the depart-and-return narrative, pinned).
- Populations handoff test: `sample_orbit` from a layer stream is
  deterministic (closes the `scale-physics` plan's recorded handoff).
- ADR-019 `draft` → binding; milestones → `done`; version bump.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| SED-001 | done | `RegionId` + explicit frame encoding + `SEED_VERSION` stamp | FR (derivation APIs) |
| SED-002 | done | `region_seed` derivation + committed snapshot vectors | FR (stable hash + version), DoD 1 |
| SED-003 | done | Opaque `RegionSeed` + `layer()` + decorrelation tests (prefix + Pearson) | Goals 2, DoD 1 |
| SED-004 | done | `ExclusionZone` + `suppressed_by_any` + override test (center/edge/outside) | Goals 3, FR, DoD 2 |
| SED-005 | done | `SeedMetadata` frozen shape (ADR-004) | FR, Constraints |
| SED-006 | done | Determinism soak test (depart-and-return identical) | Goals 4, DoD 3 |
| SED-007 | done | Populations handoff test (layer stream → deterministic orbit) | Constraints (links `scale-physics`) |
| SED-008 | done | ADR-019 → binding; milestones; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV: same list as `frame-hierarchy`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new
pure module; hash-algorithm question decided (reuse `core` kernel, no
new dep); frame encoding explicit; metadata shapes frozen for
downstream consumers; no invariant touch)_ · Todos approved by:
TECHLEAD _(signed 2026-09-17 — kernel first, shapes second, tests
third; every todo has a named test; no budget risk (offline
derivation, no tick cost))_ · UX acceptance rows: _(n-a)_ · DoD verified by: ANALYST
_(signed 2026-09-17 — every row re-checked; gate suite re-run green:
fmt, clippy `-D warnings`, workspace build, workspace tests
32+118+18+135, doc tests 6+29, `game` / `game_debug --headless` /
`game_tools --headless --tier low` runs; derivation path audited for
non-determinism: no `Debug` formatting, no time/thread/pointer input,
`format!` over integers only (stable); cross-run determinism holds by
construction + soak test; no findings)_ · Security reviewed by:
SECURITY _(signed 2026-09-17 — no new input surface, dependency,
`unsafe`, serialization, or file I/O; pure derivation over the pinned
`core` kernel; no findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Derivation API + domain-separated sub-seeds with tests showing decorrelation between layers | done | `cargo test -p game_engine seeding::` — 9 passed: committed snapshot (4041527373913777303 / 7033640996740128932), 64-draw no shared prefix, Pearson \|r\| < 0.1 over 10k, frame/body/cell independence | ANALYST _(signed 2026-09-17)_ |
| 2 | Real-data override test: catalog object suppresses procedural cell content inside its exclusion radius | done | `override_suppresses_inside_radius_only` (center/edge/outside + empty set + i64/u64 extremes without overflow) | ANALYST _(signed 2026-09-17)_ |
| 3 | Determinism soak evidence (regenerate-after-departure identical) | done | `determinism_soak_regenerate_after_departure` (64-draw rebuild identical) + `layer_stream_drives_populations_deterministically` | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, serialization, or I/O — pure derivation | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine seeding::` green: snapshot vectors,
  decorrelation, override boundaries, soak, populations handoff,
  metadata shape.
- No `Debug`-format or pointer-dependent input anywhere in the
  derivation path (audit explicitly).
- Doc tests for all new public `engine::seeding` APIs.
- `populations::sample_orbit` untouched (consumer-side proof only).

## Risks & Next steps

- Grammar changes fork the universe: `SEED_VERSION` bump is mandatory
  with any derivation change; catalog-version semantics (`None` vs
  `Some`) are owned by `star-catalog-streaming`.
- Next: `soi-handoff` (consumes frames + physics), `time-compression`
  (consumes integrator + frames), `free-flight-navigation` (consumes
  all + craft envelope).
