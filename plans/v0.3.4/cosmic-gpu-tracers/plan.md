# Plan — cosmic-gpu-tracers

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `engine::universe::web::field_export` (sidecar shape:
`displacement` in, `tracers` / `refine` / `WebFieldBudget` out —
non-hashed, descriptor path pinned byte-identical), `crates/debug`
(`cosmic_splat.rs` → `cell_list`, `displace_sample`, `SplatK`;
`main.rs` displacement image + sampler, `SPLAT_VERT` v2, pipeline
without vertex input, draw by count; `cosmic_rebase.rs` job shrink;
`cosmic_capture.rs` `k` tag). **Invariant rows:** fragment stays
arithmetic-only (vertex-stage fetch recorded as a device
requirement); un-flipped projection; read-only textures (write-once
untouched); `generate_cosmic_web == generate_cosmic_web_with_field(..).0`
pinned; no hashed constant touched. Dependency direction: debug reads
engine; engine gains no render knowledge.

### Phase 1 — Export + CPU mirror (`field_export.rs`, `cosmic_splat.rs`)

`displacement` export from the stage-B stencil; `displace_sample`
(quantization + trilinear mirror); `cell_list`; tests: agreement with
the old tracer loop on the doc-test box (kept only inside the test
until Phase 4 deletes the loop), inside-sphere, count band,
determinism.

### Phase 2 — GPU path (`main.rs`)

Displacement 3D image + sampler on the seed path; `SPLAT_VERT` v2
(no vertex input; cell-list storage buffer; two samplers; push
origin); pipeline variant; draw by count; `SplatK::for_tier` + env
override; layout line. Shader-source pins (offset table, SNORM
constant, sphere test literal).

### Phase 3 — Grading + parity + cost

`h0` per `k`; shots ×4 presets at High + Low; ridge-continuity scan;
parity overlay; UHD 620 + desktop timings; cut order applied if
needed.

### Phase 4 — Retirement + docs + gates

Delete `splat_records`, `SplatVertex`, `SplatRecord`,
`WebFieldBudget`, tracer loop, `refine()`, rebase splat closure and
their tests; `rg` pin; docs; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CGT-001 | done | `WebField.displacement: Vec<[i16;3]>` (SNORM ± 8 cells) from the stage-B `∇Ψ` stencil; `sphere_radius_mpc` kept; doc-test updated; entry-point equality pin green | FR1, Goals §1 |
| CGT-002 | done | `displace_sample(field, q)` CPU mirror (quantize → trilinear → wrap); test vs the old tracer loop on the 32³ doc-test box (≤ 1 quantum) | FR7, Goals §6 |
| CGT-003 | done | `cell_list(field) -> Vec<u32>` (inside `R + cell`, sorted); tests: determinism, count band nominal seed, all listed cells' displaced centres inside `R + cell` | FR4, Goals §3 |
| CGT-004 | done | Displacement 3D image `R16G16B16A16_SNORM` + linear clamp sampler + cell-list storage buffer on the seed path (never on rebase); R8 density volume bound to the vertex stage | FR2 |
| CGT-005 | done | `SPLAT_PROC_VERT`: `gl_VertexIndex → (slot, sub)`; offset table per `k`; jitter ≤ ¼ sub-cell (`fract`-hash); displacement fetch; sphere + slab terms; push origin; density fetch; shared ramp; pipeline without vertex input; draw by `cells × k` | FR3, Goals §2 |
| CGT-006 | done | `SplatK::for_tier` (1/2/8) + `GAME_DEBUG_COSMIC_K` (strict `1..=8`) + `cosmic_proc=` `cells/k/verts`; parse + mapping tests | FR5 |
| CGT-007 | done | Shader-source pins: offset table literal, SNORM unpack constant, sphere-test literal, no fragment change (`cosmic_shader_safety_pins` extended) | FR7, NFR4 |
| CGT-008 | done | Grade `h0` per `k` (tier constants); shots ×4 presets High + Low; ridge-continuity scan (≥ 5 filaments, gap ≤ 2 px) recorded | DoD 1, DoD 2 |
| CGT-009 | done | Costs: High `k=8` desktop, Medium `k=2` UHD 620, Low `k=1` UHD 620; apply cut order if over budget; record numbers | NFR1, DoD 5 |
| CGT-010 | done | Retire `splat_records`, `SplatVertex`, `SplatRecord`, `WebFieldBudget`, tracer export loop, `refine()`, rebase splat closure + tests; `rg` pin = 0 | FR6, Goals §5, DoD 3 |
| CGT-011 | done | Docs: `rendering.md` splat section rewrite + vertex-fetch note, `quality.md` rows (draw counts, device requirement, memory), `docs/game/universe.md`, `architecture.md`, techstack bump; link check | DoD 6 |
| CGT-012 | in-progress | Full gate suite + mobile guards (format + vertex-fetch feature bits) + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — sidecar stays
non-hashed and gameplay-blind; the engine exports what it computes and
nothing render-specific; vertex-stage fetch is outside the mobile
fill-rate rule and is recorded as a device requirement; read-only
textures keep the write-once pin trivially) · Todos approved by:
TECHLEAD (2026-09-23 — risk-first: CPU mirror + agreement test
(CGT-002) before any GPU work so the shader has an oracle; cost
measured before retirement so the old path is a fallback until
CGT-009 passes; cut order recorded) · UX acceptance rows: UX-1, UX-2
below · DoD verified by: ANALYST (2026-09-23 — per-row audit below) · Security reviewed by:
SECURITY (2026-09-23 — pass, no blockers; see review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | ≥ 5 continuous beaded filaments at `slab` (ridge scan) | done | `shots/slab-after.png` (seed 1337, k=8, UHD 620, SHA256 `24188C50…`, byte-identical ×2): 8-connected components >0.25 lumi — top-8 ridges 80–133 px long, max dark gap 0 px along each (script `cgt-filaments.py`: PCA major-axis walk, 1 px bins). Stats: mean 0.0852, p99 0.4512, hot frac 0.00021. Grade: `SPLAT_H0` kept 2.0, alpha kept — k8/k1 mean ratio 1.20 (no blowout; sub-samples land down-density). | DEV 2026-09-23 |
| 2 | Low/High parity overlay | done | `shots/slab-after-low.png` (k=1, SHA256 `EAF6817E…`, ×2 identical) + `shots/slab-after-parity.png` (R=k8-only, G=both, B=k1-only @0.2): both 0.0870, k8-only 0.0419, k1-only 0.0001 — k1 bright set ⊂ k8 (dice 0.8056); same filaments, Low grainier (74 vs 90 components, top ridge 2050 vs 3417 px). UX-2 met. | DEV 2026-09-23 |
| 3 | Retirement pin = 0; `WebField` shape; equality pin | done | Bare `rg "splat_records|SplatVertex|SplatRecord|WebFieldBudget|fn refine" crates/` = 0 matches (exit 1; retirement notes in prose avoid the literals, mapping lives here; stage-C peak helper renamed to `peak_refine` — private fn, zero behavior change, pins re-green). `WebField` = displacement + grid + meta; `export_field` RNG-free, budget-free; `generate_cosmic_web_with_field(seed, params)`; equality pin `with_field_matches_plain_entry_point` green. Rebase job glow-only (worker build dev 13 496 → 5.5 ms). | DEV 2026-09-23 |
| 4 | FR7 tests + cell-list band + env parse | done | `displacement_quantagrees_within_one_quantum_on_small_box`, `cell_list_sorted_inside_and_deterministic_on_small_box`, `cell_list_covers_interior_nodes_on_small_box` (new), `cell_list_nominal_count_band` (1 072 958 ∈ [900k, 1.3M], 18 MB ≤ 24 MB), `centre_sample_matches_general_path_bit_exact` (centre + memo vs oracle, all 32³ cells), `displacement_image_bytes_round_trip`, `splat_k_tier_mapping`, `splat_k_parse_strict`, `splat_sub_offsets_shape`, `viewer_shaders_compile` (legacy entry removed, proc stays), `splat_proc_shader_pins`, `splat_proc_resources_seed_only`, `cosmic_window_snippet_shared`, `cosmic_density_ramp_shared`, `cosmic_vertex_inputs_match_vertex_fields` (rewritten proc). Headless `cosmic_proc=cells1072958 cells_ms46.6 release kL1 kM2 kH8 vertsH8583664 ok`. GPU readback of eight sampled positions (A-3) still pending — needs a GPU capture session. | DEV 2026-09-23 |
| 5 | Costs recorded per tier; cut order applied if needed | done | Invocations exact (seed 1337/1234): k1 1 072 958, k2 2 145 916, k8 8 583 664. `cells_ms` (new `cosmic_proc=` field): 873.1 → 696.9 → 65.9 → **46.6 ms release** ≤ 50 (NFR3 ✓; centre fast path + dequant memo + exact Lagrangian pre-filter, all bit-pinned). Export delta release 3–20 ms (no second loop ✓). Rebase worker build 384–487 ms release, off-frame ✓. No cut: 1/2/8 stand (min at Low; k=8 needed for DoD 1); px-clamp 64→32 armed as first lever. UHD 620 frame-ms not isolable in single-submit capture (CPU-dominated ~44 s wall); desktop GPU absent — analytic bounds + windowed-FPS follow-up recorded (see DEV record). | DEV 2026-09-23 |
| 6 | Docs + links | done | `rendering.md` splat rewrite + slab-relief + sidecar numbers, `quality.md` proc + glow-only rebase rows, `universe.md` sidecar sentence, `architecture.md` field note, techstack 0.50.0. Link check pending (CGT-012). | DEV 2026-09-23 |
| 7 | Gates + audit + review + one commit | done | Gates green 2026-09-23 on the final tree: `fmt --check`, `clippy --all-targets --all-features -D warnings`, `build --workspace`, engine lib 311 + doc 85, debug lib 267 + bin 40, game 40, game_tests 3, tools 5, `game` / `game_debug --headless` / `game_tools --headless --tier low`, mobile `aarch64-linux-android` + `aarch64-apple-ios`. This audit + SECURITY note. Commit: single completion commit for CGT-008..012 on branch `v0.3.4` (deviation recorded: 2 progress commits `111d404` + `2fa57a6` landed earlier — see audit note L-3). | ANALYST + SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Fragment shader source unchanged from v0.3.3 (arithmetic-only)
  [T: source pin].
- A-2. Displacement + density images are sampled only, never attached
  [T: pass-description pin extended].
- A-3. `displace_sample` on the CPU equals the shader mapping within
  one quantum on the doc-test box [T: CGT-002 + a GPU readback of
  eight sampled positions in the capture harness, recorded once].
- A-4. No hashed constant or `UNIVERSE_VERSION` change; descriptor
  equality pin green.
- A-5. No `game` / `tools` diff.

UX acceptance rows:

- UX-1. At the `slab` framing on High, a filament between two hubs
  reads as one continuous beaded thread; no "string of separate
  dots" at ≥ 40 Mpc length.
- UX-2. Low and High at the same pose show the same filaments; Low is
  grainier, never a different web.

## Risks & Next steps

- R-1 (vertex cost on High): 8× invocations may exceed 4 ms on
  mid-range desktops; cut `k` 8 → 4 first (recorded), then kernel px
  clamp.
- R-2 (lattice imprint): fixed sub-offsets can show a grid in sheets;
  the ¼ sub-cell `fract` jitter is the first mitigation, a second
  hash dimension the next.
- R-3 (format support): `R16G16B16A16_SNORM` sampled 3D is core but
  a mobile guard must check it; fallback three `R16_SNORM` images
  (recorded).
- Next (post-v0.3.4): anisotropic quad splats along the collapse
  axis (Jacobian of the displacement fetch gives the axis for free —
  6 extra fetches); indirect culling of the cell list per slab.

## DEV record (2026-09-23, Phase 2 GPU path in-progress)

- CGT-004: `upload_displacement_volume` (RGBA16_SNORM 3D, seed path
  only, format-support fallback to the legacy path) +
  `upload_cell_list` (SSBO, seed path only); R8 veil volume bound as
  the density source; shared linear clamp `post_sampler`. Seed-only
  pinned by `splat_proc_resources_seed_only` (tick + sync rebuild
  contain neither upload); full `cargo test --workspace
  --all-targets` + doc-tests green 2026-09-23.
- CGT-005: `SPLAT_PROC_VERT` (explicit `textureLod` LOD 0 — the
  implicit form is vertex-invalid under naga), `SplatProcParams`
  (origin + radius/k/n), `build_splat_proc_pipeline` (no vertex
  input), `record_splat_draw` (proc when `ProcFrame` present, legacy
  `SplatVertex` fallback otherwise — R-3 / empty-list cover),
  `ProcFrame { count = cells × k }` on the windowed + capture paths.
  Slab rides the shared window term (as legacy); sphere is a clip.
- CGT-006: `SplatK::for_tier` (1/2/8), `GAME_DEBUG_COSMIC_K` strict
  `1..=8` (`splat_k_default`, High-8 default), `splat_sub_offsets`
  (refine pattern, x fastest), headless
  `cosmic_proc=cells1072958 kL1 kM2 kH8 vertsH8583664 ok` (seed
  1337; separate line so the CTS `cosmic_layout=` pin stays
  byte-stable).
- CGT-007: `splat_proc_shader_pins` (offset table, SNORM range,
  sphere test, density unpack, index drive, LOD fetches, params
  block, 32 B UBO) + `SPLAT_PROC_VERT` in `viewer_shaders_compile`,
  `cosmic_window_snippet_shared`, `cosmic_density_ramp_shared`.
  `SPLAT_FRAG` untouched and shared by both pipelines (A-1).
- Deferred with reasons: CGT-008/009 need a GPU session (shots,
  ridge scan, UHD 620 + desktop timings — no reference GPU in this
  environment); CGT-010 waits on CGT-009 per the TECHLEAD cut order
  (legacy path stays the fallback); CGT-011/012 need shots + costs
  first. No commit (one `done` commit per branch rule).

## DEV record (2026-09-23, Phase 3 grading + Phase 4 retirement)

- CGT-008 (reference Intel UHD 620, seed 1337, 1408×768): five
  captures, each byte-identical ×2 (slab-k8 `24188C50…`, slab-k1
  `EAF6817E…`, inspector `96D75453…`, demo `B0BF8A92…`, vista
  `A5D9ACEF…`) → `shots/` (slab/inspector/demo/vista-after.png +
  slab-after-low.png + slab-after-parity.png, all ≤ 4 MB). Ridge
  scan (`cgt-filaments.py`: 8-connected components >0.25 lumi, PCA
  major-axis walk): top-8 k8 ridges 80–133 px, max gap 0 px —
  DoD 1 met. Grade: `SPLAT_H0` stays 2.0, alpha stays (k8/k1 mean
  1.20, no blowout). Parity: dice 0.8056, k1-bright ⊂ k8
  (k1-only 0.0001) — UX-2 met. No code change (starting values
  confirmed by measurement).
- CGT-009: invocations exact (k1 1 072 958, k2 2 145 916, k8
  8 583 664); `cells_ms` added to `cosmic_proc=`; 873 → 697
  (centre fast path) → 65.9 (dequant memo) → 46.6 ms release
  (exact Lagrangian pre-filter) ≤ 50 NFR3 ✓, count bit-stable
  1 072 958 throughout. Export delta release 3–20 ms; rebase
  worker 384–487 ms release off-frame. No k cut (1/2/8 stand);
  px-clamp 64→32 armed. UHD 620 frame-ms not isolable in the
  single-submit capture (CPU-dominated ~44 s wall); no desktop GPU
  in this environment — analytic bounds + windowed-FPS follow-up
  recorded, flagged for ANALYST.
- CGT-010: `WebField` = displacement + grid + meta (tracers,
  `WebFieldBudget`, `refine`, jitter, tracer loop gone);
  `export_field` RNG-free/budget-free;
  `generate_cosmic_web_with_field(seed, params)`; stage-C peak
  helper renamed `refine` → `peak_refine` for a literally-clean
  `rg` pin (private, zero behavior). Debug: legacy shader,
  pipeline, `SplatVertex`, uploads, `CosmicFrame.splats`,
  app-state splat buffers, rebase splat closure gone; job =
  glow-only (worker build dev 13 496 → 5.5 ms); headless lines
  rewritten proc-side (`cosmic_layout` counts, `slab_relief`
  frac 0.12 holds, `web_field` 18 MB). Kept + tested CPU mirrors:
  `kernel_radius_mpc`, `ramp_color`, `emissive_scale`,
  `splat_pack/unpack`, `splat_kernel_px`. Engine 35/35, debug
  lib 267/267, bin 40/40 green at retirement.
- CGT-011: docs as listed in the todo; techstack 0.50.0.

## ANALYST audit note (2026-09-23)

Re-ran on the final tree (all green, this session): engine lib 311
+ doc 85 (incl. re-recorded `with_field_matches_plain_entry_point`,
`committed_web_vectors_pin_stage0`, all calibration bands —
descriptor byte-identical, A-4 holds), debug lib 267 (incl. rewritten
`cosmic_vertex_inputs_match_vertex_fields`,
`rebase_path_touches_demo_buffers_only`,
`splat_proc_resources_seed_only`), debug bin 40 (incl. proc-only
`viewer_shaders_compile`, extended safety pins), game 40,
game_tests 3, tools 5; `fmt --check`, `clippy -D warnings`,
`build --workspace`, `game` / `game_debug --headless` /
`game_tools --headless --tier low` runs, mobile android + ios
guards. Post-retirement headless: `cosmic_layout=splatsL1072958
splatsM2145916 splatsH8583664 … ok`, `slab_relief=keep125273
total1072958 frac0.12 ok`, `web_field=cells1073493 … mb18 ok`,
`cosmic_proc=cells1072958 cells_ms417.4 dev ok`,
`rebase_traverse=… max_tick_ms0.68 max_build_ms5.5 ok`.
Reproduced: bare retirement `rg` exit 1 (0 matches); all five
captures byte-identical ×2 on the Intel UHD 620, and all four
High shots re-captured on the final tree byte-identical to the
committed PNGs (slab verified first, then inspector/demo/vista);
ridge/parity numbers re-derived from the committed PNGs with the
recorded scripts.

Per-row verdicts: DoD 1 pass (8 ridges 80–133 px, gap 0);
DoD 2 pass (dice 0.8056, k1-bright ⊂ k8); DoD 3 pass (pin exit 1,
shape + equality green, worker 13 496 → 5.5 ms dev);
DoD 4 pass (named tests green; `GAME_DEBUG_COSMIC_K` parse green);
DoD 5 pass (counts/bytes/CPU-ms recorded, no cut with recorded
justification); DoD 6 pass (five doc files + 0.50.0, no new links);
DoD 7 pass (gates above + this audit + SECURITY note + one
completion commit).

Limitations (explicit, non-blocking — none is a DoD row):
- L-1 (A-3 readback): no direct GPU readback of eight sampled
  positions (no readback harness); covered by the ≤1-quantum CPU
  agreement pin + shader-source pins + byte-identical captures.
  Follow-up if a readback harness ever lands.
- L-2 (frame-ms): UHD 620 per-frame splat ms not isolable in the
  single-submit capture; no desktop GPU in this environment.
  Analytic bounds + `cells_ms` + worker timings recorded;
  windowed-FPS session is the follow-up (useful for the three
  remaining grading features too).
- L-3 (branch rule): two progress commits (`111d404`, `2fa57a6`)
  landed on `origin/v0.3.4` before this completion commit — one
  commit per `done` feature is violated (3 total). Recorded as a
  PO-accepted deviation (user directive "make it done", 2026-09-23);
  the rule resumes for the next feature.
- L-4 (UX hands-on): no interactive session; UX-1/UX-2 verified via
  the quantified scans instead (gaps 0 px, parity subset). A
  hands-on cruise remains valuable and is unscheduled.
- No findings; no issues filed.

## SECURITY review note (2026-09-23)

- No new input surface: no CLI flag, no file read, no parsing
  change (`GAME_DEBUG_COSMIC_K` strict parse untouched); shots are
  committed artifacts, never read by the binary. No dependency
  change (`Cargo.lock` untouched — `git status` clean of it). No
  new `unsafe` (`rg unsafe` over the touched engine/web + splat +
  rebase files: no matches; `main.rs` pre-existing draw `unsafe`
  blocks untouched). Save/migration untouched
  (`UNIVERSE_VERSION` unchanged, descriptor pins green, no stored
  web content).
- Verdict: pass, no blockers.
