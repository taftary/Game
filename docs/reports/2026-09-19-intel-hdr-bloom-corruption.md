# Intel UHD 620 — HDR bloom ping-pong image corruption

**Date:** 2026-09-19
**GPU:** Intel UHD Graphics 620 (driver 1656914, Vulkan 1.3.215)
**Severity:** visual-only (no crash, no validation error)
**Root cause:** image aliasing across render passes — writing then reading then rewriting the same `ImageView` in a single command buffer
**Fix commit:** `18f98ef` (in-progress, pending commit of the dedicated-target patch)

---

## 1. Symptom

Colored square artifacts appear overlaid on correctly-rendered cosmic
braided filaments when the HDR post-processing chain is active. The
squares are:

- Perfectly axis-aligned (grid-aligned, not rotated)
- Sharp-edged (not fuzzy or gradient)
- Bright, saturated, primary-colored (red, green, blue independently)
- Scattered across the screen, clustered near bright filament hubs
- Absent when the HDR post chain is disabled (`GAME_DEBUG_COSMIC_POST=0`)
- Absent in LDR mode (no HDR format at all)

The squares are **not** geometry — they are texel-read artifacts from
the bloom blur chain reading corrupted image data.

---

## 2. Investigation timeline

### Round 1 — Initial hypothesis (buffer values)
**Assumption:** the CPU-side enrichment buffer emits extreme or NaN values.
**Action:** `enrichment_layouts_are_finite_and_bounded` test scans all
~2.2M nominal vertices — finite, in-band.
**Result:** CPU buffers are clean. Corruption is created on-GPU, not in
staging buffers.

### Round 2 — Freshness verification
**Assumption:** stale binary (exe lock prevents relink).
**Action:** added build tag `cosmic visual build r2: ...` log line.
**Finding:** relink blocked twice by orphan `game_debug.exe` processes
(held the exe lock, `Accès refusé` at link step). Killing orphans
before `cargo run` ensures fresh binary.

**Lesson:** always verify the build tag in the log before diagnosing
visual bugs. If the tag is missing, the binary is stale.

### Round 3 — HDR vs scene bisect (`GAME_DEBUG_COSMIC_POST=0`)
**Action:** skip the entire HDR post chain (resolve renders to
swapchain directly).
**Result:** clean. Scene draws, layouts, shaders, buffers exonerated.
Fault is in: scene image/framebuffer, bright extract, blur passes (×4),
resolve, sets, sampler.

### Round 4 — Blur vs scene/bright/resolve bisect (`GAME_DEBUG_COSMIC_BLOOM=0`)
**Action:** skip the four blur passes; scene → bright → resolve still
runs.
**Result:** clean. Scene, bright, resolve exonerated. Fault is isolated
to the blur passes (or their ping-pong images/sets).

### Round 5 — Root cause identified
**Finding:** the original bloom chain used a two-image ping-pong
pattern:
```
bright → half_a (written by bright pass)
blur-H: half_a → half_b
blur-V: half_b → half_a  ← half_a is read above, now rewritten
blur-H: half_a → half_b  ← half_b is read above, now rewritten
blur-V: half_b → half_a
```
This is API-legal (Vulkan allows read-after-write within a command
buffer via subpass dependencies), but Intel UHD 620 driver does not
handle it correctly — the image contents are silently corrupted.

**Validation:** no Vulkan validation error is raised. The corruption is
entirely driver-internal.

### Round 6 — Fix applied
**Action:** replaced the A/B ping-pong with five dedicated targets:
```
bright → A   (written once, read by blur-H A→B)
blur-H: A → B  (written once, read by blur-V B→C)
blur-V: B → C  (written once, read by wide-H C→D)
wide-H: C → D  (written once, read by wide-V D→E)
wide-V: D → E  (final bloom, read by resolve)
```
No image is ever written twice in the same frame. Each image is
written exactly once and only read afterwards.

A second resolve descriptor set (`resolve_nobloom_set`) samples
scene+A instead of scene+E, keeping the `BLOOM=0` bisect path valid
(E is unwritten when bloom is disabled).

**Result:** squares gone. Lacy filaments + gentle hub bloom, clean on
Intel UHD 620.

---

## 3. Why validation didn't catch it

Vulkan validation checks:
- Layout transitions (correct — we transition to
  `COLOR_ATTACHMENT_OPTIMAL` before writing, `SHADER_READ_ONLY` before
  reading)
- Descriptor set bindings (correct — each set binds a valid image)
- Render pass compatibility (correct — same post pass used throughout)

It does **not** check:
- Whether an image being read in a draw call was recently written in
  the same command buffer (aliasing is API-legal)
- Driver-specific handling of read-after-write hazards

This is a known class of driver bugs on Intel integrated GPUs. The
validation layers are silent.

---

## 4. The rule for future work

**Never reuse an image as both color attachment (write) and shader
input (read) in the same command buffer on Intel UHD Graphics 620.**

If you need to read what you just wrote, use one of:
1. **Dedicated images:** write to image A, then read from A to write to
   B. Each image is written exactly once.
2. **Two-pass split:** render pass 1 writes A, then a pipeline barrier,
   then render pass 2 reads A to write B. (Still risky on Intel — prefer
   option 1.)
3. **Ping-pong with fence sync:** submit pass 1, wait for GPU fence, then
   submit pass 2. (Kills pipelining; never worth it for bloom.)

**Option 1 (dedicated images) is the safe default.** The VRAM cost is
trivial (4 extra half-res `R16G16B16A16_SFLOAT` images = ~12 MB at
1080p).

---

## 5. Bisect commands reference

When diagnosing post-chain visual bugs on Intel UHD 620, use these
flags to isolate stages:

| Flag | What it skips | What remains |
|------|---------------|--------------|
| `GAME_DEBUG_COSMIC_POST=0` | Entire HDR post chain | Scene draws direct to swapchain |
| `GAME_DEBUG_COSMIC_BLOOM=0` | Blur passes (×4) | Scene → bright → resolve |
| Both unset | Nothing | Full bloom chain |

Always verify the `cosmic visual build r2` log tag before trusting a
run — orphan processes can block the relink and leave a stale binary.

---

## 6. Files involved

- `crates/debug/src/main.rs` — `HdrChain` struct, `build_hdr_chain`,
  bloom render passes, resolve-set selection
- `crates/debug/src/cosmic_web.rs` — sprite enrichment (CPU buffers,
  exonerated)
- `crates/engine/src/render/post.rs` — bloom shaders (exonerated)
- `docs/reports/2026-09-19-intel-hdr-bloom-corruption.md` — this file

---

## 7. TL;DR for agents

> **Intel UHD 620 corrupts images reused across render passes in the
> same command buffer.** If you see colored squares in HDR bloom output,
> check whether any image is written then read then rewritten. Replace
> the ping-pong with dedicated images (one write per image, then only
> reads). Validate with `GAME_DEBUG_COSMIC_BLOOM=0` and
> `GAME_DEBUG_COSMIC_POST=0` bisects. Always verify the build tag
> before diagnosing — orphan processes can block the relink.
