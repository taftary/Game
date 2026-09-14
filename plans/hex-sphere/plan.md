# Plan — hex-sphere

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — primal mesh (icosphere)

`engine::hexsphere` module; icosahedron builder; subdivision with
edge-midpoint dedup (BTreeMap, deterministic); spherical projection
after every level; N parameter (default 6).

### Phase 2 — relaxation

Fixed Lloyd iterations (K = 7, pinned per universe_version); neighbor
adjacency from primal edges; reproject to sphere each pass.

### Phase 3 — dual mesh

Cells (center = primal vertex), corner rings (face centroids,
`[u32; 6]` + sentinel), neighbor lists (`[u32; 6]` + sentinel),
render-ready accessors; per-cell storage documented (~3.5 MB at N=6).

### Phase 4 — verification + docs

Topology suite (12 pentagons, Euler = 2, manifold, counts
V = 10·4^N+2 / F = 20·4^N, N = 0..=6); determinism hash (quantized,
committed expected value); memory/time measurement; ADR-002; docs sync;
full gates + mobile compile-guard.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| HEX-001 | done | Create `crates/engine/src/hexsphere/` module + wire into `lib.rs`; `HexSphere::generate(n: u32, radius: f32)` signature, module docs | Goals |
| HEX-002 | done | Icosahedron builder: 12 canonical vertices (golden ratio), 20 faces, normalized to R | Functional requirements |
| HEX-003 | done | Subdivision: triangle → 4 via edge midpoints, BTreeMap dedup, N times (default 6); normalize every vertex after each level | Functional requirements |
| HEX-004 | done | Fixed Lloyd relaxation: K = 7 const, adjacency from primal edges, move toward neighbor centroid + reproject each pass | Functional requirements |
| HEX-005 | done | Dual mesh: corner verts (face centroids), per-cell rings `[u32; 6]` + `u32::MAX` sentinel, neighbor lists, accessor iterators hiding sentinels; positions/rings render-ready | Functional requirements |
| HEX-006 | done | Topology tests: exactly 12 pentagons, Euler V−E+F = 2, closed manifold, 5/6 neighbors, count formulas, N = 0..=6 | Functional requirements |
| HEX-007 | done | Determinism: quantize positions to fixed-point i32, FNV-1a mesh hash, repeated-run test with committed expected hash | Non-functional requirements |
| HEX-008 | done | Measure + document per-cell storage (~3.5 MB target) and N=6 generation time (<1 s, informational) | Non-functional requirements |
| HEX-009 | done | Write ADR-002 (hex-dominant geodesic dual = planet representation); check off in decisions README | Goals |
| HEX-010 | done | Docs sync: rendering.md far-field line, architecture.md `hexsphere/` row, techstack version bump; links resolve | DoD |
| HEX-011 | done | Run full quality.md gates + mobile compile-guard; record evidence below | Non-functional requirements |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Pipeline in `engine::hexsphere`, pure + deterministic, N default 6 | done | `crates/engine/src/hexsphere.rs` + `lib.rs` wiring; `HexSphere::generate`; 11 unit tests green |
| 2 | Topology tests: 12 pentagons, Euler = 2, manifold, 5/6 neighbors, N range | done | `exactly_twelve_pentagons`, `euler_characteristic_holds` (corners − E + cells = 2), `every_edge_has_two_faces_and_outward_normals`, `counts_match_formulas` — all N = 0..=6 |
| 3 | Determinism test: identical hash across runs, committed expected hash, quantization | done | `committed_hash_matches`: hash 11459543604394007386 (N=6, R=1.0); `deterministic_across_repeated_runs` (4× equal); quantized FNV-1a |
| 4 | Per-cell storage documented + within budget; O(n) passes; no heavy physics | done | `memory_bytes` = 3,440,768 B (3.28 MiB) at N=6, asserted < 8 MB; O(n) passes only; no physics code |
| 5 | ADR-002 written + linked; rendering.md/architecture.md updated; techstack bumped; links resolve | done | `docs/decisions/ADR-002.md`; decisions README checked; rendering.md far-field line; architecture.md `hexsphere/` row + boundary bullet; techstack 0.2.2 |
| 6 | All quality.md gates green; evidence recorded per criterion | done | This table + §Evidence |

## Acceptance criteria

- `HexSphere::generate(6, r)` yields a closed manifold with exactly
  12 pentagons and counts matching 10·4^N+2 / 20·4^N formulas.
- Mesh hash identical across repeated runs; expected hash committed and
  verified on Windows/Linux/macOS.
- All quality.md gates green, incl. mobile compile-guard; no new
  clippy/fmt violations; no behavior change to existing binaries.

## Risks & Next steps

- Risk: cross-platform float drift breaks the committed hash →
  mitigation: hash is over quantized fixed-point; if drift appears,
  quantize stored positions too and re-pin (risks/README #2).
- Risk: pentagon sentinel leaks into gameplay code → accessors expose
  iterators over valid neighbors/corners only; sentinel never public.
- Risk: Lloyd quality insufficient near pentagons → escape hatch per
  notion Open questions: spherical Laplacian hybrid, fixed iteration
  count, re-pinned hash. Only on measured failure.
- Next: M1 renderer smoke consumes `HexSphere` as planet mesh source
  (validates Low-tier tri budget); surface layers (elevation, biomes)
  land as separate plans features.

## Evidence

- Local gates (2026-09-14, Windows, rustc stable): `cargo fmt --check`
  clean (3 auto-format diffs applied); `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` clean; `cargo build
  --workspace` Finished; `cargo test --workspace --all-targets` —
  11/11 hexsphere tests pass, all other targets empty; `cargo test
  --doc --workspace` — 1/1 (`HexSphere` doc example); `cargo run --bin
  game` + `cargo run -p game_debug --bin game_debug` exit 0.
- Mobile guard local: `cargo check --workspace --target
  aarch64-linux-android` + `--target aarch64-apple-ios` — both pass
  (targets pre-installed).
- Mesh numbers (N=6, R=1.0): hash 11459543604394007386 (pinned in
  `committed_hash_matches`); 40,962 cells + 81,920 corners; 3,440,768
  bytes (3.28 MiB); generated in 1.26 s in the unoptimized dev-profile
  test build — informational; release/phone-class numbers come from the
  ADR-007 harness later.
- Implementation note: the Euler test initially asserted the wrong
  formula (test bug, not mesh bug — the correct invariant is
  corners − links/2 + cells = 2); the hash was pinned after the first
  green run under `GEOMETRY_VERSION = 1`.
