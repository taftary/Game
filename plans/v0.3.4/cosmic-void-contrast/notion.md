# Notion — cosmic-void-contrast

## Status

`planned` (PO sign-off 2026-09-23; UX consulted; ARCHITECT consulted —
shader-only, ADR-026 §4)

## Context

Every v0.3.3 shot has a **uniform indigo haze** where the reference
has near-black voids. Causes visible in code:

- The shared ramp (`COSMIC_DENSITY_RAMP_GLSL`, `cosmic_veil.rs`) maps
  the lowest stop (`log2od = −2`) to `(0.10, 0.08, 0.35)` — every
  splat, sprite and march step in a void still adds indigo, and
  additive blending sums thousands of them into a floor.
- Brightness rises with density only through `emissive_scale`
  (`1 + 0.5·max(0, log2od − 1.5)`), so a filament at `1+δ ≈ 4` is
  barely brighter than the field at `1+δ ≈ 1`.
- The sphere ends as a hard limb (ray–sphere cut in `MARCH_FRAG`,
  hard tracer cut) — a disc against black, which no reference render
  shows.

The reference (report §2, §6, §8) is: deep indigo **backdrop** (the
clear colour), additive light only where matter is, voids that hold
a stray dot or two, and a network legible *because* the negative
space is dark.

## Problem & Needs

- **Legibility:** the web is read from its voids; without dark cells
  there is no web, only texture.
- **Grading tool:** v0.3.3 graded by moving five colour stops; the
  version needs a **transfer function** with a floor, a contrast band
  and a rim fade — three numbers ANALYST can measure.
- **Rim:** the field must end softly so no preset can show a disc
  edge even before `cosmic-vista-reframe` moves the edge out of frame.

## Goals

1. **One shared transfer snippet** (`COSMIC_TRANSFER_GLSL`) used by
   the splat vertex shader, veil sprites and `MARCH_FRAG`:
   `w = pow-free contrast(max(0, log2od − floor))` × `rim(r)` with
   `floor = 0` (mean density: below the mean emits nothing), a
   filament band `[0, 3]` shaped by a `smoothstep`-based curve (no
   `exp` / `pow`), and `rim(r) = 1 − smoothstep(R − 30, R, r)`.
2. **Colour stays on the existing ramp**, brightness moves to the
   transfer: colour × `w` × tier gain; hubs unaffected (they are glow
   sprites, `cosmic-hub-compact-cores` owns them).
3. **Backdrop from the clear colour**: the HDR clear is the report's
   deep indigo (`≈ (0.02, 0.02, 0.06)` linear); voids measure within
   1.15× of it at the `slab` framing.
4. **Contrast target:** filament ridge luminance ≥ 6× the void floor
   at `slab`; the faintest visible sheet ≥ 1.5× the floor (walls stay
   visible, report §5).
5. **Rim fade** on every cosmic draw (splats, sprites, march) over
   the last 30 Mpc; the `inspector` preset shows no limb (corner scan:
   no radial luminance step > 20 % within 10 px).

## Non-goals

- No hub / member changes (F5), no camera / preset changes (F6).
- No new pass, texture, or pipeline; no bloom-chain change.
- No engine change; no ramp-stop colour change beyond what the floor
  requires (stop at `−2` becomes irrelevant, kept for the veil's
  sheet colour).

## Users / Stakeholders

- Player: black voids make the network and the goal hub legible from
  inside the web.
- Developer: three named constants (`TRANSFER_FLOOR`,
  `TRANSFER_BAND_HI`, `RIM_FADE_MPC`) with scan numbers next to them.
- UX consulted: yes — UX-1 (voids read as cells), UX-2 (demo inside a
  filament is not a wash).

## Roles

Author: PO. UX consulted (required if player-facing): yes. ARCHITECT
consulted (required if cross-module): yes — shared snippet pinned
across three shaders (same mechanism as `COSMIC_DENSITY_RAMP_GLSL`);
no boundary crossing.

## Functional requirements

- FR1 (snippet): `COSMIC_TRANSFER_GLSL` in `cosmic_veil.rs` beside
  the ramp; pasted verbatim into `SPLAT_VERT`, the sprite path and
  `MARCH_FRAG`; identity pin test.
- FR2 (math): `smoothstep` / `mix` / `clamp` / FMA only; CPU mirror
  `transfer(log2od, r) -> f32` for tests.
- FR3 (constants): `TRANSFER_FLOOR = 0.0`, `TRANSFER_BAND_HI = 3.0`,
  `RIM_FADE_MPC = 30.0`, per-tier gain — starting values, graded from
  shots, recorded with scan numbers.
- FR4 (clear): HDR clear colour = deep indigo constant; the resolve
  does not add a backdrop term.
- FR5 (march): the march accumulates `transfer(...)`-weighted
  emission; the vis-weighted normalization from CGV-015 stays.
- FR6 (rim): rim weight multiplies every cosmic emission; the
  ray–sphere cut in `MARCH_FRAG` remains (bounds the march) but the
  last 30 Mpc fade to zero before it.

## Non-functional requirements

- NFR1 (cost): zero new fetches; ≤ 6 ALU ops added per splat vertex /
  march step.
- NFR2 (invariants): fragment arithmetic-only; write-once untouched;
  no projection / picking / marker change.
- NFR3 (determinism): captures byte-identical per build + seed.

## Definition of Done

1. `slab-after.png`: void-floor scan ≤ 1.15× backdrop; ridge / floor
   ≥ 6; sheet / floor ≥ 1.5 — numbers recorded in the plan.
2. `inspector-after.png`: no limb step (radial scan across the
   former edge: no > 20 % step within 10 px).
3. `demo-after.png` inside a filament: UX-2 hot-pixel fraction ≤ 50 %
   (the CGV-015 measure), voids visible as dark cells around the ship.
4. Tests: snippet identity ×3, CPU `transfer` mirror bands (below
   floor → 0; rim → 0 at `R`; monotone in the band), shader safety
   pins extended.
5. Docs: `rendering.md` grading paragraph (transfer replaces
   emissive_scale), `quality.md` constants row, techstack bump; links
   resolve.
6. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Fourth feature; depends on `cosmic-gpu-tracers` (the splat vertex
  shader it edits is v2). Shots are taken with `k` at tier default.
- Assumes the existing exposure / tone map (`exposure-tone-mapping`)
  is not retuned; if the darker field drops the auto-exposure target,
  the cosmic exposure clamp is the one knob (recorded).

## Open questions

- Should the void floor be *exactly* the clear colour (no matter
  below the mean emits) or keep a whisper (`0.02×`) for the "not
  perfectly empty" reading (report §6)? Start at zero, grade, record.
