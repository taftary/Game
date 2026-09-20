# Notion — cosmic-depth-window

## Status

`done` (PO sign-off 2026-09-20; UX consulted; ARCHITECT + TECHLEAD
breakdown in `plan.md`; ANALYST audit + SECURITY review recorded in
`plan.md` DoD table; single commit on branch `v0.3.3`)

## Context

The reference ([`target.jpeg`](../../../docs/reports/images/target.jpeg),
report §2 "Viewpoint", §6 "Voids") is a wide, near-orthographic view of
a **thin slab** of the universe — the standard way simulation renders
are made (a 10–40 Mpc/h deep projection of a much larger box). That is
why its voids are black: nothing sits behind them inside the slab.

Both cosmic surfaces today project the **entire** 500 Mpc descriptor
sphere with a 60° perspective camera (`INSPECTOR_DISTANCE_MPC = 430`,
shared `FOV_Y`; `cosmic_camera.rs` Chase/Orbit/FirstPerson) and almost
no depth attenuation (`dim = 1/(1+0.45z)` with `z ≤ 0.5` → ≥ 0.81 at
250 Mpc). Every void in the picture is filled by filaments 100–400 Mpc
behind it. No amount of grading can darken a void that has structure
behind it — this is the single largest reason the build reads as
"full" while the target reads as "foam". It is also the cheapest fix
in the version.

## Problem & Needs

- **Voids must be dark** (report §6): the negative space is what makes
  the network legible; today the negative space is filled from depth.
- **Depth must read as depth** (report §2 "Depth cues", §8
  "Depth-based attenuation"): far strands dimmer and bluer, near ones
  crisper — a real fog term, not a 20 % dim.
- **The inspector must be able to show the target's framing**: a slab
  of chosen thickness at a chosen depth, viewed from outside with a
  narrow FOV (near-ortho), scrollable through the volume.
- **The demo must not lose its immersion**: the player is inside the
  web; fog must reveal ~2–3 void-cells around the ship (80–120 Mpc)
  and hide the rest, so the local structure is legible and the far
  web fades to the indigo backdrop instead of a wall of light.
- **Perf**: everything outside the window is wasted fill; a slab/fog
  cut is also the overdraw relief the splat and veil features need on
  Low.

## Goals

1. **Visibility fog on every cosmic draw.** Per-vertex factor
   `vis = 1 / (1 + (d/L)²)` (arithmetic-only, no `exp`) with
   per-surface `L` (`COSMIC_DEMO_FOG_MPC ≈ 90`, `COSMIC_MAP_FOG_MPC`
   = ∞ unless slab mode), multiplied into every cosmic vertex shader's
   alpha (splats, smoke while it exists, glow impostors, veil later).
   The existing bounded Hubble tint stays as the **hue** cue; fog is
   the **luminance** cue.
2. **Inspector slab mode.** Toggle `S` on the Cosmic Web tab: a slab of
   thickness `T ∈ [10, 80] Mpc` (default 30) centred at depth `D`
   along the view axis, `D` scrollable with `Shift+wheel` (wheel keeps
   zoom); draws outside `[D − T/2, D + T/2]` get `vis = 0` with a
   soft 5 Mpc edge (smoothstep). The slab is a per-draw push-constant
   pair (`slab_center`, `slab_half`), no CPU culling in this feature.
3. **Near-orthographic inspector.** Slab mode also narrows the
   inspector FOV to `20°` and moves the eye out ×3 so the framing
   (visible width) is unchanged and parallax is small (report §2
   "no vanishing point"). `px_scale`, `pixels_per_unit`, and picking
   all derive from the **instance** FOV so the picking invariant holds.
4. **`slab` capture preset filled.** `cosmic-capture-harness`'s
   reserved `slab` preset becomes: slab mode on, `T = 30`, `D` through
   the home node, FOV 20°, framing width ≈ 400 Mpc — the target's
   composition.
5. **Demo fog default on.** `L = 90 Mpc` demo; a dev-widget slider
   (`F7` Inspector sub-tab) for `L` in `[30, 400]` and slab controls,
   debug-only.

## Non-goals

- No CPU frustum/slab culling of buffers (follow-up if fill is still
  the bottleneck after F6; a push-constant window is enough to prove
  the picture).
- No change to the demo camera modes, spawn, controls, or the
  `cosmic-vista-intro` motion (F7 consumes this feature's slab/fog
  push constants).
- No `exp`-based fog (mobile rule); no depth-buffer-based fog (points
  don't write depth).
- No change to the Hubble tint coefficients (already tuned; hue cue
  stays).
- No engine change.

## Users / Stakeholders

- **Developer / ANALYST (inspector):** can reproduce the target's
  framing exactly and scroll through the volume like a CT scan.
- **Player (demo):** sees the local web crisp and the far web fading;
  the hub ahead reads as "far, but there" instead of one more bright
  smear.
- UX consulted: yes — demo fog radius vs. goal legibility (the hub must
  stay visible at ≥ 150 Mpc: fog floor for hub impostors).

## Roles

Author: PO. UX consulted (required if player-facing): yes — fog must
not hide the navigation goal; hub impostors get a fog **floor** (never
below 0.25) so the target hub stays visible at any distance inside the
sphere. ARCHITECT consulted (required if cross-module): yes — touches
the camera/projection contract (`rendering.md` § Camera & screen-space
conventions): per-instance FOV on `MapOrbitCamera`, picking must keep
using the same projection matrix.

## Functional requirements

- FR1 (fog term): every cosmic vertex shader receives `fog_l` (Mpc,
  `≤ 0` = off) and computes `vis = 1/(1 + (clip.w/fog_l)²)` on
  `clip.w ≥ 0`; alpha `*= vis`. Hub impostors (kind 1 halos and cores)
  use `max(vis, 0.25)`.
- FR2 (slab term): push constants `slab_center`, `slab_half` (Mpc along
  view depth, `slab_half ≤ 0` = off); `vis *= 1 − smoothstep(slab_half
  − 5, slab_half + 5, |clip.w − slab_center|)`. Both terms live in one
  shared GLSL snippet included in every cosmic shader (one source of
  truth, pinned).
- FR3 (inspector controls): `S` toggles slab mode; `Shift+wheel`
  scrolls `D` by `T/4` per notch, clamped to `[−R, +R]` around the
  orbit target; `[`/`]` change `T` by 10 Mpc in `[10, 80]`; HUD
  readout in the inspector dock: `slab 30 Mpc @ −12 Mpc` / `slab off`.
- FR4 (FOV): `MapOrbitCamera` gains `fov_y` per instance (default
  `FOV_Y` — every other consumer unchanged); slab mode sets `20°` and
  `distance *= tan(30°)/tan(10°)` so the framed width is constant;
  toggling back restores both. `px_scale`, `pixels_per_unit`,
  `projection_matrix`, `project_to_screen` all read `self.fov_y`.
- FR5 (demo): `COSMIC_DEMO_FOG_MPC = 90.0` on by default; dev-widget
  slider `[30, 400]`; Console logs changes. No slab in the demo.
- FR6 (presets): `slab` capture preset = slab mode on, `T = 30`,
  `D` = depth of the home node from the preset eye, FOV 20°, framing
  ≈ 400 Mpc wide; `inspector` preset unchanged (slab off, 60°).
- FR7 (backdrop): with fog on, the far field must reach the clear
  colour — the ACES resolve receives the same `COSMIC_BACKDROP`; no
  additive floor sneaks in (test: a fully fogged vertex contributes
  exactly 0).

## Non-functional requirements

- NFR1 (shader cost): fog + slab add ≤ 8 ALU per vertex, 0 per
  fragment; arithmetic-only (`smoothstep`, `/`, `*`).
- NFR2 (invariants): projection stays un-flipped `directx::perspective`
  at any `fov_y`; `ndc = (2u−1, 1−2v)` picking on the inspector uses
  the same matrix as the draw (pin test at 20° and 60°); marker via
  `world_to_pixels`; bloom write-once untouched.
- NFR3 (determinism): no RNG; fog/slab are pure functions of the
  camera; capture determinism (F0 gate) must still pass at the `slab`
  preset.
- NFR4 (no regression elsewhere): Galaxy/System/Planet maps use
  `MapOrbitCamera` with the default `fov_y` — pixel-identical (their
  existing tests unchanged).
- NFR5 (perf): fog/slab are visibility, not culling — no frame-time
  claim here; the overdraw *relief* is measured by the consumers
  (splats: CTS-008 rerun at the `slab` preset, recorded here as a
  before/after number).

## Definition of Done

1. `slab-after.png` at the filled `slab` preset shows ≥ 6 distinct dark
   voids (report §6: "8–12 distinguishable voids") with luminance ≤ 3×
   the backdrop in their interiors; `inspector-after.png` (60°, no
   slab) unchanged in framing.
2. `demo-after.png`: far field fades to the backdrop; the home hub
   impostor remains visible (fog floor) — ANALYST confirms goal
   legibility (UX-1).
3. `S`, `Shift+wheel`, `[`/`]` work; readout shows state; Controls list
   + `controls.md` updated; no existing key rebound.
4. Tests: fog/slab snippet identical across all cosmic shaders (pin);
   `vis` monotone in depth; fully-fogged vertex contributes 0; FOV
   toggle keeps framed width (±1 %); picking round-trip at 20° and
   60°; other map cameras pixel-identical (existing tests).
5. Splat overdraw estimate at the `slab` preset recorded before/after
   (expected ≥ 5× relief).
6. Docs: `rendering.md` camera-contract paragraph (per-instance FOV),
   depth-window paragraph, `controls.md`, `quality.md` note, techstack
   version bump; links resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Depends on `cosmic-capture-harness` (presets) and lands after
  `cosmic-tracer-splat` so the slab shot is judged on the new
  material. Fourth feature on branch `v0.3.3`.
- `L = 90 Mpc` and `T = 30 Mpc` are starting values; tuned from shots,
  recorded as constants.
- The shared GLSL snippet is string-concatenated at compile time (the
  inline-shader convention) — one Rust `const` included by every cosmic
  shader.

## Open questions

- Should the demo also get an optional slab (a "sensor" mode)? Not for
  v0.3.3 — recorded as a `cosmic-vista-intro` consideration (the vista
  uses the slab for the opening shot only).
- Fog floor for hubs `0.25` vs. a distance-based cap: decide from
  `demo` shots (UX-1).
