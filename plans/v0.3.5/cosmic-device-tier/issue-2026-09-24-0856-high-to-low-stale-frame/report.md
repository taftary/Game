# Issue report — cosmic-device-tier / issue-2026-09-24-0856-high-to-low-stale-frame (UTC)

Specs: [`specs.md`](specs.md)

## Roles

Investigated by (role: DEV): DEV (code-reading investigation, 2026-09-24) · Fix direction reviewed by TECHLEAD: _(pending)_ ·
ARCHITECT review (if invariant touch): _(required — bloom write-once invariant is touched, pending)_

## Root cause

Primary (blocking): **the veil-march HDR target is read but never
written in `VeilMode::Sprites`.**

1. `F4` High → Low switches veil `March { 48/32 }` → `Sprites` and
   bloom levels 5/4 → 3, then `refresh_cosmic_seed` builds a brand-new
   `HdrChain` (new `Image` objects for scene, all `down`/`up` levels,
   and the quarter-res march target).
2. Every frame the bloom pyramid levels are all written (prefilter +
   downs + pass-through copy + tent ups), and the scene target is
   cleared + drawn — so those fresh images become defined on frame one.
3. The march target is the exception: both recording paths skip
   `record_veil_march` in Sprites mode (windowed
   `crates/debug/src/main.rs:10658`, capture
   `crates/debug/src/main.rs:2410`). Skipping the pass also skips its
   `begin_post_pass` black clear (`crates/debug/src/main.rs:6721`), so
   the fresh march `Image` (created by `create_post_view`,
   `crates/debug/src/main.rs:5793` — `COLOR_ATTACHMENT | SAMPLED`, no
   initialization) stays **undefined** forever.
4. The resolve samples it anyway, every frame, at full grade:
   `resolve_pair(&up[0].view, &march_view, …)` binds march unconditionally
   (`crates/debug/src/main.rs:8592`), `record_cosmic_view_arm` draws the
   resolve with `march_gain = VEIL_MARCH_RESOLVE_GAIN = 30.0`
   (`crates/debug/src/main.rs:6964`, `crates/debug/src/main.rs:823`),
   and the resolve GLSL is pinned as
   `scene + bloom·i + march·e` (`crates/debug/src/main.rs:11792`).
   Undefined march texels ×30 is the "weird display".
5. Why it looks like the *old* frame: the old chain is dropped at the
   rebuild, its device pages return to the `StandardMemoryAllocator`
   pool, and the new march image (same quarter-res extent, same format)
   very likely reuses the same pages — still holding the previous
   tier's march contents. On shared-memory iGPUs (UHD 620) reuse is the
   norm, so High → Low deterministically ghosts the High march over the
   Low sprites scene. A fresh `--tier low` capture on an idle device may
   read zeroed pages and look fine by luck, which is why the CDT-004
   Low capture passed while the windowed cycle fails.

Contributing (test blind spot): the write-once pin asserts the wrong
description in Sprites mode. `record_bloom_chain`
(`crates/debug/src/main.rs:6786`) always asserts
`describe_veil_chain(levels, true, bloom_enabled)` — march included —
even when the caller is about to skip the march pass. The
`describe_veil_chain(levels, false, …)` form (no march pass, resolve
reads scene + bloom only — `crates/debug/src/cosmic_bloom.rs:118`)
does not match the shader either, which always binds/samples march.
So neither description models what Sprites mode actually records, and
the pin stays green while the frame samples an unwritten image.

## Evidence

- Rebuild path: `cycle_cosmic_tier` → `refresh_cosmic_seed` →
  `build_hdr_chain` with `bloom_levels_for(tier)` — code refs in
  specs § Logs/Evidence; swapchain-recreate path
  (`crates/debug/src/main.rs:10383`) shares the same skip, so
  resize-in-Low and boot-in-Low hit the same unwritten image.
- Skip sites + "cleared target adds ~0" comments asserting a clear that
  never records: `crates/debug/src/main.rs:10658`,
  `crates/debug/src/main.rs:2410`.
- Clear lives only inside the skipped pass:
  `begin_post_pass` black clear (`crates/debug/src/main.rs:6721`),
  post pass `load_op: Clear` (`crates/debug/src/main.rs:5724`),
  march writer `record_veil_march` (`crates/debug/src/main.rs:6910`).
- Unconditional march read at full gain: `crates/debug/src/main.rs:8592`,
  `crates/debug/src/main.rs:6964`, `crates/debug/src/main.rs:11792`,
  `crates/debug/src/main.rs:823`.
- `MARCH_FRAG` (`crates/debug/src/main.rs:686`) clamps
  `steps = max(pc.origin_steps.w, 1.0)`, so "just run the march with
  steps=0" would still march once — not a valid clear substitute.
- Spec cross-check: `cosmic-device-tier/plan.md` R-2 ("veil switch
  staleness … closed by construction") assumed the seed rebuild covered
  the veil switch; it covers buffers + worker but not the march-target
  lifetime, which is why the review-verified L-2 (no live `F4` keypress
  in automation) missed it.

## Alternatives considered

- **Bloom-level mismatch (5 → 3 leaves stale `up[3..]`):** ruled out.
  The chain is fully rebuilt; `resolve_set` binds the new `up[0]`, and
  every level in the new chain is written each frame. No dangling
  descriptor (all sets rebuilt in `build_hdr_chain`).
- **Glow-buffer staleness (veil sprites missing):** ruled out.
  `refresh_cosmic_seed` re-uploads both glow buffers with the new
  `veil_mode` (`crates/debug/src/main.rs:7871`); splat `count =
  cells × k` follows `splat_k` per frame (`crates/debug/src/main.rs:8061`).
- **In-flight use-after-free of the old chain:** ruled out as the
  visible cause. One frame in flight behind `previous_frame_end`;
  `Arc` keeps old images alive for the in-flight CB. Would flicker one
  frame, not persist.
- **Swapchain-extent mismatch:** ruled out. Extent is unchanged across
  an `F4` cycle; scene/bloom extents derive from the same extent.
- **March *contents* stale (old volume):** ruled out as primary. The
  veil volume is re-uploaded on the seed path
  (`crates/debug/src/main.rs:7832`); the problem is the march *target*
  is never written at all in Sprites mode, regardless of volume.

## Fix direction

No grade, tier, shader-math, or budget change (repair only):

1. **Clear the march target every frame it is skipped** (preferred):
   record an empty post pass (`begin_post_pass(march_fb) → end`)
   on the Sprites path in both windowed and capture recording, so the
   image is defined black (0,0,0 → ×30 = 0) and the write-once shape
   ("march target written once per frame") actually holds. Keeps the
   resolve shader and all sets untouched.
2. **Harden the pin:** make `record_bloom_chain`'s debug assert describe
   what is actually recorded (march bool from `veil_mode`, both call
   sites + capture), and extend `cosmic_bloom` tests with the
   Sprites-mode shape (march-cleared, resolve reads march-black).
3. **Defense in depth (optional, TECHLEAD call):** pass `march_gain = 0`
   in Sprites mode as well, so even a future skip reads as zero. Two
   lines, zero grade impact in March mode.

Blast radius: `crates/debug` bin only (two call sites + one helper +
tests). No `engine`/`game`/`tools` diff, no pipeline/layout change, no
new image, no per-frame cost beyond one clear. ARCHITECT review needed
because the bloom write-once description is touched. Verification:
windowed High → Low → Medium cycle on HDR hardware shows clean Low
(sprites, march 0.00 in timing) with no ghost; Low `--tier` capture
byte-stable across repeated runs after a High capture on the same
device (allocator-reuse poison test); full gate list per `quality.md`.
