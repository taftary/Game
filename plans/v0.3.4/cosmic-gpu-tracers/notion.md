# Notion — cosmic-gpu-tracers

## Status

`planned` (PO sign-off 2026-09-23; UX consulted; ARCHITECT consulted —
sidecar shape change + new render primitive, ADR-026 §2)

## Context

The v0.3.3 field render draws **one tracer per 4 Mpc lattice cell**
(≈ 1.1M inside the sphere). A filament in this web is 1–2 cells wide,
so at any framing the tracers along it read as a dotted blob, not a
thread — compare
[`../../v0.3.3/cosmic-vista-intro/shots/vista-after.png`](../../v0.3.3/cosmic-vista-intro/shots/vista-after.png)
with the reference. The engine already holds the cure — the
displacement field `D₊·∇Ψ` on the lattice — and `WebField::refine()`
(`field_export.rs`) can interpolate it trilinearly to emit 8
sub-tracers per cell. It was never wired: 8.4M tracers × 16 B ≈ 134 MB
of vertex data plus a ≈ 1 s CPU pass per seed (and per rebase, under
the v0.3.3 rebuild path) is unacceptable on every tier. PO question of
2026-09-23: *"is there another solution to not have this calculation
anymore?"* — yes: do not store tracers at all.

**Procedural GPU tracers.** Export the displacement grid itself
(128³ × 3 components) as the sidecar and upload it as a 3D texture.
The splat vertex shader maps `gl_VertexIndex` → Lagrangian cell +
sub-sample offset → trilinear displacement fetch → Eulerian position,
then samples the R8 density volume the veil already uploads for
colour and kernel radius. The sub-sample count per cell is a **draw
parameter per tier**, not a precompute. The origin goes through a push
constant, so tracers never need a rebase rebuild (the splat part of
`cosmic-rebase-async`'s job is deleted here).

## Problem & Needs

- **Player / reference match:** filaments must read as continuous
  threads with beads, not as sparse dots; sub-cell tracer density is
  the only way to populate the Zel'dovich caustics densely enough.
- **No calculation to wait for:** no per-seed multi-second refinement,
  no per-rebase tracer rebuild, no 100+ MB vertex buffers.
- **One code path across tiers:** Low must be the same web with fewer
  sub-samples (ADR-025 §4), which a draw count gives for free.

## Goals

1. **Sidecar shape.** `WebField.displacement: Vec<[i16; 3]>` —
   growth-scaled displacement in cell units, quantized SNORM over
   ± 8 cells (quantum ≈ 1 kpc); `WebField.tracers` retires. The
   descriptor path stays byte-identical (pinned).
2. **Procedural splat draw.** One 3D texture (`R16G16B16A16_SNORM`,
   128³, ≈ 16.8 MB) + the existing R8 density volume; the splat
   vertex shader reconstructs `x = q − D₊∇Ψ(q)` per invocation with
   a deterministic sub-cell offset pattern (fixed lattice offsets,
   plus a `fract`-hash jitter of ≤ ¼ sub-cell); density from the R8
   volume at `x`; kernel radius and colour as today's adaptive kernel
   but with tier constants `h0` re-tuned for the sub-sample count.
3. **Cell list, once per seed.** A compact `u32` list of Lagrangian
   cells whose displaced centre lands inside the sphere (+ 1 cell
   margin) is built on the CPU once per seed (≈ 2.1M trilinear
   evaluations, ≤ 50 ms, on the reseed / load path) and drives the
   draw (`vertex_count = cells × k`). No per-travel CPU work.
4. **Tier draw counts.** `k` = sub-samples per cell: Low 1 / Medium
   2 / High 8 (starting values; measured on the reference iGPU and a
   desktop GPU; cut order recorded). Low invocation count equals
   today's tracer count so Low cost does not rise.
5. **Retirement.** `splat_records`, `SplatVertex`, `SplatRecord`,
   the 16 B packed stream, the splat closure in the rebase job, and
   the `field_export.rs` tracer loop + `refine()` retire in this
   feature (each a todo).
6. **Pin tool.** `displace_sample(field, q) -> [f32; 3]` on the CPU
   mirrors the shader mapping exactly (same quantization, same
   trilinear weights) so tests can assert positions inside the
   sphere, agreement with the retired tracer positions on the parent
   lattice (≤ 1 quantum), and that `k = 8` sub-samples of one cell
   land within the cell's displaced neighbourhood.

## Non-goals

- No anisotropic / quad splats (stretching along the collapse axis)
  — recorded as the next fidelity step after this version.
- No change to the lattice size, `growth_factor`, smoothing, or any
  hashed parameter; the export is non-hashed (ADR-025 §1 nature kept).
- No compute shaders, no indirect draws (a plain `draw(vertex_count)`
  over the cell list is enough; indirect culling is LOD/culling
  hardening, deferred).
- No change to the veil (sprites / march) beyond sharing the texture
  sampler.

## Users / Stakeholders

- Player: filaments read as threads at every framing; Low and High
  show the same web.
- Developer: `cosmic_layout=` prints `cells=N k=K verts=N·K`; the
  `GAME_DEBUG_COSMIC_K=<1..=8>` override lets grading rounds sweep
  `k` live.
- UX consulted: yes — UX-1 (thread continuity at the slab framing),
  UX-2 (Low/High parity).

## Roles

Author: PO. UX consulted (required if player-facing): yes.
ARCHITECT consulted (required if cross-module): yes — `WebField`
shape change in `engine::universe::web::field_export` (non-hashed,
additive-then-remove), new vertex-stage texture fetch recorded as a
device requirement, `cosmic-rebase-async` job shrink.

## Functional requirements

- FR1 (export): `export_field` writes `displacement` (i16 × 3 per
  lattice cell, SNORM ± 8 cells) from the same `∇Ψ` stencil stage B
  uses; `tracers` removed from `WebField`; `WebFieldBudget` retires
  (budget is now a draw count); doc-test updated; equality pin
  between entry points green.
- FR2 (upload): one 3D image `R16G16B16A16_SNORM` 128³ (alpha unused)
  + linear sampler with clamp-to-edge is created on reseed / load,
  never on rebase; the R8 density volume is bound to the splat
  vertex stage as a second sampler.
- FR3 (shader): `SPLAT_VERT` v2 — inputs: none (no vertex buffer);
  `gl_VertexIndex` → `(cell_slot, sub)`; cell index from the cell
  list (`uint` storage buffer or `R32_UINT` buffer texture); `q =
  cell + offset[sub] + jitter`; `x = q − fetch(disp, q/n)`; sphere
  and slab tests; `pos_rel = x·cell_mpc − half − pc.origin`; density
  = `fetch(density, x/n)`; kernel and colour via the shared ramp /
  transfer snippet. Fragment shader unchanged (arithmetic-only).
- FR4 (cell list): `cell_list(field) -> Vec<u32>` (pure, CPU,
  deterministic): Lagrangian cells whose displaced centre (via
  `displace_sample`) lies within `R + cell`; sorted ascending.
- FR5 (tiers): `SplatK::for_tier` → 1 / 2 / 8; env override
  `GAME_DEBUG_COSMIC_K`; `cosmic_layout=` prints `cells`, `k`, `verts`.
- FR6 (retirement): `rg splat_records|SplatVertex|SplatRecord|WebFieldBudget|fn refine` = 0
  in `crates/`; rebase job carries glow only.
- FR7 (pins): `displace_sample` vs a CPU reference of the old tracer
  loop on the small doc-test box (≤ 1 quantum); all sampled positions
  inside `R + cell`; shader-source pins for the sub-offset table and
  the SNORM unpack constant; capture determinism byte-identical per
  `(build, seed, GPU)`.

## Non-functional requirements

- NFR1 (cost): High `k = 8` on a desktop GPU ≤ 4 ms for the splat
  pass at 1080p; Medium `k = 2` on the UHD 620 ≤ 3 ms; Low `k = 1`
  ≤ today's splat cost. Cut order: `k` 8 → 4 on High, 2 → 1 on
  Medium; then kernel px clamp 64 → 32.
- NFR2 (memory): sidecar ≈ 16.8 MB (grid) + ≈ 4.4 MB (cell list);
  GPU: one 16.8 MB image; no tracer vertex buffers (−34 MB vs v0.3.3
  High).
- NFR3 (boot): export cost drops (no second displacement loop); cell
  list ≤ 50 ms nominal on the load path.
- NFR4 (invariants): fragment stays `fract`-hash / arithmetic only;
  vertex-stage texture fetch recorded as a device requirement in
  `quality.md`; projection un-flipped; bloom write-once untouched
  (read-only textures).
- NFR5 (determinism): `cell_list` and `displace_sample` are pure
  functions of `WebField`; captures byte-identical per build + seed
  on one GPU.

## Definition of Done

1. Shots `slab-after.png`, `inspector-after.png`, `demo-after.png`,
   `vista-after.png` at High: ANALYST traces ≥ 5 filaments across the
   slab framing as continuous beaded threads (gap ≤ 2 px along the
   ridge) — quantified by a ridge-continuity scan recorded in the plan.
2. Low (`k = 1`) / High (`k = 8`) parity overlay at `slab`: same
   filaments, Low grainier (UX-2).
3. `rg` retirement pin = 0; `WebField` has `displacement`, no
   `tracers`; descriptor equality pin green.
4. Tests listed in FR7 green; `cell_list` count band on the nominal
   seed recorded; `GAME_DEBUG_COSMIC_K` parse test.
5. Costs recorded per tier on the reference UHD 620 and a desktop
   GPU; cut order applied if needed and recorded.
6. Docs: `rendering.md` cosmic splat section rewritten (procedural
   tracers, vertex fetch note), `quality.md` rows (draw counts per
   tier, device requirement, memory), `docs/game/universe.md` sidecar
   sentence, `architecture.md` field-render note, techstack bump;
   links resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Third feature of the version; depends on `cosmic-sphere-clip`
  (bounded descriptor for shots) and `cosmic-rebase-async` (the job
  it shrinks).
- Assumes vertex-stage sampled-image reads are available on every
  target (Vulkan core; mobile guards check the feature bit).
- `k = 8` sub-offsets are the `refine()` pattern (`0.25 + 0.5·s`)
  generalized: `k = 1` → cell centre, `k = 2` → two offsets along the
  local largest displacement axis is *not* done (keep it simple: a
  fixed table per `k`).

## Open questions

- Should the R8 density fetch use the **Eulerian** cell at `x` (as
  today's tracer `overdensity`) or a 3³-smoothed value baked into the
  volume? Today's volume is already smoothed for the veil; start with
  it, grade from shots, record.
- `R16G16B16A16_SNORM` vs three `R16_SNORM` images: single RGBA
  image unless a target lacks the format (mobile guard decides;
  fallback recorded).
