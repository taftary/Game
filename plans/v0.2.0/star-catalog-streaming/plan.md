# Plan — star-catalog-streaming

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — HEALPix core (`engine::catalog::healpix`)

Owning module: new `engine::catalog` (pure math, headless-testable, no
GPU, no I/O — same shape as `render::depth` / `render::bands`). No new
dependencies (std only), no `unsafe`, no input surfaces.

- NESTED scheme, orders 12–14 (`nside = 1 << order`,
  `npix = 12 * nside²`): `ang2pix(ra, dec)`, `pix2ang(pixel)`,
  `neighbors8(pixel)`, nested-range query for cone + frustum polygon
  (range-set math, never enumerate 2×10⁸ pixels).
- Index math is `f64`; outputs are integer pixel ids, so the 1-ulp
  cross-platform float rule (`core::rng` docs) never reaches a hash.
- Pinned by structural tests (pix→ang→pix round-trip, neighbor
  symmetry, children partition the parent, pole/equator known answers)
  plus committed vectors captured on first run (labeled as such, same
  convention as `seeding.rs`).
- Invariant row: pure index math — touches no camera, projection,
  picking, or marker code.

### Phase 2 — Tile format v1 + cooker (`engine::catalog::format`, `game_tools catalog`)

- Binary tile v1, little-endian fixed layout: magic `GSCT`, version
  `u16 = 1`, order `u8`, nested pixel `u64`, source count `u32`,
  mean G magnitude + mean BP–RP `f32`, epoch `f64`, catalog-version
  string; records `source_id u64, ra/dec f64, pm_ra/pm_dec f32
  (mas/yr), g_mag f32, bp_rp f32`. Decode validates magic / version /
  bounds and returns a typed error — corrupt bytes never panic
  (SECURITY input surface: truncated / wrong-magic / wrong-version /
  oversized-count fixtures in tests).
- `CATALOG_VERSION` stamps (`gaia-dr3-synth/1.0` synthetic,
  `gaia-dr3/1.0` real extract) feed `SeedMetadata::catalog_version`
  (ADR-020, ADR-004 metadata already reserved).
- Cooker `game_tools catalog --out <dir> --order 12 [--synthetic N]
  [--csv <path>]`: deterministic synthetic Gaia-like generator
  (seeded, realistic density tiers incl. galactic plane) + minimal CSV
  extract ingest (`source_id,ra,dec,pmra,pmdec,g,bp_rp`); writes tiles
  + manifest (tile list, counts, hashes). Same inputs ⇒ byte-identical
  output. First real consumer of `docs/techstack/assets.md`'s cooker
  pipeline (that doc gains the catalog section).

### Phase 3 — Cache + loader + scheduler (`engine::catalog::{cache, io, scheduler}`)

- `TileCache`: `HashMap<TileKey, Slot>` with byte accounting (decoded
  + GPU-resident estimate via the uniform record stride), 2 GB ceiling
  enforced on insert (lowest-priority evicted first), `memory_bytes()`
  accessor (the `hexsphere` pattern) + `tracing` events.
- Loader: fixed `std::thread` pool (2 workers floor, mobile-safe),
  `mpsc` request/result channels, file reads off-thread; main-thread
  `poll()` is non-blocking — no sim-tick coupling, decode stays pure.
- Scheduler: desired set from the frustum query over the
  caller-supplied view-projection + active frame + `ShipState.vel`
  (travel direction); priority frustum → travel → distance; cancels
  stale requests.
- Latency: per-tile `Instant` timers into a dependency-free fixed-bucket
  histogram; `tracing` p50/p95 per flush; <100 ms local-load budget
  asserted in tests (fault-injectable loader covers the slow path).
- Frustum helper: pure planes-from-`Mat4` in `catalog` — operates on a
  caller-supplied matrix, never touches camera code (invariant-safe).

### Phase 4 — Identity + fallback (`engine::catalog::{ids, fallback}`, seeding hook)

- `StarId`: one `Copy + Hash + Ord` ID space —
  `Catalog { source_id }` / `Fallback { tile, slot }` — with canonical
  `Display` (`gaia-dr3/<source_id>`, `fallback/<order>/<pixel>/<slot>`);
  `resolve(StarId, &store)` returns astrometry regardless of backing,
  so the swap is invisible to selection, focus, and navigation state.
- Fallback: tile metadata (count + mean color) + seeded stream on
  domain `catalog/fallback/v1/<order>/<pixel>` (via `region_seed` with
  `StellarNeighborhood` cell `[order, pixel, 0]` — the ADR-019 grammar
  with real semantics); golden-angle Fibonacci lattice on the tile's
  spherical quad + magnitude scatter around the mean. Pure,
  deterministic, quantized where hashed.
- Swap protocol: 200 ms deadline from request → fallback renders; on
  catalog arrival the slot swaps atomically keyed by `StarId`.
- ADR-019 hook: `exclusion_zones(tile)` feeds `ExclusionZone`s so
  bright catalog sources suppress procedural cells around them.

### Phase 5 — Space sky surface (`engine::render::stars`, `game_debug` backdrop)

- `equatorial_to_world`: J2000 equatorial unit vector → engine world
  direction in the viewing frame. Owns the mapping `zodiacal.rs`
  explicitly deferred here (adapter lives here, never as an edit to
  that module). Pure + known-answer tests (poles, equator points).
- `render::stars`: Backdrop-band star layer — tile → point-vertex
  expansion (camera-relative `f32` via `recenter`, color from BP–RP +
  G, size/alpha from magnitude), per-tile subbuffers with wholesale
  re-upload on change (the `upload_map` pattern), ~300 ms
  fallback→catalog crossfade (UX note).
- First live surface: `game_debug` planet-viewer backdrop pass
  (Backdrop bucket already leads `plan_passes`); `tools` viewer wiring
  deferred to keep scope (notion non-goal discipline).
- Invariant rows: projection untouched (reuses the un-flipped
  `directx::perspective`); point sprites carry no winding risk; NDC
  `+1` = top and picking paths untouched; ENU frame untouched.

### Phase 6 — Proper-motion plumbing + DoD evidence + lifecycle close

- Record → `ProperMotion` conversion (f32→f64 widening documented),
  epoch from the tile header; `extrapolate` at game-time; `OutOfWindow`
  → `tracing` warning + debug-overlay flag (player HUD flag is the
  recorded `navigation-hud` handoff).
- DoD evidence: (1) order-12 fixture streams within budget with metrics
  log; (2) `tests/` integration with a delayed loader — fallback
  ≤200 ms, seamless swap, `StarId` stable (first real `game_tests`
  content); (3) decode→`ProperMotion`→`extrapolate` round-trip within
  the 1% PO tolerance.
- ADR-017 + ADR-020 `draft` → binding; `assets.md` cooker section live;
  milestones table → `done`; `docs/techstack/README.md` version bump
  (0.23.0); notion ↔ plan sync; full `quality.md` gate list green.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| SCS-001 | done | HEALPix core (`ang2pix`/`pix2ang`/`neighbors8`/range query, orders 12–14) + structural + committed-vector tests | Goals 1, Functional requirements |
| SCS-002 | done | Tile format v1 encode/decode + magic/version/bounds validation + corrupt-fixture tests | Goals 1–2, NFR |
| SCS-003 | done | `game_tools catalog` cooker: synthetic generator + CSV ingest + tiles/manifest, deterministic | Goals 1–2, DoD 1 |
| SCS-004 | done | `CATALOG_VERSION` stamps + `SeedMetadata` wiring | Constraints & Assumptions |
| SCS-005 | done | `TileCache` + byte accounting + 2 GB ceiling + eviction + `memory_bytes()` | Goals 2, NFR |
| SCS-006 | done | Thread-pool loader + priority scheduler (frustum→travel→distance) + <100 ms local-load test | Goals 2, NFR |
| SCS-007 | done | Latency histogram + `tracing` p50/p95 flush | NFR |
| SCS-008 | done | Pure frustum-from-view-proj helper | Goals 2 |
| SCS-009 | done | `StarId` ID space + canonical display + backing-independent `resolve` | Goals 4, DoD 2 |
| SCS-010 | done | Deterministic fallback generator + 200 ms swap protocol | Goals 3, DoD 2 |
| SCS-011 | done | ADR-019 `exclusion_zones` catalog lookup hook | Constraints & Assumptions |
| SCS-012 | done | `equatorial_to_world` J2000 mapping + known-answer tests | Functional requirements |
| SCS-013 | done | `render::stars` Backdrop layer + debug planet-viewer wiring + ~300 ms crossfade | Goals 1–3, Users / Stakeholders |
| SCS-014 | done | Record → `ProperMotion` plumbing + ±1000 yr validity surfacing | DoD 3 |
| SCS-015 | done | DoD evidence (fixture stream test, `tests/` forced-miss integration, PM round-trip); ADR-017/020 → binding; docs sync; gates green | DoD 1–3, NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`,
`cargo run -p game_tools -- --headless --tier low`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — single new pure
module `engine::catalog` + `engine::render::stars`; dependency direction
legal (`engine` reads `frames`/`physics`/`seeding`, binaries call down;
`game` stays vulkano-free); no locked choice changed (std threads, no
new dependency — `stack.md` untouched); binding invariants listed
explicitly per phase (determinism: fallback pure + loader off-tick;
rendering: Backdrop + log-depth, no projection/picking changes; save:
`catalog_version` into the reserved `SeedMetadata` field, shape frozen
for `autosave-persistence`); phase order dependency-safe (index →
format → runtime → identity → surface → evidence))_ · Todos approved by:
TECHLEAD _(signed 2026-09-17 — todos risk-first (index + format
validation before loader/threads and surfaces), each independently
verifiable with a named test; budget-checked (2 GB ceiling enforced in
code per SCS-005, tier-gate: Low tier uses order 12 + smaller resident
set; no draw-call/tris impact beyond point sprites inside the existing
Backdrop pass; loader threads off the 20 Hz tick so <8 ms sim budget is
untouched); gate list linked above)_ · UX acceptance rows: _(signed
2026-09-17 — never-blank sky ≤200 ms; ~300 ms crossfade hides the swap
pop; tile state dev-overlay only; PM-validity warning overlay +
`tracing`; no new controls)_ · DoD verified by: ANALYST _(signed
2026-09-17 — every row re-checked and reproduced on this tree: DoD-1 =
`dod1_order12_tile_set_streams_within_budget` (8 tiles / 24k records /
0.92 MB / p50-p95 2 ms) + cache ceiling/eviction units; DoD-2 =
`dod2_forced_miss_falls_back_fast_and_swaps_stably` + registry swap
unit; DoD-3 = `dod3_epoch_proper_motion_plumbs_end_to_end` +
`records_plumb_to_proper_motion_within_tolerance` (1% + window edges);
gate suite re-run green on this tree (fmt, clippy `-D warnings`,
workspace build/test/doc, `game` / `game_debug --headless` (sky line:
6763 fallback units, 471817 stars, model-only) / `game_tools
--headless --tier low` byte-identical hash, both mobile guards);
save paths untouched (`SeedMetadata` shape unchanged); corrupt-input
fixtures green (tile/CSV/manifest); explicit non-blocking follow-ups:
full-sky real import, galaxy-map rewiring, `BodyId` swap, HUD validity
flag, order-13/14 dense tiering with real data)_ ·
Security reviewed by: SECURITY _(signed 2026-09-17 — deep review (new
file-I/O surfaces): tile decode validates magic/version/bounds with a
count cap + input-bound allocation (no panic path, fixtures for
truncated/wrong-magic/wrong-version/oversize/NaN); CSV ingest validates
header/fields/ranges + duplicate ids with line errors; manifest parser
is a strict subset (pixels/counts/dups/consistency, trailing-garbage
rejected); loader/cooker paths join numeric ids only (no traversal);
no new dependency (`Cargo.lock` delta = `glam` wired into `game_tests`
only, already in tree); no new `unsafe` in `engine` (verified by grep;
debug draw reuses the pre-existing SAFETY-comment pattern); save +
migration untouched; verdict pass with two notes (manifest fnv
content-verified at tile decode, not at parse; cooker RAM scales with
input size — dev tool, local-user trust))_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Order-12 tile set streams within budget on the reference tier (evidence: memory + latency metrics) | done | `game_tests::dod1_order12_tile_set_streams_within_budget`: 8 dense order-12 tiles (24,000 records, 0.92 MB) stream via scheduler + threaded loader into the 2 GB cache; every tile < 100 ms, p50/p95 2 ms (bucket-edge estimates over sub-ms loads); `TileCache` ceiling + eviction unit tests | ANALYST _(signed 2026-09-17 — reproduced green; evidence line captured from `--nocapture` run)_ |
| 2 | Forced-tile-miss test shows fallback inside 200 ms and seamless replacement; IDs stable across the swap | done | `game_tests::dod2_forced_miss_falls_back_fast_and_swaps_stably`: absent file → loader error → same `StarId::fallback` resolves lattice inside the 200 ms policy deadline; after arrival it refines to the catalog counterpart in-tile; unit miniature `fallback_ids_survive_the_catalog_swap` | ANALYST _(signed 2026-09-17 — reproduced green)_ |
| 3 | Gaia DR3 epoch/proper-motion fields plumbed to `scale-physics` | done | `game_tests::dod3_epoch_proper_motion_plumbs_end_to_end` + `format::records_plumb_to_proper_motion_within_tolerance`: decode → `ProperMotion` → `extrapolate` within the 1% PO tolerance; `epoch_validity` flags the ±1000 yr edges | ANALYST _(signed 2026-09-17 — reproduced green)_ |

## Acceptance criteria

- `cargo test -p game_engine catalog::` green: index round-trips +
  neighbor symmetry + partition, format validation + corrupt fixtures,
  cache ceiling + eviction, scheduler priority order, fallback
  determinism + swap, `StarId` resolve stability, PM conversion.
- `cargo test -p game_tests` green: forced-miss integration (delayed
  loader ⇒ fallback ≤200 ms ⇒ swap ⇒ `StarId` unchanged).
- `game_tools catalog --synthetic` output byte-identical across runs;
  decode of its tiles round-trips every record field.
- Doc tests for all new public `engine` APIs (`quality.md` test policy).
- `game_tools --headless --tier low` output unchanged apart from timing
  (no binary/GPU behavior change outside the new code paths).
- No new dependency in `Cargo.lock`; no new `unsafe`; no `Debug`-format
  or wall-clock input in any generation path.

## Risks & Next steps

- Synthetic-only data may hide perf cliffs: the cooker generates
  realistic density tiers (galactic plane) so order-13 paths are
  exercised before real data arrives.
- GPU buffer duplication counts toward the 2 GB — SCS-005 accounting
  covers decoded + GPU-resident bytes by construction.
- 256 px point-sprite cap may clip bright stars: `exposure-tone-mapping`
  owns photometric calibration; this feature carries positions + colors,
  not the zero point (recorded handoff, same pattern as zodiacal-light).
- Full-sky real import, galaxy-map rewiring, `BodyId` canonical swap,
  and player HUD validity flag are explicit follow-ups (see notion
  non-goals + handoffs), never scope creep here.
- Process: v0.2.0 features land directly on `main` (log-depth,
  zodiacal, this feature) by decision 2026-09-17, deviating from
  `plans/README.md` §7 (version branch); rule amendment left to PO.
- Next feature: `exposure-tone-mapping` (consumes catalog star colors +
  the zodiacal source term).
