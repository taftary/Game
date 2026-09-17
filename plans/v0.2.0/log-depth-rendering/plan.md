# Plan — log-depth-rendering

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Pure log-depth kernel (`engine::render::depth`)

Owning module: new `engine::render::depth` (pure math + GLSL source
composition, headless-testable, no GPU — same shape as `render::checker`:
Rust computes, tests pin, shaders embed). No new dependencies (`glam`
unused here — `f64` scalars only). No `unsafe`, no I/O, no input
surfaces.

- `LogDepthParams { far_plane: f64 }` + `fcoef()` =
  `1.0 / log2(far + 1.0)` (Vulkan Z ∈ [0, 1] form of the Outerra
  coefficient; OpenGL's `2.0 / …` variant is explicitly out).
- `encode(w) = log2(1 + w) * fcoef`, clamped to [0, 1]; `decode(d)`
  inverse for tests. Monotonic increasing in view-space `w`; `w = 0 → 0`,
  `w = far → 1`.
- GLSL sharing the `checker::glsl_const_block` precedent: a
  `LOG_DEPTH_EPILOGUE` snippet (operates on `gl_Position` at the end of
  `main`, reads push-constant `log_far`) plus
  `glsl_log_depth_push_decl()` for the push-block field, and a composed
  `planet_vert_logdepth()` source (base body + epilogue — one authored
  body, never two hand-duplicated literals). All composed sources must
  compile through `compile_glsl_to_spirv` to floor-safe SPIR-V
  (naga, ≤ 1.3); literal drift guards pin Rust consts against shader
  literals.
- Invariant row: the encoding changes **depth writes only** — NDC
  conventions untouched (NDC +1 = top, RH `directx::perspective`,
  no Y-flip). `projection_uses_vulkan_ndc` passes unmodified.
  `FrontFace::CounterClockwise` + `CullMode::Back` untouched.

### Phase 2 — Band scheduler (`engine::render::bands`)

Owning module: new `engine::render::bands` (pure pass-planning, same
headless shape as `depth`). Consumes `engine::frames::FrameId` as the
active-scale key (read-only — no `frames` logic changes).

- `DepthBand` enum (backdrop / far / mid / near-field …), `DepthMode`
  (`None` backdrop, `Log`, `LinearTight`), `BandPass { band, near, far,
  mode, clear_depth }`.
- `plan_passes(active_frame, visible_bands) -> Vec<BandPass>`:
  far→near order, depth cleared between bands, near-field carries tight
  near/far — never one global range (ADR-016).
- Per-pair-of-scales band-sharing table as data (which layers share a
  depth pass), tier-parameterizable: exact band boundaries per tier stay
  open (notion open question; ADR-007 perf data pending) so the table
  ships Low-tier defaults + override hooks, not baked numbers.
- Pipeline-kind classification rows for the existing consumers (map
  backdrop = no-depth-write stays a backdrop band; planet fill; line
  overlay; UI) — the table documents the full structure even where
  wiring lands later.

### Phase 3 — Pipeline integration (`game_tools` smoke)

GPU wiring lives in the binaries (stack rule: `game`/`tools`/`debug`
own `vulkano` pipeline code; `engine` stays GPU-free). Minimal,
default-pixel-preserving:

- `game_tools` smoke gains a D32F (`D32_SFLOAT` — mandatory format, no
  extension, inside the Vulkan 1.1 floor of ADR-007) depth attachment
  and a log-depth planet variant built from
  `depth::planet_vert_logdepth()` + `Fcoef` in push constants.
- Default smoke pixels unchanged (linear path kept; log variant behind
  a flag) — the M1 smoke contract holds.
- Non-goal restated: no `game_debug` viewer-wide conversion; the viewer
  keeps D16 + current passes. Viewer conversion is future work for the
  consuming features.

### Phase 4 — Multi-decade test scene + evidence

- Seeded demo scene in `game_tools` (windowed flags + headless
  scheduler print): near-coplanar surface pairs at decade-separated
  view distances, plus a 3-band composition (backdrop shell / mid /
  near-field).
- DoD-1 evidence: (a) headless separability oracle — the same vertex
  pair discriminates under `encode` but collides under linear D16/D32F
  (`Less` compare), i.e. fights-before / resolves-after computed, not
  eyeballed; (b) windowed before/after captures observed at review.
- DoD-2 evidence: headless pinned `plan_passes` output for the demo
  scene (exact pass list, clear ops, near/far) + windowed 3-band
  occlusion capture.

### Phase 5 — Docs + lifecycle

- `docs/techstack/rendering.md`: new *Depth bands & log-depth* section
  with the band-sharing table (DoD-3) + the log-depth/projection
  invariant note.
- `docs/techstack/quality.md`: depth-pass budget rows (max extra
  passes per tier, folded into the draw-call budget — ADR-016
  consequence).
- ADR-016 `draft` → binding; milestones v0.2.0 table
  `log-depth-rendering` → `done`; `docs/techstack/README.md` version
  bump.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| LDP-001 | done | `LogDepthParams` + `fcoef`/`encode`/`decode` (f64); monotonicity + endpoint tests | Goals 1, Functional requirements |
| LDP-002 | done | Separability oracle: decade-separated near-coplanar pair resolves under log, collides under linear D16/D32F | DoD 1 |
| LDP-003 | done | GLSL epilogue + push decl + composed `planet_vert_logdepth()`; naga compile-to-floor-safe-SPIR-V tests; literal drift guards | Functional requirements |
| LDP-004 | done | `bands`: `DepthBand`/`DepthMode`/`BandPass`, `plan_passes` ordering + clear-between-bands invariants, sharing table as data (Low-tier defaults) | Goals 2–3, DoD 2 |
| LDP-005 | done | `game_tools`: D32F depth attachment + log-depth planet variant (flag-gated); default smoke pixels unchanged | DoD 1, NFR, Constraints |
| LDP-006 | done | Multi-decade demo scene (windowed + headless print); before/after z-fight captures | DoD 1 |
| LDP-007 | done | 3-band composite path + occlusion evidence (pinned scheduler plan + capture) | DoD 2 |
| LDP-008 | done | Band-sharing table in `rendering.md` | DoD 3 |
| LDP-009 | done | `quality.md` depth rows; ADR-016 → binding; milestones table; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`,
`cargo run -p game_tools -- --headless --tier low`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — phases map to
new pure `engine::render::{depth,bands}` + binary-side wiring in
`game_tools` only; dependency direction legal (engine stays GPU-free,
`bands` reads `frames::FrameId` without touching frame logic); locked
choices respected (GLSL→naga, Vulkan 1.1 floor, D32F mandatory format);
invariants listed explicitly (projection/NDC/winding untouched,
`projection_uses_vulkan_ndc` unmodified); foundations before surfaces
(kernel → scheduler → wiring → evidence))_ · Todos approved by: TECHLEAD
_(signed 2026-09-17 — todos risk-first (encoding math + separability
oracle before any GPU wiring), each independently verifiable with a
named test or pinned output; budget risk checked (≤ 3 passes demo,
folded into draw-call budget; D32F depth image only in the flagged
variant); gate list linked above)_ · UX acceptance rows: _(n-a — no
player-facing surface; recorded in notion.md `Roles`)_ · DoD verified
by: ANALYST _(signed 2026-09-17 — every row re-checked, gate suite
re-run green on this tree: fmt, clippy `-D warnings`, workspace build,
workspace tests 32+118+18+176, doc tests 6+44, `game` /
`game_debug --headless` / `game_tools --headless --tier low` runs;
DoD-1 = oracle tests (`oracle_linear_depth_z_fights_at_decade_range`,
`oracle_log_depth_separates_on_float_buffer`,
`oracle_log_depth_needs_float_buffer_at_decade_range`) + `--log-depth`
windowed run on Intel UHD 620 (pipelines + 3 passes created, 20 s
frames, validation layers silent); DoD-2 = pinned
`mixed_scene_plans_far_mid_near_in_order` + `sharing_table_spot_checks`
+ the same GPU run executing Backdrop→Mid→Near; DoD-3 = rendering.md
table present; scope decision recorded: visual flicker-demo at smoke
scale omitted (both encodings resolve there — physically
meaningless); full N-band windowed compositing stays with the
consuming features)_ · Security reviewed by: SECURITY _(signed
2026-09-17 — no new input surface (one `--log-depth` flag, windowed
only, rejected with `--headless`), dependency, `unsafe` (two
pre-existing `ShaderModule::new` sites reused via the shared
constructor, same SAFETY basis), serialization, or file/network I/O;
GLSL is composed from constants and naga-validated before upload; no
findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Log-depth active with before/after z-fight evidence at a multi-decade test scene | done | `oracle_*` tests (linear D16/D32F collide, log D32F separates on 10 km @ 1e9 m, near 0.1, far 1e12) + `--log-depth` GPU run (Intel UHD 620, validation-clean) | ANALYST _(signed 2026-09-17)_ |
| 2 | Multi-band composite renders correct occlusion across at least 3 bands (screenshot/headless assert) | done | Headless: `mixed_scene_plans_far_mid_near_in_order` (exact `[Far, Mid, Near]`, all cleared) + `sharing_table_spot_checks`; GPU: Backdrop→Mid(log planet)→Near(linear quad) executes, validation-clean | ANALYST _(signed 2026-09-17)_ |
| 3 | Band-sharing table documented in `rendering.md` | done | `rendering.md` § *Depth bands & log-depth* (bucket rule + per-frame range table + pipeline-kind rows) | ANALYST _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine render::depth::` green: `fcoef`/endpoint/
  monotonicity + separability oracle (log resolves, linear D16/D32F
  collide on the same pair).
- `cargo test -p game_engine render::bands::` green: far→near order,
  clear between bands, sharing-table lookups, no global range.
- Composed log-depth vertex shader compiles via `compile_glsl_to_spirv`
  to SPIR-V ≤ 1.3 (same `assert_valid_spirv` bar as planet shaders).
- `game_tools` default smoke pixels unchanged; `--headless --tier low`
  stays green; log variant + demo scene behind flags.
- Projection invariant: `projection_uses_vulkan_ndc` passes unmodified;
  `FrontFace::CounterClockwise` + `CullMode::Back` in all touched
  pipelines.
- Doc tests for all new public `engine::render::{depth,bands}` APIs
  (`quality.md` test policy).

## Risks & Next steps

- Vertex-only `log(z)` mis-interpolates across large triangles at
  grazing incidence (known Outerra caveat — tessellated planet mesh
  mitigates; backdrop quads are the exposure). Fallback kept in scope:
  a fragment `gl_FragDepth` correction variant behind the same kernel;
  the DoD-1 evidence gate decides whether it is needed. No scope creep
  beyond that switch.
- Exact band boundaries per tier stay open (notion open question):
  table ships Low-tier defaults + override hooks; ADR-007 device data
  refines them later — recorded as accepted gap, not a blocker.
- `game_debug` viewer conversion (D16 → log-depth passes) is explicitly
  out; `star-catalog-streaming` + `exposure-tone-mapping` consume the
  `bands` scheduler and `depth` kernel as-is.
- Depth precision at the extreme: `f32` varyings carry `gl_Position`;
  the kernel documents the `w`-range where `f32` interpolation error
  stays below one depth quantum — measured in LDP-002, not assumed.
- Next features in version order: `zodiacal-light` (small, feeds two
  features), `exposure-tone-mapping`, `depth-cueing` (visual-style PO
  decision due), `star-catalog-streaming` (network/content PO decision
  due), `waypoint-transitions` (composes all).
