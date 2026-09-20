# Plan — bloom-mip-chain

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: two crates. `engine::render::post` gains the new GLSL
strings, `BloomParams` fields, and CPU mirrors (engine stays free of
GPU-API code — the existing `post.rs` pattern). `debug` rebuilds
`HdrChain` as a level array and records the chain **from a pass
description list** so the Intel write-once rule becomes an executable
pin (FR4). **Invariant rows:** bloom targets written exactly once and
never read by their writer (pinned); resolve UV contract
(`v_uv = vec2(pos.x, 1.0 − pos.y)`, NDC top row) unchanged; `game`
untouched; `tools --hdr` `ResolvePass` untouched.

### Phase 1 — Shaders + params + CPU mirrors (`post.rs`)

`BLOOM_PREFILTER_FRAG` (soft knee + 13-tap), `BLOOM_DOWN_FRAG`,
`BLOOM_UP_FRAG` (tent × weight + down[k]); `BloomParams { knee, levels,
level_weights }`, `for_tier`; mirrors `jimenez13_offsets`,
`tent9_weights`, `soft_knee`; tests pin literals, clamp, naga compile.
Remove `BLOOM_BLUR_FRAG` + `gaussian9_weights` (or keep the latter if
another consumer exists — grep).

### Phase 2 — Pass description + structural pin (`main.rs`)

`BloomPassDesc { writes: ImageId, reads: Vec<ImageId>, kind }`;
`describe_bloom_chain(levels) -> Vec<BloomPassDesc>` (pure);
`record_bloom_chain(cmd, &descs, &chain)` executes it. Test: every
image written once, never read by its writer, resolve reads `up[0]`
only, `GAME_DEBUG_COSMIC_BLOOM=0` description reads `down[0]`.

### Phase 3 — `HdrChain` rebuild (`main.rs`)

`down: Vec<Target>`, `up: Vec<Target>` (image + view + framebuffer +
set), extents `½^k`; `build_hdr_chain(levels)`; swapchain-recreate path;
`up[last]` alias-free pass-through. Delete `bloom_a_fb`…`bloom_e_fb` and
`blur_*_set`.

### Phase 4 — Grade + Intel check + shots + gates

Lower `COSMIC_*_BLOOM_INTENSITY`; halo radial profile on `slab`
capture; UHD 620 captures; docs; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| BMC-001 | pending | `post.rs`: `BLOOM_PREFILTER_FRAG` / `BLOOM_DOWN_FRAG` / `BLOOM_UP_FRAG`; CPU mirrors + literal pins; naga compile tests | FR1, NFR3 |
| BMC-002 | pending | `BloomParams { knee, levels, level_weights }` + `spec_defaults` + `for_tier` (3/4/5) + clamp tests; remove `BLOOM_BLUR_FRAG` (+ `gaussian9_weights` if unused) | Goals §2–3, §6, FR2 |
| BMC-003 | pending | `describe_bloom_chain(levels)` pure description + **write-once structural pin** test (incl. `BLOOM=0` variant) | FR3, FR4, FR5, NFR2 |
| BMC-004 | pending | `HdrChain` as `down[]`/`up[]` targets; `record_bloom_chain` executes the description; swapchain-recreate rebuild; delete A–E targets | Goals §1, §6, FR3, NFR5 |
| BMC-005 | pending | Resolve reads `up[0]` (or `down[0]` when bloom off); LDR bypass untouched; capture determinism gate passes | FR5, FR7, NFR4 |
| BMC-006 | pending | Grade: `COSMIC_DEMO/MAP_BLOOM_INTENSITY` lowered per FR6 from `slab`/`demo` shots; record values | Goals §4, FR6 |
| BMC-007 | pending | Halo radial profile on `slab-after.png` (≥ 60 px half-max, core ≤ 8 px at 90 %); faint-thread check | DoD 1 |
| BMC-008 | pending | Intel UHD 620 captures `slab` + `demo` (no coloured squares); record shot paths | DoD 2, NFR2 |
| BMC-009 | pending | Budget numbers: passes + images + pixel count per tier vs today; memory at 1080p High | NFR1 |
| BMC-010 | pending | Docs: `rendering.md` bloom paragraph, `quality.md` row + formula, Intel report "pinned by test" note, techstack version bump; link check | DoD 6 |
| BMC-011 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — engine keeps GLSL +
mirrors only; debug owns Vulkan; the write-once invariant becomes an
executable pin before the chain is rebuilt) · Todos approved by:
TECHLEAD (2026-09-20 — risk-first: the pass description + pin
(BMC-003) lands before any image is allocated; tier levels 3/4/5 give
Low a recorded cut; Intel check is a DoD row, not a hope) · UX
acceptance rows: n-a · DoD verified by: ANALYST _(pending)_ · Security
reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Halo ≥ 60 px half-max, core tight, no thread slabs | pending | radial profile numbers + shots | ANALYST _(pending)_ |
| 2 | UHD 620 clean; structural pin green | pending | shot paths + test name | ANALYST _(pending)_ |
| 3 | Mirrors pin literals; `for_tier`; clamp | pending | test names | ANALYST _(pending)_ |
| 4 | Intensities lowered; `BLOOM=0` works; LDR untouched | pending | constants diff + run log | ANALYST _(pending)_ |
| 5 | Old chain removed | pending | `rg bloom_a_fb\|BLOOM_BLUR_FRAG` = 0 | ANALYST _(pending)_ |
| 6 | Docs + links | pending | file list | ANALYST _(pending)_ |
| 7 | Gates + audit + review + one commit | pending | gate log, commit | ANALYST + SECURITY _(pending)_ |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Every bloom image written exactly once per frame and never
  sampled by the pass that writes it [T: structural pin on
  `describe_bloom_chain`].
- A-2. Resolve UV contract (`RESOLVE_VERT`, NDC top row) unchanged [T:
  existing post tests].
- A-3. `engine::render::post` contains no `vulkano` types [T: grep in
  test].
- A-4. `game` and `tools` diff empty (tools `ResolvePass` untouched).
- A-5. `BloomParams::new` rejects NaN/negative/out-of-range `levels` [T].
- A-6. LDR bypass path byte-identical (no HDR format → same draws).

## Risks & Next steps

- R-1 (bandwidth on Low): 6 passes at ≤ ½ res ≈ today's 5; if a Low
  device profile shows > 1 ms extra, drop Low to 2 levels (recorded
  cut), never skip the pin.
- R-2 (alias-free `up[last]`): the smallest up level equals the
  smallest down level; a pass-through pass costs one tiny draw and
  keeps the "one writer per image" rule simple — do not alias.
- R-3 (flicker on 1.5 px splats): if captures flicker frame-to-frame
  (determinism gate would catch it as non-identical PNGs), add Karis
  average to the prefilter (constant-cost).
- Next: `cosmic-gas-veil-v2` composites its raymarch target at the
  same resolve (reads one more image — description list gains a row,
  pin covers it).
