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
| CGT-008 | pending | Grade `h0` per `k` (tier constants); shots ×4 presets High + Low; ridge-continuity scan (≥ 5 filaments, gap ≤ 2 px) recorded | DoD 1, DoD 2 |
| CGT-009 | pending | Costs: High `k=8` desktop, Medium `k=2` UHD 620, Low `k=1` UHD 620; apply cut order if over budget; record numbers | NFR1, DoD 5 |
| CGT-010 | pending | Retire `splat_records`, `SplatVertex`, `SplatRecord`, `WebFieldBudget`, tracer export loop, `refine()`, rebase splat closure + tests; `rg` pin = 0 | FR6, Goals §5, DoD 3 |
| CGT-011 | pending | Docs: `rendering.md` splat section rewrite + vertex-fetch note, `quality.md` rows (draw counts, device requirement, memory), `docs/game/universe.md`, `architecture.md`, techstack bump; link check | DoD 6 |
| CGT-012 | pending | Full gate suite + mobile guards (format + vertex-fetch feature bits) + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

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
below · DoD verified by: ANALYST _(pending)_ · Security reviewed by:
SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | ≥ 5 continuous beaded filaments at `slab` (ridge scan) | pending | | |
| 2 | Low/High parity overlay | pending | | |
| 3 | Retirement pin = 0; `WebField` shape; equality pin | partial | `WebField` gains `displacement` (tracers kept until CGT-010); equality pin `with_field_matches_plain_entry_point` green 2026-09-23 | DEV |
| 4 | FR7 tests + cell-list band + env parse | partial | CPU + GPU-path pins green 2026-09-23: `displacement_quantagrees_within_one_quantum_on_small_box`, `cell_list_sorted_inside_and_deterministic_on_small_box`, `cell_list_nominal_count_band` (seed 1234: 1 072 958 cells ∈ [900k, 1.3M]; sidecar 18 971 896 B ≈ 18.1 MB ≤ 24 MB), `displacement_image_bytes_round_trip`, `splat_k_tier_mapping`, `splat_k_parse_strict`, `splat_sub_offsets_shape`, `viewer_shaders_compile` (incl. `SPLAT_PROC_VERT`), `splat_proc_shader_pins`, `splat_proc_resources_seed_only`, `cosmic_window_snippet_shared`, `cosmic_density_ramp_shared`. Headless `cosmic_proc=cells1072958 kL1 kM2 kH8 vertsH8583664 ok` (seed 1337). GPU readback of eight sampled positions (A-3) still pending — needs a GPU capture session. | DEV |
| 5 | Costs recorded per tier; cut order applied if needed | pending | | |
| 6 | Docs + links | pending | | |
| 7 | Gates + audit + review + one commit | pending | | |

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
