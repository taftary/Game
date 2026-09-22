# Notion — cosmic-capture-harness

## Status

`done` (PO sign-off 2026-09-20; ARCHITECT + TECHLEAD breakdown in
`plan.md`; ANALYST audit + SECURITY review recorded in `plan.md` DoD
table; single commit on branch `v0.3.3`)

## Context

v0.3.3 is a visual version: every feature's Definition of Done is "closer
to [`docs/reports/images/target.jpeg`](../../../docs/reports/images/target.jpeg)"
(reading: [`2026-09-20-cosmic-web-visual-description.md`](../../../docs/reports/2026-09-20-cosmic-web-visual-description.md)).
v0.3.2 asked for the same evidence ("A/B shots attached") and never got
it: the only comparison image (`current.png`) was hand-captured once,
went stale within a day, and was deleted. Grading rounds D-1…D-7 in
`cosmic-web-illustris-look` were judged from memory across sessions.

There is no way to render the cosmic web to a file today: `game_debug
--headless` is GPU-free by design (CI-safe), `game_tools` has no
readback, and the windowed viewer has no screenshot key. ADR-025 §3
makes reproducible evidence the first v0.3.3 feature for exactly this
reason.

## Problem & Needs

- **Reproducibility.** Two people (or one person on two days) cannot
  produce the same image of the same build: camera pose, seed, window
  size, tab, and exposure all vary.
- **Evidence trail.** DoD rows need a file path that ANALYST can open,
  not "looked closer on my screen".
- **Regression detection.** Later features (splats, veil, bloom) each
  change the picture; without fixed shots nobody can tell which feature
  moved which pixel.
- **Presets that match the target framing.** The target is an outside,
  near-orthographic slab view; the inspector's default 430 Mpc framing
  and the demo's inside-a-filament spawn are both wrong framings for
  the A/B. The harness must own the framing so the features don't.

## Goals

1. `game_debug --capture <out.png> [--seed N] [--view <preset>] [--size WxH]`
   renders one frame of a cosmic surface offscreen (no window) and
   writes an 8-bit sRGB PNG, then exits 0. Deterministic per (build,
   seed, preset, size) on a given GPU.
2. Named camera presets that the whole version reuses: `inspector`
   (current default framing), `slab` (the target framing: outside,
   narrow FOV, thin slab once `cosmic-depth-window` lands), `demo`
   (player spawn, Chase), `vista` (the `cosmic-vista-intro` opening
   shot). Presets are data (seed-independent camera pose in Mpc), not
   code paths.
3. A repo convention for evidence: `plans/v0.3.3/<feature>/shots/
   <preset>-<before|after>.png` with a one-line caption in the plan's
   DoD table. Shots are committed with the feature's single `done`
   commit.
4. A windowed shortcut (`F12`) that writes the same PNG of the current
   surface with the current camera to `captures/` — for exploration
   only; DoD evidence always comes from presets.

## Non-goals

- No perceptual metric / automatic similarity score against the target
  (analyst-judged A/B is the rule for this version; a metric is a
  follow-up if judgement proves inconsistent).
- No video / sequence capture, no GIFs.
- No CI capture (needs a GPU); the harness is a **local gate**, listed
  in `quality.md` as such.
- No release-binary (`game` crate) change; no touch on `tools`.
- No HDR/EXR output — the resolve output (post-ACES, post-bloom) is
  what the player sees and what gets compared.
- No change to any cosmic render feature — the harness only observes.

## Users / Stakeholders

- **DEV** (every v0.3.3 feature): before/after shots per preset as DoD
  evidence.
- **ANALYST**: opens two files side by side with the target; judges.
- **PO / user**: sees the actual picture in the plan folder without
  running anything.
- UX consulted: n-a for the CLI; **yes** for the `F12` windowed key
  (must not shadow a game key — ADR-022 chrome rule).

## Roles

Author: PO. UX consulted (required if player-facing): yes — `F12` key
binding only (debug shell, exempt otherwise). ARCHITECT consulted
(required if cross-module): yes — new dependency (`png`) in the debug
crate; offscreen render path must reuse the exact windowed frame
recording (no second render path to drift).

## Functional requirements

- FR1 (offscreen render): `--capture` builds the same device, HDR
  chain, pipelines, and buffers as the windowed viewer, records the
  same frame closure into an offscreen swapchain-format image of
  `--size` (default `1408x768`, the target's aspect), then copies the
  image to a host buffer and encodes PNG. No window, no event loop.
- FR2 (presets): `--view inspector|slab|demo|vista`; each preset is a
  `CapturePreset { surface, camera pose, fov, slab (optional), exposure
  overrides: none }` constant in `debug::cosmic_capture`. `slab` and
  `vista` may render identically to `inspector`/`demo` until their
  owning features land — the preset name is reserved on day one so
  shots stay comparable across the version.
- FR3 (determinism): same (build, seed, preset, size) → byte-identical
  PNG on the same GPU/driver (fixed sim time `t = 0`, no animation, no
  auto-exposure drift; all randomness comes from the seed). Test: two
  consecutive captures compare equal (local gate, GPU).
- FR4 (`F12`): in the windowed viewer, `F12` writes
  `captures/<surface>-<seed>-<yyyymmdd-hhmmss>.png` of the current
  frame and logs the path to the Console; never blocks the frame loop
  for more than one frame (readback is issued after present, encoded
  on the next frame).
- FR5 (exit codes + errors): missing HDR support → capture still works
  through the LDR bypass and logs it; no Vulkan device → exit 2 with a
  one-line reason (never a panic); unwritable path → exit 3.
- FR6 (evidence convention): `plans/README.md` gains a "shots/"
  sub-folder rule (allowed under a feature folder, PNG only, ≤ 1 MB
  each, named `<preset>-<before|after>[-<tag>].png`).

## Non-functional requirements

- NFR1 (no drift): offscreen and windowed paths share one frame
  recording function; the only difference is the target image and the
  absence of present. A test pins that `--capture` and the windowed
  path call the same `record_frame` symbol (structural pin, not pixel).
- NFR2 (dependencies): exactly one new crate (`png`, pure Rust, no
  `unsafe` feature flags, pinned minor) in `crates/debug` only;
  workspace `Cargo.toml` declares it; `game` and `engine` do not depend
  on it. SECURITY reviews the dependency (decoder is not used — encode
  only).
- NFR3 (budgets): capture is a dev tool; no frame budget applies. Cold
  start of `--capture` ≤ boot + 2 s at `1408x768` on the reference
  desktop.
- NFR4 (headless gate untouched): `game_debug --headless` stays
  GPU-free and does not import the capture path's Vulkan code (feature
  or module split keeps CI green on GPU-less runners).
- NFR5 (invariants): readback respects the NDC-top-row convention —
  the PNG's row 0 is the top of the picture (a flipped PNG would hide
  a projection bug behind a capture bug).

## Definition of Done

1. `cargo run -p game_debug -- --capture shots/inspector.png --seed
   1337 --view inspector` produces a PNG on the reference desktop
   (Intel UHD 620 + one discrete GPU), top row = top of picture, exit 0.
2. All four presets render; `slab`/`vista` are reserved and documented
   as "identical to inspector/demo until F3/F7".
3. Two consecutive captures with identical arguments are byte-identical
   (local gate script recorded in `plan.md`).
4. `F12` in the windowed viewer writes a PNG and logs its path; the
   Controls list in Settings shows the key; no existing key rebound.
5. `game_debug --headless` still GPU-free (CI green); `game` crate diff
   empty; `png` is the only new dependency and SECURITY signed it.
6. Docs: `quality.md` local-gate row, `plans/README.md` shots rule,
   `rendering.md` capture note, `controls.md` `F12`, techstack version
   bump; every link resolves.
7. Baseline shots of the **v0.3.2 build** for all four presets committed
   under `plans/v0.3.3/cosmic-capture-harness/shots/` as the version's
   "before" reference.

## Constraints & Assumptions

- Lands first in v0.3.3 (every later DoD depends on it). One `done`
  commit on branch `v0.3.3`.
- Assumes the windowed frame path can be driven without a `winit`
  surface (vulkano offscreen image as color attachment). If the
  swapchain-format assumption leaks into pipelines, the fix is to
  parameterize the render pass format, not to fork the frame code.
- Capture size default `1408x768` (target's aspect); presets store
  camera pose in Mpc relative to the home node so they survive reseeds
  meaningfully.

## Open questions

- Whether to commit shots at full `1408x768` or downscale to keep the
  repo small (≤ 1 MB rule proposed; PO leans full size, PNG, no
  downscale — a downscale hides the very details being judged).
- `F12` vs another key: `F12` is free in the current Controls list;
  UX confirms during plan review.
