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
| CGT-001 | pending | `WebField.displacement: Vec<[i16;3]>` (SNORM ± 8 cells) from the stage-B `∇Ψ` stencil; `sphere_radius_mpc` kept; doc-test updated; entry-point equality pin green | FR1, Goals §1 |
| CGT-002 | pending | `displace_sample(field, q)` CPU mirror (quantize → trilinear → wrap); test vs the old tracer loop on the 32³ doc-test box (≤ 1 quantum) | FR7, Goals §6 |
| CGT-003 | pending | `cell_list(field) -> Vec<u32>` (inside `R + cell`, sorted); tests: determinism, count band nominal seed, all listed cells' displaced centres inside `R + cell` | FR4, Goals §3 |
| CGT-004 | pending | Displacement 3D image `R16G16B16A16_SNORM` + linear clamp sampler + cell-list storage buffer on the seed path (never on rebase); R8 density volume bound to the vertex stage | FR2 |
| CGT-005 | pending | `SPLAT_VERT` v2: `gl_VertexIndex → (slot, sub)`; offset table per `k`; jitter ≤ ¼ sub-cell (`fract`-hash); displacement fetch; sphere + slab tests; push origin; density fetch; shared ramp; pipeline without vertex input; draw by `cells × k` | FR3, Goals §2 |
| CGT-006 | pending | `SplatK::for_tier` (1/2/8) + `GAME_DEBUG_COSMIC_K` (strict `1..=8`) + `cosmic_layout=` `cells/k/verts`; parse + mapping tests | FR5 |
| CGT-007 | pending | Shader-source pins: offset table literal, SNORM unpack constant, sphere-test literal, no fragment change (`cosmic_shader_safety_pins` extended) | FR7, NFR4 |
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
| 3 | Retirement pin = 0; `WebField` shape; equality pin | pending | | |
| 4 | FR7 tests + cell-list band + env parse | pending | | |
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
