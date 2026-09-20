# Notion — cosmic-gas-veil-v2

## Status

`planned` (PO sign-off 2026-09-20; UX consulted; ARCHITECT + TECHLEAD
breakdown in `plan.md`)

## Context

The filament **bodies** — the soft translucent indigo-violet gas that
makes the target read as "looking through a slab" (report §4 "Form",
§5 walls, §8 "volumetric / line-splat filaments with alpha proportional
to local density") — are today approximated by two things: ~52k smoke
quads (`smoke_puffs`, `cosmic_web.rs:600-697`; `SMOKE_VERT/FRAG`,
`main.rs:434-543`) stretched along straight links, and the descriptor's
150k glow points drawn as 2.8 Mpc "gas veil" sprites
(`glow_point_cloud`, `:161`). Both follow the link graph, so they
cannot show curvature, branching, or walls, and they cost seven grading
rounds without reaching the look.

`web-field-export` provides the 128³ grid: per cell a T-web class
(void / sheet / filament / node) and a quantized log-overdensity. That
grid **is** the gas body — filament and sheet cells with their density.
This feature renders the grid and retires smoke + the old veil.

ADR-025 §2/§4: sprites on Low, quarter-res emission-only raymarch on
Medium/High; Low must read as the same web with less body.

## Problem & Needs

- **Continuous bodies** around the splats: the splats are galaxies /
  matter samples; the gas between them is what turns dots into
  threads (report §4).
- **Walls** (report §5): faint veils between filaments, no beads —
  they exist in the classification and nowhere in the picture.
- **Alpha ∝ density**: thick trunks near hubs, hair-thin wisps at void
  edges — from data, not from per-link constants.
- **Two tiers, one picture**: phones cannot raymarch a 128³ volume at
  30 fps; desktops can. Low must not look like a different game.
- **Retire the smoke path**: two overlapping filament systems (smoke +
  splats) double the fill cost and fight each other's grade.

## Goals

1. **Low — cell sprites.** One world-sized additive sprite per
   `SHEET`/`FILAMENT`/`NODE` cell with `1+δ ≥ 0.5` inside the sphere
   (nominal ≈ 120–180k), diameter `cell_size · (1.2 … 2.0)` by class,
   alpha `0.006 · (1+δ)^0.5` clamped `≤ 0.05`, colour from the same
   density ramp as the splats but shifted cool (sheets indigo,
   filaments violet-lavender, node cells pale) — rides the glow
   `PointList` (`kind 1`). This is the current "gas veil" idea done
   from the grid instead of the link graph.
2. **Medium/High — raymarch.** The grid uploaded once as a `R8_UNORM`
   3D texture (2 MB); a quarter-res fullscreen pass marches the view
   ray through the sphere (ray–sphere clip), `N` steps (Medium 32, High
   48), emission-only accumulation (no absorption — additive look),
   density → colour via the shared ramp, **fog + slab applied
   in-march** (the `cosmic-depth-window` terms as functions of depth
   along the ray), 4-tap ordered-dither offset per pixel to hide
   banding; written once to a dedicated HDR target; composited
   additively at the resolve (one extra sampled image — the
   `bloom-mip-chain` pass-description list gains a row, so the
   write-once pin covers it).
3. **One ramp.** `COSMIC_DENSITY_RAMP_GLSL` shared by splats, sprites,
   and the raymarch (single source of truth, pinned).
4. **Retire smoke + old veil.** `smoke_puffs`, `SmokePuff`,
   `SmokeVertex`, `SMOKE_*` consts/shaders/pipeline, `glow_point_cloud`,
   and every braid / spine / strand / fray helper that only they used
   — plus their tests and doc paragraphs. The debug cosmic renderer
   ends as: splats + hub sprites + members + veil (sprites or march)
   + bloom.
5. **Shots** at `slab`, `inspector`, `demo` on Low (sprites) and High
   (march), side by side.

## Non-goals

- No absorption / scattering / shadowing (emission-only; the reference
  is additive, report §8).
- No temporal reprojection / TAA for the march (dither + quarter-res
  is enough at 32–48 steps; revisit only if banding survives shots).
- No change to `WebField` beyond reading it; no engine change.
- No raymarch on Low, ever (ADR-025 §4); no runtime tier switching UI
  beyond the existing `--tier` / device profile.
- No sorting of sprites (additive).
- No sheet *geometry* (the roadmap's "sheet-sprite render" is
  satisfied by class-tinted cell sprites + the march; no meshes).

## Users / Stakeholders

- **Player (demo):** flies inside a translucent gas body that thickens
  toward hubs and thins to nothing in voids; on Low the body is
  grainier but present.
- **Developer (inspector):** slab view shows filaments as soft bodies
  with faint walls between them (report §5).
- **Intel UHD 620:** the march target is written once and only read —
  the rule's test case.
- UX consulted: yes — inside-a-filament legibility (the march must not
  fog the HUD-relevant hub; fog floor for hubs already exists, the
  march itself must not saturate the centre of the screen).

## Roles

Author: PO. UX consulted (required if player-facing): yes — near-eye
behaviour inside the gas (no full-screen wash), Low vs High parity
("same web"). ARCHITECT consulted (required if cross-module): yes —
3D texture + a new fullscreen pass in `debug`; composite at the
resolve (touches `bloom-mip-chain`'s pass list); ramp snippet shared
across three shader families.

## Functional requirements

- FR1 (sprites): `veil_sprites(&WebField, origin) -> Vec<(pos, color,
  misc)>` — one record per qualifying cell at the cell centre + a
  deterministic sub-cell offset (hash of the cell index, no RNG stream)
  to break the lattice; diameter by class (sheet 2.0, filament 1.4,
  node 1.2 × cell); alpha `min(0.006·sqrt(1+δ), 0.05)`; colour
  `ramp(log2(1+δ)) · class_tint` (sheet ×`[0.8,0.8,1.0]`, filament ×1,
  node ×`[1.0,0.95,0.9]`). Pure function; count band at nominal
  `[100k, 200k]`.
- FR2 (texture): `R8_UNORM` 3D image `128³`, value = the grid's 6-bit
  log-density (class dropped — the march colours by density only),
  uploaded once per (seed); sampler linear, clamp-to-edge; origin
  offset + cell size in the push block so the march samples in the
  render frame after rebase.
- FR3 (march pass): fullscreen triangle (`RESOLVE_VERT`), target
  `HDR/4` (quarter-res, own image, write-once), push block: inverse
  view-proj, eye, sphere centre/radius, cell size, grid origin, steps,
  fog/slab terms, exposure. Per pixel: ray–sphere intersect → clip to
  `[t0, t1]` → `N` steps with per-pixel dither `frac(dot(px, magic))`
  → `acc += ramp(δ) · a(δ) · vis(t) · dt`; `a(δ) = k · max(0, 1+δ −
  0.5)`; output `acc`. Arithmetic + one 3D texture fetch per step;
  no `exp`, no `pow` in the loop (the ramp is `mix`-based).
- FR4 (composite): resolve samples the march target bilinearly and
  adds it to the scene before ACES (`scene + bloom·i + march·e`).
  `bloom-mip-chain`'s `describe_bloom_chain` gains the march image as
  a read of the resolve and a write of the march pass; the pin covers
  it.
- FR5 (tiers): `VeilMode::for_tier` = Low → `Sprites`, Medium →
  `March { steps: 32 }`, High → `March { steps: 48 }`; env override
  `GAME_DEBUG_COSMIC_VEIL=sprites|march` for A/B on one machine.
  Sprites are **not** drawn when marching (one body, not two).
- FR6 (retire): delete smoke + old veil + braid/spine/strand helpers
  and tests; `cosmic_layout=` prints `veil sprites N` or `veil march
  steps S`; `rendering.md` smoke paragraphs removed.
- FR7 (near-eye): march contribution within `2 · cell_size` of the eye
  ramps to zero (same near-eye fade rule as splats) so the player is
  never inside an opaque wash.

## Non-functional requirements

- NFR1 (budgets, `quality.md`): Low ≤ 200k veil sprites (≤ 7 MB), no
  new pass; Medium/High: one quarter-res pass × 32/48 steps ≈ 16–25 M
  texture fetches per frame at 1080p (≈ 1–2 ms on a 2019 iGPU class,
  measured and recorded); one `2 MB` 3D image + one quarter-res HDR
  image (≈ 2 MB at 1080p R16G16B16A16). `quality.md` cue-raymarch row
  is **replaced** by this feature's row (the v0.2.0 64×64/16-step
  budget described a plan that this supersedes).
- NFR2 (Intel rule): march target written once (its own pass), read
  only at the resolve; pinned via the pass description.
- NFR3 (determinism): sprites are a pure function of the grid (hash
  jitter, no RNG); march is deterministic per frame (fixed dither);
  capture gate passes on both modes.
- NFR4 (shaders): march fragment loop is `mix`/FMA/texture only;
  sprite fragment unchanged (rim-zero).
- NFR5 (invariants): projection un-flipped; the march reconstructs
  rays from the **same** view-proj as the draws (inverse of the pushed
  matrix, NDC top row) — pin: a march of a synthetic single-cell
  volume lands the blob where a splat at the cell centre lands (≤ 1 px
  at quarter res × 4).
- NFR6 (parity): Low vs High shots at `slab` framing: same filaments
  visible (ANALYST overlays), Low grainier; no filament present in one
  and absent in the other.

## Definition of Done

1. High `slab-after-march.png`: filaments read as soft continuous
   bodies with faint walls between them (report §4/§5), thickness
   tracks density (thick near Tier A hubs, hair-thin at void edges),
   voids stay ≤ 3× backdrop. Low `slab-after-sprites.png`: same
   structure, grainier (NFR6 overlay recorded).
2. `demo-after-*.png` both modes: player inside a filament sees a
   translucent body, no full-screen wash (FR7), hub ahead visible.
3. Smoke, old veil, braid/spine/strand code gone (`rg smoke_puffs\|
   glow_point_cloud\|braid_point\|spine_subsegments` = 0); layout line
   prints veil mode.
4. Tests: sprite count band + class diameters + alpha clamp + hash
   jitter determinism; ramp snippet identical across splat/sprite/
   march shaders (pin); march ray-reconstruction pin (NFR5); pass
   description includes the march image and the write-once pin is
   green; `VeilMode::for_tier`; env override parsing.
5. Intel UHD 620 High captures clean (no coloured squares); march
   frame cost measured on the reference iGPU and desktop, recorded;
   Low sprite budget recorded.
6. Docs: `rendering.md` veil section (replaces smoke + old veil
   paragraphs), `quality.md` row replaces the cue-raymarch row,
   techstack version bump, `architecture.md` note if a module is
   added; links resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Depends on `web-field-export` (grid), `cosmic-depth-window`
  (fog/slab terms as functions of depth), `bloom-mip-chain` (pass
  description + resolve composite), `cosmic-tracer-splat` (shared
  ramp). Seventh feature on branch `v0.3.3`.
- Emission constants (`k`, alpha, class tints) are starting values;
  tuned from shots and recorded.
- `engine::render::cue` keeps its CPU `raymarch_web` reference for
  tests; it is not the GPU path (documented).

## Open questions

- Step count on Medium (32) may band at the sphere edge; dither should
  cover it — decided from shots (raise to 40 if not).
- Whether Low should draw sprites for `SHEET` cells at all (walls are
  the subtlest element; cutting them saves ~40 % of sprites). Start
  with sheets on at half alpha; cut first under budget pressure
  (recorded cut order: sheet sprites → alpha floor → count stride).
