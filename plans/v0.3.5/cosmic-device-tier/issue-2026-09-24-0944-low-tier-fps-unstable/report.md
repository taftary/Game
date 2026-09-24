# Issue report — cosmic-device-tier / issue-2026-09-24-0944-low-tier-fps-unstable (UTC)

Specs: [`specs.md`](specs.md)

## Roles

Investigated by (role: DEV): DEV (code-reading investigation, 2026-09-24) · Fix direction reviewed by TECHLEAD: _(pending)_ ·
ARCHITECT review (if invariant touch): _(pending — a shader-branch fix touches the shared window snippet contract)_

## Root cause

Two stacked mechanisms, neither related to the stale-frame fix:

**R1 — the spikes are vsync misses on a nearly-full frame budget.**
The swapchain is created with default present mode = Fifo
(`crates/debug/src/main.rs:8738`), so 60 Hz frames quantize to
16.7/33.3 ms. Prepass averages 15.2 ms, leaving ~1.5 ms of headroom;
any jitter tips the frame to 33 ms. That is the observed pattern
(mostly green, red every ~8 bars, avg 18.5 / max 38.6). The 240-sample
maxima additionally still contain the `F4` rebuild frame itself
(synchronous multi-MB uploads + chain realloc on the event loop), so
frame max 38.6 / bloom max 4.0 / march max 2.0 overstate the steady
state. Nothing in the stale-frame fix can spike: one empty
quarter-res clear + one float select per frame (March row reads 0.0
avg in the operator's own numbers).

**R2 — Low pays full fill for fragments that add exactly zero
(fade-without-cull).** `GLOW_FRAG` (`main.rs:460`) and `SPLAT_FRAG`
(`main.rs:481`) have no zero-alpha discard — zero-visibility
fragments still run the full fragment shader and blend zero through
the additive `One, One` chain. Three paths produce such fragments,
all by alpha/color zeroing with no geometry cull:

- slab / fog window: `cosmic_window_vis` multiplies `v_alpha`
  (`GLOW_VERT`, `main.rs:455`; `SPLAT_PROC_VERT`, `main.rs:638`)
  but `gl_PointSize` is untouched — out-of-slab sprites rasterize
  full quads and add 0;
- transfer weight: below-mean splats carry `transfer_w == 0` in
  `v_color` (`main.rs:654`) and add exactly 0.0 at full quad cost;
- near-eye fade (`main.rs:642`): same shape.

Consequence: **`S` (slab) cannot improve fps by construction** — it
only fades, never culls. The "8.3× fill relief" in the
`cosmic-depth-window` DoD is a CPU-side listed-cell statistic (12% of
cells in slice), not measured GPU fill. This explains the operator's
"S is not working well" if it meant frame rate. (If it meant the key
does nothing at all: `S` is inspector-only — both arms gate on
`content == CosmicWeb && !GameDemo`, `main.rs:9750`/`main.rs:10089`;
in Game Demo `S` is thrust, and a focused seed/subdiv/radius field
swallows the keystroke, `main.rs:9755`. Still needs the operator's
confirmation of which it was.)

The codebase already owns the correct pattern: the sphere-cull writes
`gl_Position` outside clip + zero size + zero alpha ("degenerate-draw
contract", `main.rs:611`), which lets the GPU clip the point for
free. Fog/slab/transfer never use it.

## Evidence

- Operator rows: prepass owns the frame (15.2/18.5 avg); march 0.0 avg
  (fix behaving); prepass max 21.3 << frame max 38.6 (worst frames come
  from outside the timestamped passes).
- Shader reads: `GLOW_VERT`/`SPLAT_PROC_VERT`/`GLOW_FRAG`/`SPLAT_FRAG`
  refs above; only `MAP_FRAG` (`main.rs:366`) has a `discard`, and the
  cosmic pipelines don't use it.
- Precedent: CDT-005 measured Low inspector *slower* than Medium
  (prepass 40.9 vs 32.7) — consistent with R2 (Low's 212k world-sized
  sprites vs 44k in march mode + 1M splat invocations, all
  unculled in voids).

## Alternatives considered

- **Stale-frame-fix regression:** ruled out (March 0.0 avg; added GPU
  work is one clear of a quarter-res target; no per-frame CPU added).
- **Rebase-swap hitches:** ruled out as the periodic source (swap only
  fires while travelling; operator scene is static; ~2 ms when it fires).
- **Slab as the fps remedy:** ruled out by R2 (fade-only). Slab stays
  the visual-void tool, never a perf lever, until a cull lands.

## Fix direction

Not in this issue (needs PO priority + the operator's remaining
readings: GPU model, window size, release-vs-dev, Medium/High rows).
Candidates for the queued post-v0.3.5 work, cheapest first:

1. **Degenerate-branch on exact-zero** (shader-only, zero visual
   change, follows the sphere-cull precedent): in `GLOW_VERT` /
   `SPLAT_PROC_VERT`, when the slab term is exactly 0 (outside
   `slab_half + 5`, adds exactly 0.0) or `transfer_w <= 0.0`, emit the
   degenerate position + zero size instead of a full quad. Fog stays
   untouched (smooth falloff, never exactly zero — culling it would
   change the grade).
2. **CPU skip of zero-weight veil sprites** at `upload_cosmic_glow`
   build time (saves vertices *and* fill; buffer rebuilds are
   seed-path-only, never per-frame).
3. Keep the queued `cosmic-splat-culling` / `cosmic-splat-bricks` /
   `cosmic-fill-levers` features as the structural answer; (1) is a
   valid incremental slice of them.
