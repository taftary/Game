# Cosmic Web — visual architecture and rendering pipeline

**Date:** 2026-09-19
**Scope:** end-to-end description of how the cosmic web is generated, enriched, and rendered to produce the current visual
**Status:** reference report — no changes to code

Reference image: [`images/target.jpeg`](images/target.jpeg) (Illustris-style
goal: gaseous blue filaments, golden-red hubs, dark voids). The
`current.png` comparison shot of the 2026-09-19 build was removed on
2026-09-20 (outdated); v0.3.3 `cosmic-capture-harness` replaces ad-hoc
shots with reproducible captures. Sections 1–9 describe the v0.3.2
build as of update-2026-09-19-1933 (ribbon era); §10 covers the
smoke/Illustris-look pass; §11 records the v0.3.3 pivot to a
field-based render. Close reading of the target itself:
[`2026-09-20-cosmic-web-visual-description.md`](2026-09-20-cosmic-web-visual-description.md).
This file was restored from git history on 2026-09-20 (it had been
deleted together with `current.png` while still linked from ADR-024
and the v0.3.2 plans).

---

## 1. Overview

The cosmic web is a procedurally generated large-scale structure of the
universe: galaxy clusters (nodes) connected by filaments, separated by
vast voids. It is the outermost scale of the game (`FrameId::Cosmological`)
and the first thing the player sees at boot.

The visual is built in three layers:

1. **Generation** — a seeded Zel'dovich perturbation pipeline produces a
   physically-motivated `WebDescriptor` (nodes, links, glow points).
2. **Enrichment** — deterministic CPU-side vertex layouts add braided
   filament strands, particulate grain, and emissive node impostors
   (render-only, never fed back into gameplay).
3. **GPU rendering** — additive point/line pipelines with HDR bloom and
   Hubble redshift tinting turn ~2M vertices into the final image.

---

## 2. Generation pipeline

**Entry point:** `generate_cosmic_web(seed, params)` in
`crates/engine/src/universe/web/mod.rs:54`

A single pure function composes four stages. Deterministic per
`(seed, UNIVERSE_VERSION, params)` — same triple replays identically
on every platform.

### Stage A — Gaussian initial field (`web/field.rs`)

- White Gaussian noise on a periodic 128^3 integer Lagrangian lattice
  (4 Mpc/cell = 512 Mpc box).
- Irwin-Hall shaping (sum of 12 uniform randoms = N(0,1), no transcendentals).
- Weighted dyadic box smoothings at 3 scales (sigmas 1, 2, 4 cells;
  radii 2, 3, 7 cells via `round(σ·√3)`; weights 0.35, 0.7, 1.0)
  approximating a Lambda-CDM power spectrum rollover.
- Output: standardized zero-mean, unit-variance potential field Psi.

### Stage B — Zel'dovich displacement (`web/displace.rs`)

- Comoving displacement: `x = q - D+ * grad(Psi)` where D+ = 3.0
  (growth factor / "cosmic time").
- Central finite differences for the gradient.
- Nearest-grid-point (NGP) deposit of unit-mass tracers into Eulerian
  density. Intentionally NOT smoothed (preserves the high dynamic range
  that creates sharp void walls and dense nodes).
- Mass is conserved exactly (periodic box, every tracer lands somewhere).

### Stage C — T-web classification (`web/classify.rs`)

- Tidal tensor H_ij from second central differences of the potential.
- Jacobi eigenvalue solver (fixed 8 sweeps, sqrt-only rotations —
  no sin/cos/convergence branches, ensuring bit-identical replay).
- Classification per cell by number of eigenvalues below
  −λ_th (strongly negative, collapsed axis):
  3 = **NODE**, 2 = **FILAMENT**, 1 = **SHEET**, 0 = **VOID**
  (Forero-Romero T-web). Threshold `web_threshold = 0.06`; the
  test is `eig < −web_threshold` (`classify.rs:174`).
- Deep voids (density < 10% of mean) skip the eigensolver.
- Peak extraction: local density maxima in 3x3x3 neighborhoods,
  densest-first, min 6 Mpc separation, targeting ~6000 nodes.
- **Press-Schechter n=0 halo mass function**: power law + exponential
  cutoff above M* = 6e13 Msun, inverted from a Simpson-integrated CDF
  table. Denser peaks get rarer (more massive) ranks.

### Stage D — Descriptor assembly (`web/descriptor.rs`)

- **Nodes**: ~3000-8000 halo nodes (test band; nominal exactly 6,000 at
  seed 1234) with f64 Mpc positions (parabolic
  sub-cell refinement), Press-Schechter masses (5e12 to >3e14 Msun),
  virial radii from M = 200*rho_c*4/3*pi*r^3.
- **Filament links**: spatial grid linking, max 60 Mpc separation, 8
  NGP density samples along each candidate segment, density cut at 1.2x
  mean, capped at 8 shortest links per node. ~20,000 links.
- **Dwarf glow points**: seeded along links with ~2 Mpc transverse
  Gaussian jitter, two-pass budget capped at 150,000 points.
- **Home node**: group-band node nearest center (1e12-1e13 Msun,
  Local Group analog).
- **Void fraction**: 60-90% of cells.

### Runtime parameters (`web/params.rs`)

| Parameter | Nominal | Role |
|-----------|---------|------|
| `lattice_cells` | 128 | Grid resolution per axis |
| `cell_size_mpc` | 4.0 | Mpc per cell |
| `descriptor_radius_mpc` | 250.0 | Camera extent + validation (not a generation cut) |
| `growth_factor` | 3.0 | Zel'dovich displacement strength |
| `web_threshold` | 0.06 | T-web eigenvalue collapse cutoff |
| `void_density_ratio` | 0.1 | Void definition (< 10% mean density) |
| `peak_density_ratio` | 1.7 | Node-candidate density cut |
| `link_density_ratio` | 1.2 | Filament-link density cut |
| `linking_length_mpc` | 60.0 | Max node-pair separation for linking |
| `min_node_separation_mpc` | 6.0 | Greedy exclusion radius |
| `target_node_count` | 6,000 | Upper bound on nodes |
| `mass_star_msun` | 6e13 | Press-Schechter M* |
| `max_glow_points` | 150,000 | Dwarf glow hard cap |

---

## 3. CPU enrichment layer

**File:** `crates/debug/src/cosmic_web.rs`

The base descriptor is enriched with three deterministic visual layers.
These are render-only — they never feed back into selection, flight,
or saves.

### 3a. Ribbon filament strands (`strand_records`)

Each filament link gets 1-3 strand records (by density) expanded
on-GPU into camera-facing ribbon quads (update-2026-09-19-1245 P1 —
the 1-px `LineList` wireframe is retired):

- **Record**: root `a`, trunk `raw`, lateral basis `(u, v)`, wander
  `(phase, amplitude)`, twist `(phase, windings, mix_u, mix_v)`,
  link `rgba` — 96 B, ~3.8 MB for the nominal web (was ~34 MB of
  baked line verts per surface).
- **Expansion**: one instance per strand, 6 verts per segment × 10
  subdivisions; the vertex shader evaluates the same `braid_point`
  math (trunk + shared wander + per-strand twist, sinusoidal, tapered
  to the hubs) the grain pass reuses — ~1.2M tris total.
- **Camera-facing**: ribbon side = segment × view axis
  (guarded normalization), world-space half-width 0.75 Mpc with a
  1.5-px minimum, width × melt profile melting into the hubs.
- **Near-eye fade**: smoothstep 1→6 Mpc on eye-to-midpoint distance
  (the player spawns inside a filament).
- **Colors**: dim indigo `[0.18, 0.22, 0.60]` at low density grading
  to bright blue-violet `[0.48, 0.57, 1.95]` at full density, warmed
  toward amber near hub endpoints in-shader.
- **Alpha**: 0.05-1.00 by density, × melt profile × per-surface
  exposure × near-fade.

### 3b. Particulate grain (`grain_cloud`)

Up to 800,000 sprite points along the braid strands:

- **Budget**: 8 grain points per Mpc of link, density-weighted, capped.
- **Placement**: each point lands on a strand of the same braid shape
  (re-derived independently — no shared stream state between braid
  and grain passes) with Irwin-Hall-3 transverse jitter (sigma 0.8 Mpc).
- **Color**: lavender-white `[0.68*bright, 0.62*bright, 1.0*bright]`,
  size 1.5-2.5 px, alpha 0.08.

### 3c. Node impostors (`node_impostors`)

Three layered sprites per node (update-2026-09-19-1245: a white
pinpoint added so cluster light varies white→gold→amber like the
target, instead of one flat gold):

- **White pinpoint**: near-white `[1.05, 1.00, 0.95]` × (1.2-3.2x),
  smallest (1.5-3.5 px), fixed pixel size. Dwarfs read blue-white
  through the falloff, giants white-gold.
- **Golden mid core**: mass-graded emissive color (blue-white dwarfs
  `[0.60, 0.68, 1.00]` to deep-golden giants `[1.00, 0.82, 0.50]`
  via the shared `mass_level` ramp, 1e12.3–1e15 M☉ — small hubs stay
  blue-white), multiplied by emissive factor 1.5-5.0x to cross the
  bloom threshold. Fixed pixel size (3-12 px — big enough that the
  blur chain keeps a visible halo). This is the bloom target.
- **Soft halo**: cyan→amber by mass, faint (alpha 0.12-0.20),
  world-sized in Mpc (2.5-6 Mpc diameter based on the same
  `mass_level` grade — `node_impostors` never reads
  `virial_radius_mpc`). Shrinks with distance. Band narrowed from
  3-8 Mpc in the second pass so near-camera halos hit the 256 px
  point-size clamp as a smaller, dimmer smudge (real fix: quad
  impostors in the ribbon update).

### 3d. Base descriptor glow (`glow_point_cloud`) — the gas veil

Dwarf glow points from the descriptor, reworked in the second pass of
update-2026-09-19-1933 from fixed-pixel specks to a **gas veil**:
world-sized soft sprites (2.8 Mpc diameter, kind 1) in faint blue
`[0.45, 0.50, 1.00]`, alpha 0.045. They hug the links (the descriptor
emits glow along filaments only, never in voids), so filaments sit in
a volumetric-looking mist like the target reference.

---

## 4. GPU rendering

### Pipelines (in `crates/debug/src/main.rs`)

Three pipeline types (five compiled objects — LDR + HDR scene variants
for Glow and Ribbon, shared Map). Glow and Ribbon are additive;
Map is standard alpha:

| Pipeline | Topology | Blend | Role |
|----------|----------|-------|------|
| **Glow** | `PointList` | additive (`SrcColor=One, DstColor=One`) | Grain, gas veil, node cores + halos |
| **Ribbon** | `TriangleList` (instanced strand records) | additive | Filament tubes (Gaussian lateral falloff) |
| **Map** | `PointList` | standard alpha | Inspector player marker |

All cosmic pipelines use **no depth write** — overlapping sprites and
lines accumulate freely, creating brighter intersections at nodes.

### Vertex shaders

Both cosmic vertex shaders apply a bounded Hubble redshift tint from
view depth (coefficients softened in update-2026-09-19-1933 so golden
hubs survive at depth; safety clamps unchanged):

```
z = min(redshift * max(clip.w, 0), 0.5)
tint = (1 + 0.75*z, 1, 1/(1 + 0.7*z))
dimming = 1/(1 + 0.45*z)
```

This makes distant structures progressively redder and dimmer, providing
depth cueing without a volumetric pass. Both shaders also take a
per-surface **alpha exposure** multiplier (`RibbonPush.exposure`
scales ribbon alpha; `GlowPush.exposure` pre-exists): the
zoomed-out inspector stacks ~50 strands per pixel where the immersive
demo stacks a few, so the two surfaces grade independently
(`COSMIC_MAP_*` vs `COSMIC_DEMO_*` consts in `main.rs`).

The glow vertex shader also computes point size by sprite kind:
- kind 0: fixed pixel size (cores, grain, glow)
- kind 1: world-sized by `px_scale / clip.w` (node halos)

The glow fragment shader applies a **rim-zero quadratic falloff**
`(1 - 4*d^2)^2` for soft round sprites, premultiplied output.

### Draw order

1. Deep indigo clear: `[0.008, 0.005, 0.024, 1.0]` — near-black
   violet (deepened in update-2026-09-19-1933). Voids read as
   negative space against additive filaments.
2. Additive braid lines (webline pipeline).
3. Additive grain + glow + impostors (glow pipeline).
4. Player marker (map pipeline, last, on top).

The indigo clear is critical: additive blending on a black background
produces only light, never subtraction. The near-violet tint gives
voids a subtle cosmic color rather than pure black.

---

## 5. HDR bloom post-processing

**Files:** `crates/engine/src/render/post.rs`, `crates/debug/src/main.rs` (`HdrChain`)

On capable devices (R16G16B16A16_SFLOAT or B10G11R11_UFLOAT_PACK32):

### Pass chain

Five dedicated bloom targets (A–E), all at half-res:

```
HDR scene pass (indigo clear + cosmic draws)
    ↓
Bright extract → target A (half-res, threshold 1.0, ceiling 64.0)
    ↓
Blur H pass 1: A → B (half-res, 9-tap Gaussian, σ = 1.6, step 1/texel)
    ↓
Blur V pass 1: B → C (half-res, same kernel, step 1/texel)
    ↓
Blur H pass 2: C → D (half-res, same kernel, step 2/texels — wider reach)
    ↓
Blur V pass 2: D → E (half-res, same kernel, step 2/texels)
    ↓
Bloom-composite resolve: aces_fit(hdr * exposure + E * intensity)
```

All five targets are allocated at the **same half extent**; pass 2 uses
a wider **2× texel step** for a broader blur kernel, not a quarter-res
downsample. Every target is written exactly once (see rule below).

- **Exposure**: per-surface (update-2026-09-19-1933) — 1.15 demo /
  0.85 inspector
- **Bloom intensity**: per-surface — 2.2 demo / 1.2 inspector (spec
  default 0.85 lifted: the half-res 4-pass chain keeps only ~1/4 of a
  1-px line's over-threshold energy and dilutes point sources
  ~1/(2πσ²), so visible bloom needs the push)
- **ACES fit**: HDR scene clamped to 65000.0 to prevent NaN

### Critical design rule

**Every bloom target is written exactly once, then only read.** No
image is ever reused as both color attachment (write) and shader input
(read) in the same command buffer. This was a hardware workaround for
Intel UHD 620 driver corruption (see
`docs/reports/2026-09-19-intel-hdr-bloom-corruption.md`).

On LDR-incapable devices: the same braid + glow draws go direct-to-
swapchain (no bloom, no tonemap).

---

## 6. Visual characteristics summary

### Color palette

| Element | Color | Notes |
|---------|-------|-------|
| Background/clear | `[0.008, 0.005, 0.024]` | Near-black violet |
| Filament ribbons (low density) | `[0.18, 0.22, 0.60]`, alpha 0.05 | Dim indigo, recedes into voids |
| Filament ribbons (high density) | `[0.48, 0.57, 1.95]`, alpha 1.0 | Blue-violet tubes, bloom; warms amber near hubs |
| Gas veil (dwarf glow) | `[0.45, 0.50, 1.00]`, 2.8 Mpc | Alpha 0.045, world-sized mist |
| Grain particles | lavender-white, alpha 0.08 | Brightness 0.35-0.9 |
| Node pinpoints | `[1.05, 1.00, 0.95]` × 1.2-3.2 | Near-white, 1.5-3.5 px |
| Node cores, dwarfs → giants | `[0.60, 0.68, 1.00]` → `[1.00, 0.82, 0.50]` | Blue-white → deep gold, emissive x1.5-5.0 |
| Node halos | cyan→amber by mass | Alpha 0.12-0.20, 2.5-6 Mpc world-sized |

(Palette values current as of update-2026-09-19-1933.)

### Structure counts (nominal seed 1234)

| Element | Count |
|---------|-------|
| Nodes (galaxy clusters/groups) | ~3,000-8,000 (test band) |
| Filament links | ~20,000 |
| Dwarf glow points | up to 150,000 |
| Grain points | up to 800,000 |
| Void fraction | 60-90% of lattice cells |
| Void median diameter | 10-100 Mpc |
| Filament length class | 50-80 Mpc |

### Depth cues

- **Hubble redshift tint**: distant structures redder + dimmer (shader-
  side, `clip.w`-dependent, capped at z=0.5).
- **Additive overlap**: strands accumulate light at nodes, making hubs
  naturally brighter.
- **Bloom glow**: emissive node cores produce soft halos that scale with
  mass.
- **Node halos**: world-sized sprites that shrink with distance,
  providing parallax.

---

## 7. Integration with the scene hierarchy

- **Frame**: `FrameId::Cosmological` (comoving Mpc), the outermost scale.
  Below it: `Galactocentric`, `SolarSystem`, etc.
- **Player**: `ShipState` in cosmological frame with free-flight momentum
  physics + time compression (10^9 ceiling). Spawns inside a filament
  20-40 Mpc from the home node.
- **Camera**: Chase/Orbit/FirstPerson modes, un-flipped
  `directx::perspective`, distance-derived near/far, 50 Mpc rebase.
- **Picking**: click-select via `project_to_screen` (8 px radius),
  targeting `web.nodes` only — visual enrichment layers never feed
  selection.
- **Dimension tabs**: Game Demo shows player-in-web; Cosmic Web tab
  (dimension 1) shows the same web with orbit/pan/zoom inspector camera.
  Both share CPU layout functions but have separate GPU buffer sets.
- **Galaxy maps**: a separate `BackdropSprite` field (400 dim sprites)
  provides a simpler cosmic web backdrop for the far-away galaxy map
  view.

---

## 8b. Known limitations

- **Redshift saturation**: `COSMIC_REDSHIFT_PER_MPC = 0.002` hits the
  z = 0.5 cap at 250 Mpc of view depth (was 0.004/125 Mpc before
  update-2026-09-19-1933 — the softer ramp keeps the cue monotonic
  across half the visible box and lets gold survive at depth).
  Structures beyond 250 Mpc still render at maximum redshift.
- **No depth occlusion**: depth test with writes off and nothing writing
  depth; distant node cores shine through foreground filaments. Combined
  with saturation, depth ordering beyond 250 Mpc collapses.
- **gl_PointSize clamp at 256 px** (glow shader): near-camera
  world-sized halos saturate silently (mitigated by the narrowed
  2.5–6 Mpc band; real fix = P2 quad impostors).
- **Zero culling / LOD**: ~2.4M ribbon verts and ~1M additive sprites
  are drawn every frame regardless of camera distance or frustum.
- **Ribbon vertex cost**: the ribbon shader evaluates ~12
  `braid_point` calls per segment (~600 trig/strand, ~24M/frame at
  full draw) — measured fine on UHD 620 (60/59.5 fps debug at
  1296×759), but headroom is thin. Fallbacks, in order: cut
  `BRAID_SUBDIVISIONS` 10→6-8, draw indexed (22 unique verts/strand
  vs 60 expanded), distance strand culling (P5 LOD).
- **Near-eye ribbon slabs**: fixed by the 1→6 Mpc eye fade
  (update-2026-09-19-1245 P1) — unfaded camera-adjacent ribbons fill
  the screen white.
- **Env kill-switches**: `GAME_DEBUG_COSMIC_POST=0` forces LDR;
  `GAME_DEBUG_COSMIC_BLOOM=0` keeps HDR scene but skips blur chain
  (`main.rs:5115-5117`). Present in code, not previously documented.
- **Dual buffer sets**: ~100 MB resident (demo + inspector; braid
  dropped ~34 MB → ~4 MB with strand records, glow/grain unchanged),
  inspector rebuilt redundantly on every demo rebase
  (`main.rs:refresh_cosmic`).
- **Player marker**: 1-vertex `Buffer::from_iter` allocated every frame
  (`main.rs:4861-4864`).

---

## 8. Files involved

### Generation (engine crate)

| File | Role |
|------|------|
| `crates/engine/src/universe/web/mod.rs` | Pipeline orchestrator |
| `crates/engine/src/universe/web/field.rs` | Stage A: Gaussian initial field |
| `crates/engine/src/universe/web/displace.rs` | Stage B: Zel'dovich displacement |
| `crates/engine/src/universe/web/classify.rs` | Stage C: T-web classification |
| `crates/engine/src/universe/web/descriptor.rs` | Stage D: renderable descriptor |
| `crates/engine/src/universe/web/params.rs` | Runtime parameters |

### Enrichment + rendering (debug crate)

| File | Role |
|------|------|
| `crates/debug/src/cosmic_web.rs` | CPU vertex layouts, inspector UI |
| `crates/debug/src/cosmic_player.rs` | Player spawning inside filament |
| `crates/debug/src/cosmic_demo.rs` | Demo state management |
| `crates/debug/src/main.rs` | GPU pipelines, shaders, draw calls, HdrChain |

### Post-processing (engine crate)

| File | Role |
|------|------|
| `crates/engine/src/render/post.rs` | Bloom shaders, BloomParams |
| `crates/engine/src/render/cue.rs` | Web density sampling, Hubble redshift |

### Documentation

| File | Role |
|------|------|
| `docs/decisions/ADR-023.md` | Decision record for cosmic-scale player |
| `docs/game/universe.md` | Generation overview |
| `docs/techstack/rendering.md` | Rendering architecture (cosmic extension) |
| `plans/v0.3.2/cosmic-scale-player/notion.md` | Feature specification |
| `plans/v0.3.2/cosmic-scale-player/update-2026-09-18-2328/` | Cinematic refresh spec |
| `plans/v0.3.2/cosmic-scale-player/update-2026-09-19-1933/` | Palette quick pass spec |

---

## 9. TL;DR for agents

> The cosmic web is a **seeded Zel'dovich perturbation** on a 128^3
> lattice producing ~6000 nodes + ~20000 filament links, enriched with
> **ribbon strand records** (1-3 per link, expanded on-GPU into
> camera-facing tubes with Gaussian falloff), **particulate grain**
> (~800K sprites), a **gas veil** (world-sized mist sprites), and
> **3-layer node impostors** (white pinpoint + gold core + amber
> halo). Rendered through **additive ribbon/triangle + point
> pipelines** on a deep indigo clear, with **HDR bloom** (bright
> extract + 2-scale separable Gaussian blur + ACES resolve, five
> dedicated write-once targets) and **Hubble redshift tinting** in
> the vertex shader. Generation replays identically per
> `(seed, version, params)` on every platform (hash quantized to hide
> 1-ulp libm drift). Enrichment is render-only, uses std sin/cos (same-
> platform replay only, not cross-platform bit-identical), and never
> affects gameplay. Key files: `web/mod.rs` (generation),
> `cosmic_web.rs` (enrichment), `main.rs` (GPU pipelines + HdrChain).

---

## 10. Follow-up: Illustris-look pass (2026-09-20, `cosmic-web-illustris-look`)

Render-only, toward `target.jpeg`: strands fray `3–7` per link over
one or two seeded arms (long links split); smoke reshaped from round
blobs to tangent-aligned stretched sheaths (core + halo tiers,
bifurcation warming, aniso falloff); gold beads (`≤60k`) glitter along
spine sub-segments; faint-thread alpha floor lowered so voids stay
dark. Nominal headless: `smoke51953 grain800000 beads59958
impostors18000`. Descriptor, hashes, pipelines, and budgets unchanged;
Low fallback order armed (beads → halo → strands) but not triggered.
Remaining gap: true volumetric bodies and quad node impostors (still
open), distance/frustum culling (still open).

---

## 11. Follow-up: v0.3.3 pivot to a field-based render (2026-09-20, ADR-025)

Seven grading rounds in one day (`cosmic-web-illustris-look` D-1…D-7)
closed as much of the gap as graph decoration can. The remaining
deltas against `target.jpeg` are structural, not tunable:

| Target ingredient | v0.3.2 build | Why grading cannot fix it |
|---|---|---|
| Curved, branching, continuous gaseous filament bodies | 1–6 stretched quads on each 6–60 Mpc *straight* link; length carried by 800k grain dots | Geometry comes from the link graph, which is straight by construction |
| Walls / sheets between voids | none | T-web `SHEET` cells are classified in `classify.rs` then discarded |
| Black voids | full 500 Mpc volume projected, depth dim ≈ 0.81 at 250 Mpc | The reference is a thin slab; every void is filled by structure behind it |
| ~10 blazing hubs + pink member-galaxy scatter | 6000 nodes × 3 emissive impostors, no members | No mass-rank hierarchy; every node is a white pin |
| Four point classes by local density | flat per-link palette | No density → color ramp exists |
| Wide soft hub bloom (~80 px) | half-res σ=1.6 two-scale blur (~20 px) | Blur radius is bounded by the chain design |

The engine already computes the data Illustris-style renders splat:
`displace.rs:56-59` produces one Zel'dovich tracer per lattice cell
(~2M), rounds each to a cell for NGP deposit, and drops the position.
v0.3.3 keeps those tracers (plus the 128³ density/class grid) as a
**non-hashed render sidecar** (`WebField`) and renders *the field*:
adaptive-kernel tracer splats colored by local density, a depth
window (fog + inspector slab), a mass-rank hub hierarchy with member
galaxies, a write-once mip bloom chain, a grid-driven gas veil
(sprites on Low, quarter-res raymarch on Medium/High), and a vista
intro for the demo. The link graph stays the gameplay authority
(nodes, links, home, picking, fly-to); grain, beads, smoke, and the
braid/spine/strand code retire. Feature folders:
[`plans/v0.3.3/`](../../plans/v0.3.3/); decision:
[`ADR-025`](../decisions/ADR-025.md).