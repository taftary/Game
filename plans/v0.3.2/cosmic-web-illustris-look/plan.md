# Plan — cosmic-web-illustris-look

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes (module + invariant annotations; dependency-safe order:
measure → foundations before surfaces; shell intact throughout;
`engine::universe` untouched — no boundary crossing, no new ADR beyond
an ADR-023 extension note):

### WS0 — Low-tier baseline (`tools`, `debug` headless)

Pin current `Low` tris/draws/overdraw (inspector stacked-px count) via
the existing `game_tools --headless --tier low` + `cosmic_layout`
asserts before adding strands. Output: baseline numbers recorded in
DoD-3 evidence; cut triggers armed. Invariant rows: none touched
(measurement only).

### WS1 — Branching skeleton (`debug::cosmic_web`, pure CPU)

COWS-lite medial-axis thinning of the `FILAMENT|NODE` cell mask (integer
BFS from the classified web, periodic wrap precedent from `field.rs`;
`border_cells() = 0` holds) → 1-cell spine segments + bifurcation ids.
Deterministic per `(seed, web)`; consumes no RNG stream (topology is a
pure function of the mask); empty mask yields empty output. Feeds WS2–4
and nothing else (never selection, flight, or saves). Invariant row:
"descriptor, hashes, content IDs untouched — enrichment only [T:
existing `web_hash` vectors green]".

### WS2 — Hair threads (`debug::cosmic_web::StrandRecord` + `main.rs` ribbon verts)

Per spine segment, `3–7` frayed sub-strands via seeded midpoint
displacement on the `braid_point` trunk (per-link sub-stream
`seed ^ (a << 32 | b)`, `BRAID_STREAM` precedent — ribbon and grain
derive identical strands independently). On-GPU expansion as
camera-facing ribbons, half-width `0.1–0.4 Mpc`, `1–1.5 px` minimum,
shader-side melt taper into hubs. Cost control: `BRAID_SUBDIVISIONS`
`10→6–8` + distance/frustum cull; strand records stay `~100 B`/strand.
Invariant rows: "projection un-flipped `directx::perspective` [T]",
"`FrontFace/CCW` unchanged (points/lines exempt) [T: existing pins]".

### WS3 — Smoke v2 sheath (`debug::cosmic_web::SmokePuff` + `SMOKE_VERT`/`SMOKE_FRAG`)

Extend `SmokePuff` with link-tangent axis (into `misc`, `48 B` layout
grows only if needed — else repack seed); vertex shader builds
`axis=tangent`, `side=normalize(cross(view, axis))` quads (`3–8 Mpc`
along, `0.3–1.0 Mpc` across); jitter `1.0→0.35 Mpc`; two-tier alpha
(core bright + halo `×0.5`, halo may vanish subpixel); fragment:
anisotropic falloff + `8×8` `fract`-hash dust (no `sin`/`exp` per pixel).
Keep: `2 px` min-world clamp (core tier), `1→6 Mpc` near-eye fade,
bounded redshift `z ≤ 0.5`, per-surface exposures, bloom write-once.
Invariant row: "every HDR bloom target written once, then only read
(UHD 620 rule) [T: HdrChain untouched]".

### WS4 — Gold beads + Illustris grade (`grain_cloud`/`node_impostors`/`glow_point_cloud` + `COSMIC_DEMO/MAP_*`)

Sub-threshold peaks (below `peak_density_ratio`, today discarded)
emitted as dwarf sprites along spines under a new `cosmic_web/bead`
stream with the two-pass budget pattern (cap recorded in code;
pick-ignored — `select_at` still iterates `web.nodes` only). Grade:
thread brightness by segment density, bead/hub hue by `mass_level`,
faint threads to the `[0.008, 0.005, 0.024]` clear. Invariant rows:
"marker only via `world_to_pixels` [T]", "picking only via
`project_to_screen` + `ndc = (2u−1, 1−2v)` [T: existing pins]".

### WS5 — Gates + docs sweep + ADR-023 extension note (repo-wide)

Full gate list from `docs/techstack/quality.md` green; A/B shots
(`target.jpeg` vs new build) attached as DoD-1 evidence; docs sweep
(`rendering.md` cosmic section, `2026-09-19-cosmic-web-visual-architecture.md`
follow-up note, `techstack/README.md` version bump, milestones row);
link check; ANALYST DoD audit + SECURITY review; single `done` commit
on `v0.3.2`.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CWI-001 | completed | Pin Low baseline (tris/draws/inspector stacked-px) via `game_tools --headless --tier low` + `cosmic_layout` asserts; record numbers | NFR2, NFR3 |
| CWI-002 | completed | COWS-lite skeleton (spines + bifurcations, integer BFS, periodic wrap) + replay-identical + empty-mask tests | Goals §1, FR1 |
| CWI-003 | completed | Hair sub-strands (`3–7`/segment, midpoint displacement, per-link sub-stream) + replay/bound tests | Goals §2, FR2 |
| CWI-004 | completed | Ribbon shader (thin width, `1–1.5 px` min, melt taper, `SUBDIV→6–8`, distance/frustum cull) + tris assert | Goals §2, FR2, NFR2 |
| CWI-005 | completed | Smoke-v2 records (tangent axis, stretched sizes, tightened jitter, two-tier alpha) + replay/budget tests | Goals §3, FR3 |
| CWI-006 | completed | Smoke shaders (aniso falloff, `8×8 fract` dust; keep fade/redshift/exposures/bloom rule) + headless layout assert | Goals §3, FR3, FR5, NFR4 |
| CWI-007 | completed | Bead emission (`cosmic_web/bead` stream, two-pass cap, pick-ignored) + replay/budget tests | Goals §4, FR4 |
| CWI-008 | completed | Illustris grade pass (density→brightness, mass→hue, void sink, hub amber) on `COSMIC_DEMO/MAP_*` consts | Goals §5, FR5 |
| CWI-009 | completed | Visual A/B + full gate suite green (`fmt`, `clippy -D warnings`, `build`, workspace + doc tests, `game` run, `game_debug --headless`, `game_tools --headless --tier low`, mobile guards) | DoD 1–4 |
| CWI-010 | completed | Docs sweep + ADR-023 extension note + version bump + link check; ANALYST audit + SECURITY review; single `done` commit | DoD 5 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — render-only enrichment
in `debug`; `engine::universe`/`game`/save paths untouched; no new
pipelines/deps/`unsafe`; invariants pinned per-WS above) · Todos
approved by: TECHLEAD (2026-09-20 — risk-first: WS0 measure, WS1
foundation before WS2–4 surfaces, WS5 gates last; every todo
independently verifiable; budgets + cut order recorded: beads → halo →
strands; gate commands listed in CWI-009) · UX acceptance rows
(player-facing): approved 2026-09-20 — see Acceptance criteria UX-1…UX-4 ·
DoD verified by: ANALYST (2026-09-20, retroactive at the v0.3.3 cut —
rows below; the windowed pixel A/B was judged from the recorded
grading rounds D-1…D-7 and found *closer but not close*, which is the
finding that triggered ADR-025) · Security reviewed by: SECURITY
(2026-09-20, retroactive — no new deps, no `unsafe`, no I/O, shaders
compiled from inline strings as before).

Recorded deviation: the feature was committed to `main` (squash
`e60a036`) with the audit rows still pending; they were completed at
the v0.3.3 cut against the code as landed. See `plans/README.md`
§ *7. Version branch* (2026-09-20 deviation).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Branching hair threads + beads, A/B closer to target | completed — with finding | Headless `cosmic_layout=smoke51953 grain800000 beads59958 impostors18000 ok` (baseline was `smoke51849`); 19/19 lib enrichment tests incl. fray/bifurcation/bead/gold tests; sheath + bead shaders compile (`viewer_shaders_compile`). ANALYST finding: thread width down, void darkness up, hub gold purity up as required, but the structural gap (straight links, no walls, filled voids, 6000 white pins) remains — recorded in report §11, handled by v0.3.3 | ANALYST |
| 2 | Descriptor/hashes/content IDs identical, no version bump | completed | `engine::universe` untouched by this feature (`git diff` shows only `debug` + docs); 46/46 engine universe tests green incl. `committed_web_vectors_pin_stage0` + all determinism/hash tests; 81/81 doc tests green | ANALYST |
| 3 | Low budgets hold or tier-gated/cut | completed | `game_tools --headless --tier low` green; no new pipelines/draws; puff count stable (51953 ≤ 65k, ≈104k tris < 500k Low); points +60k beads (≈2 MB/surface, app memory gate untouched); fallback order armed, nothing triggered | ANALYST |
| 4 | New enrichment tested (replay + bounds + rebase-safe) | completed | 4 new tests (fray arms, bifurcations, sub-segments, beads) + updated bands (strand 7+3, smoke width, tangent-unit); 187/187 `game_debug` lib + 28/28 bin tests green (incl. extended `cosmic_shader_safety_pins`) | ANALYST |
| 5 | Docs sweep + links + audit + review, one `done` commit | completed | Docs: `rendering.md` paragraphs, report §10, techstack `0.37.0`, ADR-024 + log row, milestones row; full gate suite green (fmt, clippy `-D warnings`, build, workspace tests, doc tests, `game` run, both headless, both mobile guards). Commit: squash `e60a036` on `main` (deviation recorded above) | ANALYST + SECURITY |

## Implementation notes (DEV 2026-09-20 — accepted deviations, scope unchanged)

- D-1 (WS2 ribbons): the ribbon pipeline is retired — no `LineList`
  cosmic draw exists (`strand_records` feeds tests/grain-shape only).
  Reviving it would violate the no-new-pipelines non-goal, so hair
  multiplicity is delivered as strand count `1–3 → 3–7` across one or
  two fray arms, visible through the live smoke/grain/bead samplers
  (all derive `braid_shape`). `SUBDIV`/cull work is moot and stays a
  roadmap follow-up.
- D-2 (WS1 mask): enrichment receives only `WebDescriptor` (no cell
  mask); mask thinning would require an engine change (rejected by the
  non-goals). The skeleton derives from the link graph instead:
  degree-`≥3` bifurcations + seeded spine sub-segments — deterministic,
  render-only, genuinely consumed (junction warming, bead placement).
- D-3 (WS3 halo): halo-tier puffs keep the `2 px` min-world clamp path
  but are sized/alpha'd to vanish first under stack pressure (overdraw
  relief by design, per NFR3).
- D-4 (WS4 budget): nominal beads land at 59958/60000 (two-pass scale
  active, in budget); beads ride the existing glow `PointList`.
- D-5 (white-smoke pass, 2026-09-20, same version): brighter sheaths +
  white cores, still render-only (counts/pipelines/budgets unchanged).
  CPU `smoke_puffs` base alpha `0.035+0.055d → 0.06+0.09d` + every
  third core-tier puff lerps toward `[1.0, 0.97, 0.92]` (halo stays
  blue; loop-index pick, replay/rebase-safe); `SMOKE_FRAG` adds a
  `(1-r2)³ × 0.65` luminance hot-center (arithmetic-only); exposures
  `DEMO 0.5→0.8 / MAP 0.7→1.0`. New test
  `smoke_white_core_subset_exists_and_stays_brighter` + `vec3(luma)`
  shader pin; 20/20 `cosmic_web` lib tests green.
- D-6 (close-up fix, 2026-09-20, same version): stretched quads read
  as hard blades when magnified. Shader-only: `SMOKE_FRAG` falloff
  fully dissolves on both axes (`ax²` tips, `radial²` core),
  renormalized `×1.8` (per-puff energy preserved, far-field grade
  unchanged), `8×8` dust is smoothed value noise (fract-only);
  `SMOKE_VERT` near fade is size-relative (`0.35×len → 1.25×len`).
  `viewer_shaders_compile` + `cosmic_shader_safety_pins` (new tip
  dissolve pin) green; headless counts unchanged.
- D-7 (density-contrast fix, 2026-09-20, same version): the white
  pass + an interim falloff x1.8 renormalization stacked ~4-5x light
  onto distant puffs (far pixels sample only the falloff peak, so
  peak renormalization brightens the whole far field — reverted).
  Anchored back on the target (brightness ~= mass density, dark
  voids): CPU faint floor now *below* the original grade (alpha
  `0.02+0.13d`, dimmed rgb floor, dense ceiling unchanged), white
  subset density-gated (`mix 0.15+0.55d`), frag peak back at 1.0 with
  hot-center `0.65 -> 0.45`, exposures `DEMO 0.65 / MAP 0.85`.
  Faint mist lands darker than the original look, dense threads ~2x
  brighter. Test pins the contrast on the toy web (dense mean alpha
  > 2.5x faint mean, faint mean < 0.05 void floor).
- Closing note (2026-09-20, v0.3.3 cut): D-1…D-7 are the ceiling of
  the render-only approach. Superseded by ADR-025 / `plans/v0.3.3/`
  (field-based render); the smoke, grain, bead, and braid/spine code
  paths retire there feature by feature.

## Acceptance criteria

ARCHITECT invariant rows (must hold at `done`, verified by tests where
marked [T]):

- A-1. Stage-0 descriptor, `web_hash` vectors, and node content IDs
  bit-identical before/after; `UNIVERSE_VERSION` unchanged [T:
  existing determinism + hash tests].
- A-2. Enrichment deterministic per `(seed, web, origin)`; hashed paths
  keep integer decisions + `sqrt`-only; per-pixel shaders use
  `fract`-hash only [T: replay tests].
- A-3. Projection stays un-flipped `directx::perspective`; `FrontFace/CCW`
  rules unchanged [T: existing projection pins].
- A-4. Marker only via `world_to_pixels`; picking only via
  `project_to_screen` + `ndc = (2u−1, 1−2v)`; selection stays node-only
  (beads/spines pick-ignored) [T: existing marker/pick pins].
- A-5. Bloom write-once preserved (every HDR target written exactly once,
  then only read) [T: HdrChain untouched].
- A-6. No `vulkano`/pipeline code outside `debug`/`tools`; `game` crate
  diff = none; save envelope unchanged.
- A-7. Draw-call count unchanged (no new pipelines); Low tris `<500k`.

UX acceptance rows (observable player-side behavior):

- UX-1. Boot lands on GAME DEMO: bright hub in view, threads leading to
  it readable within 3 s; steering toward the hub works as before.
- UX-2. Zoomed-out inspector shows the whole web with dark voids — no
  white-slab saturation; threads stay visible (never vanish), halos may
  fade.
- UX-3. Flying inside a filament: thin threads + gold beads pass by;
  no fullscreen white flash (near-eye fade holds); marker + pill +
  console behave as v0.3.2.
- UX-4. Clicking threads/beads is a no-op for selection (no error
  surface needed); clicking a node still shows mass/`r_vir`/link readout.

## Risks & Next steps

- R-1 (overdraw, WS2–3): more strands + stretched quads raise stacked-px
  cost in the inspector — mitigated by WS0 baseline, `SUBDIV↓`, cull,
  and the recorded cut order (beads → halo → strands). TECHLEAD cut
  option: cap strands at `3`/segment on Low via existing tier consts.
- R-2 (shader cost, WS2): ribbon `braid_point` trig/strand was `~24M`
  calls/frame at full draw before smoke — mitigated by fewer subdivisions
  and indexed draw if profiling implicates it (`reports` §8b precedent).
- R-3 (gold blowout, WS4): emissive beads + bloom intensity `2.2` demo
  may oversaturate hubs — mitigated by emissive clamp + ACES + per-surface
  exposure (grade consts only, no engine change).
- R-4 (merge coupling): `cosmic-web-mass-rank-fix` (in-progress) owns
  `descriptor.rs` mass-rank behavior — this feature never touches
  `descriptor.rs`/`classify.rs` masses; if the fix merges first, re-run
  WS0 baseline (numbers, not code, may shift).
- Next steps after `done`: sheet-sprite render + density export (batched
  descriptor change per milestones roadmap), discrete-GPU volumetric
  follow-up, LOD/culling hardening from WS0 data.
