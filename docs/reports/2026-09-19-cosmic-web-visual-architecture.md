# Cosmic Web — visual architecture and rendering pipeline

**Date:** 2026-09-19
**Scope:** end-to-end description of how the cosmic web is generated, enriched, and rendered to produce the current visual
**Status:** reference report — no changes to code

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

### 3a. Braided filament strands (`braid_segments`)

Each filament link gets 1-3 strands (by density) twisting around the
link trunk:

- **10 subdivisions** per strand, polyline segments.
- **Shared trunk wander**: low-frequency sinusoidal wobble that all
  strands of a link follow (phase, amplitude from seeded RNG).
- **Per-strand twist**: sinusoidal orbit around the trunk with
  random phase, windings (1-2), and lateral mix.
- **Taper**: `sin(t * Pi)` so strands melt to zero at node endpoints,
  creating smooth transitions into cluster hubs.
- **Lateral amplitude**: ~1.5 Mpc (`BRAID_AMPLITUDE_MPC`).
- **Colors**: dim indigo `[0.40, 0.42, 0.85]` at low density grading
  to bright cyan-violet `[0.72, 0.78, 1.05]` at full density.
- **Alpha**: 0.18-0.58, multiplied by √taper (not taper directly).

### 3b. Particulate grain (`grain_cloud`)

Up to 800,000 sprite points along the braid strands:

- **Budget**: 8 grain points per Mpc of link, density-weighted, capped.
- **Placement**: each point lands on a strand of the same braid shape
  (re-derived independently — no shared stream state between braid
  and grain passes) with Irwin-Hall-3 transverse jitter (sigma 0.8 Mpc).
- **Color**: lavender-white `[0.68*bright, 0.62*bright, 1.0*bright]`,
  size 1.5-2.5 px, alpha 0.08.

### 3c. Node impostors (`node_impostors`)

Two sprites per node:

- **Hot core**: mass-graded emissive color (blue-white dwarfs to
  yellow-white giants, RGB from `[0.62, 0.70, 1.00]` to
  `[1.00, 0.93, 0.72]`), multiplied by emissive factor 1.2-2.5x to
  cross the bloom threshold. Fixed pixel size (2.5-6 px). This is the
  bloom target — the bright cores produce the soft glow halos.
- **Soft halo**: pale cyan, faint (alpha 0.10), world-sized in Mpc
  (2-8 Mpc diameter based on mass log-grade `l`, same as the core
  color grade — `node_impostors` never reads `virial_radius_mpc`).
  Shrinks with distance.

### 3d. Base descriptor glow (`glow_point_cloud`)

Dwarf glow points from the descriptor: lavender `[0.58, 0.55, 0.88]`,
1.5 px, alpha 0.12. These fill the filament bodies with a dim
particulate haze.

---

## 4. GPU rendering

### Pipelines (in `crates/debug/src/main.rs`)

Three pipeline types (five compiled objects — LDR + HDR scene variants
for Glow and Webline, shared Map). Glow and Webline are additive;
Map is standard alpha:

| Pipeline | Topology | Blend | Role |
|----------|----------|-------|------|
| **Glow** | `PointList` | additive (`SrcColor=One, DstColor=One`) | Grain, dwarf glow, node cores + halos |
| **Webline** | `LineList` | additive | Braid filament strands |
| **Map** | `PointList` | standard alpha | Inspector player marker |

All cosmic pipelines use **no depth write** — overlapping sprites and
lines accumulate freely, creating brighter intersections at nodes.

### Vertex shaders

Both cosmic vertex shaders apply a bounded Hubble redshift tint from
view depth:

```
z = min(redshift * max(clip.w, 0), 0.5)
tint = (1 + 0.9*z, 1, 1/(1 + 1.2*z))
dimming = 1/(1 + 0.8*z)
```

This makes distant structures progressively redder and dimmer, providing
depth cueing without a volumetric pass.

The glow vertex shader also computes point size by sprite kind:
- kind 0: fixed pixel size (cores, grain, glow)
- kind 1: world-sized by `px_scale / clip.w` (node halos)

The glow fragment shader applies a **rim-zero quadratic falloff**
`(1 - 4*d^2)^2` for soft round sprites, premultiplied output.

### Draw order

1. Deep indigo clear: `[0.012, 0.008, 0.030, 1.0]` — near-black
   violet. Voids read as negative space against additive filaments.
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

- **Exposure**: 1.0
- **Bloom intensity**: 0.85
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
| Background/clear | `[0.012, 0.008, 0.030]` | Near-black violet |
| Filament strands (low density) | `[0.40, 0.42, 0.85]` | Dim indigo |
| Filament strands (high density) | `[0.72, 0.78, 1.05]` | Bright cyan-violet |
| Grain particles | lavender-white, alpha 0.08 | Brightness 0.35-0.9 |
| Dwarf glow points | `[0.58, 0.55, 0.88]` | Alpha 0.12 |
| Node cores (dwarfs) | `[0.62, 0.70, 1.00]` | Blue-white, emissive x1.2 |
| Node cores (giants) | `[1.00, 0.93, 0.72]` | Yellow-white, emissive x2.5 |
| Node halos | pale cyan | Alpha 0.10, world-sized |

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

- **Redshift saturation**: `COSMIC_REDSHIFT_PER_MPC = 0.004` hits the
  z = 0.5 cap at 125 Mpc of view depth. Most of a 500 Mpc web viewed
  from one edge renders at maximum redshift — the primary depth cue is
  effectively binary beyond 125 Mpc.
- **No depth occlusion**: depth test with writes off and nothing writing
  depth; distant node cores shine through foreground filaments. Combined
  with saturation, depth ordering beyond 125 Mpc collapses.
- **gl_PointSize clamp at 256 px** (`main.rs:396`): near-camera
  world-sized halos saturate silently.
- **Zero culling / LOD**: ~2.18 M vertices and ~1 M additive sprites
  are drawn every frame regardless of camera distance or frustum.
- **Braid vertex duplication**: LineList emits 20 verts/strand vs 11
  for a strip (+82% vertex traffic on 1.22 M braid verts).
- **Env kill-switches**: `GAME_DEBUG_COSMIC_POST=0` forces LDR;
  `GAME_DEBUG_COSMIC_BLOOM=0` keeps HDR scene but skips blur chain
  (`main.rs:5115-5117`). Present in code, not previously documented.
- **Dual buffer sets**: ~137 MB resident (demo + inspector), inspector
  rebuilt redundantly on every demo rebase (`main.rs:4806-4831`).
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

---

## 9. TL;DR for agents

> The cosmic web is a **seeded Zel'dovich perturbation** on a 128^3
> lattice producing ~6000 nodes + ~20000 filament links, enriched with
> **braided strand polylines** (1-3 per link, sinusoidal twist, tapered
> at endpoints), **particulate grain** (~800K sprites), and **emissive
> node impostors** (bloom-target cores + world-sized halos). Rendered
> through **additive point/line pipelines** on a deep indigo clear,
> with **HDR bloom** (bright extract + 2-scale separable Gaussian blur +
> ACES resolve, five dedicated write-once targets) and **Hubble redshift
> tinting** in the vertex shader. Generation replays identically per
> `(seed, version, params)` on every platform (hash quantized to hide
> 1-ulp libm drift). Enrichment is render-only, uses std sin/cos (same-
> platform replay only, not cross-platform bit-identical), and never
> affects gameplay. Key files: `web/mod.rs` (generation),
> `cosmic_web.rs` (enrichment), `main.rs` (GPU pipelines + HdrChain).
