# Notion — bloom-mip-chain

## Status

`planned` (PO sign-off 2026-09-20; ARCHITECT + TECHLEAD breakdown in
`plan.md`)

## Context

The cosmic bloom (`HdrChain`, `crates/debug/src/main.rs`; GLSL +
`BloomParams` in `crates/engine/src/render/post.rs`) is a half-res,
two-scale separable Gaussian: bright → A, H σ=1.6 → B, V → C, H at 2×
step → D, V at 2× step → E, resolve(scene, E). Effective radius is
≈ 20 px at 1080p. Five targets, each written exactly once (Intel
UHD 620 rule, `docs/reports/2026-09-19-intel-hdr-bloom-corruption.md`).

The target's cluster cores (report §3 "soft bloom halo", §8 "a second,
larger bloom pass for cluster cores") carry halos of 50–80 px radius
with a long soft tail, plus a tight bright core — a **two-scale**
glow the current chain cannot produce: pushing intensity to 2.2 only
brightens the 20 px lobe (the v0.3.2 `COSMIC_DEMO_BLOOM_INTENSITY`
compensation) and blooms everything above threshold equally.

Industry standard for this look is a **mip pyramid**: progressive
downsample (½ … 1/32) with a 13-tap filter, then progressive upsample
with a 3×3 tent, summing levels — wide, cheap, stable. It has to be
done here **without ping-pong**: every level is its own image, written
once on the way down and once on the way up (two images per level, or
one down image + one up image per level), then only read.

## Problem & Needs

- **Wide halos** for Tier A hubs (`cosmic-hub-hierarchy`): 3–5× the
  current radius, with a smooth falloff instead of a Gaussian lobe.
- **Two-scale glow**: tight bright core + broad faint halo from one
  chain (level weighting), not two separate passes.
- **Stability**: no flicker on sub-pixel sprites (13-tap downsample is
  the standard fix for firefly flicker; the current 9-tap H/V on a
  half-res bright extract flickers on 1.5 px points).
- **Intel rule kept**: no image is both attachment and sampled input in
  one command buffer.
- **Mobile budget**: half-res base + 4 levels ≈ the same bandwidth as
  today's 5 half-res targets; Low may stop at 3 levels.

## Goals

1. **Mip chain**: bright extract at ½ res → down `¼`, `⅛`, `1/16`
   (`1/32` on High) with a 13-tap (Jimenez) filter → up from the
   smallest with a 3×3 tent, each up level = `tent(up_{k+1}) + down_k`
   into a **fresh** image → resolve reads the top up level. Every
   image written exactly once, then only read.
2. **Level weights**: `BloomParams` gains per-level weights (default
   `[1.0, 0.8, 0.6, 0.4, 0.3]`) applied at the up pass, so the core
   (fine levels) and the halo (coarse levels) are balanced by data, not
   by intensity.
3. **Tiering**: Low = 3 levels (½, ¼, ⅛), Medium = 4, High = 5;
   `quality.md` row updated.
4. **Intensity back to spec**: `COSMIC_*_BLOOM_INTENSITY` return toward
   `spec_defaults()` (0.85) as the chain now delivers the halo; the
   grade knobs stay per-surface constants.
5. **Threshold with knee**: soft-knee threshold (`knee = 0.5`) replaces
   the hard `max(hdr − t, 0)` so Tier B cores fade into bloom instead
   of popping.
6. **Retire** the 2-scale H/V blur (`BLOOM_BLUR_FRAG` 9-tap) and the
   five fixed targets A–E; `GAME_DEBUG_COSMIC_BLOOM=0` still resolves
   with the bright extract only.

## Non-goals

- No change to auto-exposure, ACES, or the resolve's tone curve.
- No lens dirt / anamorphic streaks / ghosting.
- No compute shaders (fragment passes on fullscreen triangles, the
  `RESOLVE_VERT` precedent); no mipmapped images with
  `vkCmdBlitImage` (blit chains are exactly the read/write aliasing
  the Intel rule forbids on one image).
- No change for the non-cosmic HDR path in `game_tools --hdr`
  (`ResolvePass` there is separate and stays).
- No debug-UI beyond the existing env switches.

## Users / Stakeholders

- **Player / developer:** hub halos that look like the reference;
  faint threads no longer bloom into slabs; no flicker on points.
- **Intel UHD 620 machine:** must stay artifact-free (the rule's
  reason to exist).
- UX consulted: n-a (rendering quality, no interaction). ARCHITECT
  consulted: yes — touches `engine::render::post` (shared GLSL +
  `BloomParams`) and the debug `HdrChain`; the write-once rule is a
  binding invariant.

## Roles

Author: PO. UX consulted (required if player-facing): n-a.
ARCHITECT consulted (required if cross-module): yes — `engine::render::post`
gains shaders + params (engine stays GPU-API-free: GLSL strings + CPU
mirrors only, the existing pattern); `debug` owns the Vulkan chain;
write-once invariant pinned structurally.

## Functional requirements

- FR1 (shaders, `post.rs`): `BLOOM_PREFILTER_FRAG` (soft-knee
  threshold + 13-tap downsample from full-res scene → ½), `BLOOM_DOWN_FRAG`
  (13-tap), `BLOOM_UP_FRAG` (3×3 tent of the coarser up level × weight
  + the same-level down image), `resolve_frag_bloom` unchanged in
  interface (samples scene + one bloom image). CPU mirrors:
  `jimenez13_offsets()`, `tent9_weights()`, `soft_knee(x, t, k)`
  with tests pinning the GLSL literals (the `gaussian9_weights`
  precedent).
- FR2 (params): `BloomParams { threshold, knee, intensity, levels: u8,
  level_weights: [f32; 5] }`; `spec_defaults()` = `(1.0, 0.5, 0.85, 4,
  [1.0, 0.8, 0.6, 0.4, 0.3])`; `levels` clamped to `[2, 5]`;
  `for_tier(QualityTier)` = 3 / 4 / 5.
- FR3 (chain, `main.rs`): `HdrChain` holds `down[k]` and `up[k]` images
  for `k = 0..levels` (down: ½ … ½^levels; up: same extents), each with
  its own framebuffer + descriptor set; frame order: prefilter(scene) →
  down[0]; down[k] → down[k+1]; up[last] = down[last] (alias-free: a
  copy pass or `up[last]` written by a pass-through); up[k] =
  tent(up[k+1])·w[k+1] + down[k]; resolve(scene, up[0]). **No image is
  written twice; no image is read in the pass that writes it.**
- FR4 (structural pin): a test walks the recorded chain description
  (a `Vec<PassDesc { writes: ImageId, reads: Vec<ImageId> }>` built by
  the same code that records commands) and asserts every `ImageId` is
  written exactly once and never read by its writing pass. This is the
  first *executable* form of the Intel rule.
- FR5 (no-bloom switch): `GAME_DEBUG_COSMIC_BLOOM=0` → prefilter only,
  resolve with `down[0]` (as today with A).
- FR6 (grade): `COSMIC_DEMO_BLOOM_INTENSITY 2.2 → ≤ 1.2`,
  `COSMIC_MAP_BLOOM_INTENSITY 1.2 → ≤ 0.9` (targets; final values from
  shots, recorded).
- FR7 (LDR bypass): unchanged (no HDR format → direct draws).

## Non-functional requirements

- NFR1 (budgets): Low 3 levels → 1 + 3 + 2 = 6 fullscreen-triangle
  passes at ≤ ½ res (today 5); images: `scene + 2·levels` (Low 7 vs 6
  today; High 11) with total pixel count ≤ 1.4× today (geometric
  series). `quality.md` row updated with the formula. Memory at 1080p
  High: ≈ 16 MB HDR (R16G16B16A16) — recorded.
- NFR2 (Intel rule): FR4 pin + a manual check on the UHD 620 machine
  (`slab` + `demo` captures, no coloured squares) recorded in the DoD.
- NFR3 (shaders): fragment shaders are texture fetches + FMA only; no
  `exp`. Soft knee is a rational polynomial.
- NFR4 (determinism): capture determinism gate (F0) passes with the new
  chain on the reference GPU.
- NFR5 (transients): images rebuild with the swapchain (existing
  `build_hdr_chain` pattern); no per-frame allocation.

## Definition of Done

1. Shots `slab-after.png`, `demo-after.png`: Tier A hub halos measure
   ≥ 60 px radius at half-max on a 1080p capture (ANALYST samples a
   radial profile), core stays tight (≤ 8 px at 90 %); faint threads
   do not bloom into slabs.
2. Intel UHD 620: `slab` + `demo` captures show no corruption
   (recorded shot paths); FR4 structural pin green.
3. `BloomParams` CPU mirrors pin the GLSL literals (13-tap offsets,
   tent weights, knee); `for_tier` = 3/4/5; `levels` clamp test.
4. Bloom intensities lowered per FR6 and recorded; `GAME_DEBUG_COSMIC_BLOOM=0`
   still works; LDR bypass untouched.
5. Old blur shader + 5-target chain removed (no `bloom_a_fb` … `bloom_e_fb`,
   no `BLOOM_BLUR_FRAG`).
6. Docs: `rendering.md` bloom paragraph rewritten, `quality.md` row,
   Intel report gets a "now pinned by test" note, techstack version
   bump; links resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Depends on `cosmic-hub-hierarchy` (bloom inputs are correct only
  after tiering — otherwise the wide chain would bloom 6000 pins) and
  `cosmic-capture-harness`. Sixth feature on branch `v0.3.3`.
- The 13-tap and tent kernels are the Jimenez 2014 ("Next Generation
  Post Processing in Call of Duty") formulation; constants recorded in
  `post.rs` with the citation.
- Engine remains GPU-API-free (GLSL strings + CPU mirrors); all
  `vulkano` code in `debug`.

## Open questions

- Whether the smallest level should be 1/32 on High (5 levels) — at
  1080p that is 60×34 px; fine. Keep 5; revisit only if the halo tail
  looks blocky in shots (then stop at 4).
- Karis average in the prefilter (firefly suppression) — include if
  1.5 px splats flicker in captures; decided from shots, not upfront.
