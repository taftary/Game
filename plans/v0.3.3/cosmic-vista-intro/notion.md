# Notion — cosmic-vista-intro

## Status

`planned` (PO sign-off 2026-09-20; UX consulted; ARCHITECT + TECHLEAD
breakdown in `plan.md`). Player-facing; the last feature of v0.3.3 —
it depends on the whole render stack being in place.

## Context

The Game Demo boots with the player **inside** a filament 30 Mpc from
the home node, Chase camera 8 Mpc behind, 60° FOV
(`cosmic_player::spawn`, `cosmic_camera.rs`). That is the right place
to *play* from (ADR-023: the player navigates the cosmic scale), but
it is the wrong place to *see the web* from: the reference
([`target.jpeg`](../../../docs/reports/images/target.jpeg)) is an
outside, near-orthographic slab view — the composition the whole
version is graded against — and the player never sees it.

PO decision (2026-09-20, plan-mode Q2): **both surfaces**, with a vista
intro — the demo opens on the reference composition, holds it, then
dives to the player's spawn so the immersive view inherits the
picture the player just saw. `cosmic-depth-window` provides fog/slab
push terms; `cosmic-capture-harness` reserved the `vista` preset;
`cosmic-hub-hierarchy` provides a Tier A hub to frame.

## Problem & Needs

- **First impression** (`cosmic-scale-player` notion: "goal legible in
  3 s"): a player who spawns inside gas sees gas; a player who has just
  seen the web from outside knows what the gas *is* and where the
  bright hub *is* — the dive gives the inside view its meaning.
- **The graded shot must be reachable in-game**: otherwise the version
  optimises a picture nobody sees.
- **Continuity** (the spec §9.3 waypoint-experience principle): the
  transition from vista to Chase must be one continuous camera motion
  (no cut), with fog/slab easing from the slab view to the immersive
  values, so the eye never loses the hub.
- **Skippable and re-playable**: never a forced cutscene; any input
  skips to Chase; a key replays the vista.

## Goals

1. **Vista pose.** Camera outside the sphere-slab: eye at `≈ 180 Mpc`
   from the nearest Tier A hub to the home node, looking at that hub,
   with the home node inside the frame; FOV `25°`; slab `T = 40 Mpc`
   centred on the hub's depth; fog off (slab does the windowing).
   Framed width `≈ 400 Mpc` (the target's composition, report §2).
2. **Hold, then dive.** Hold `2.0 s` (player reads the composition),
   then `6.0 s` continuous ease (`smoothstep` on `t`) from the vista
   pose to the Chase pose behind the player marker: FOV `25° → 60°`,
   slab widens `40 → off` (half-thickness to `≥ R`), fog `off → L_demo`
   (90 Mpc), eye travels the straight path in Mpc with the look target
   sliding from the hub to the marker. Total `8 s`, then normal Chase.
3. **Skip / replay.** Any steering or thrust input, `Esc`, or click
   skips (a `0.6 s` fast ease to Chase, never a cut); `V` replays the
   vista from the current position (the same 8 s path from the vista
   pose). Reseed (`R`) restarts the vista.
4. **Marker + HUD.** The player marker (`YOU` dot + heading arrow)
   stays visible throughout via the pinned `world_to_pixels` path (it
   is tiny at 180 Mpc — a `≥ 6 px` floor keeps it findable); the
   fly-to pill and Console are unchanged; a small `press any key`
   hint fades in after `1 s` of hold (debug shell text, not release UI
   — the release-binary UI question stays open in spec §10).
5. **`vista` capture preset filled** with the `t = 0` vista pose, so
   the reference composition is a reproducible shot.

## Non-goals

- No cinematic camera *system* (splines, multi-shot sequences, letter
  boxing, music); one pose → one ease.
- No change to spawn position, Chase parameters, controls, or fly-to.
- No slab in normal play (the slab is an intro device; it eases off).
- No release-binary UI (the hint is debug-shell text under the
  ADR-022 chrome rules).
- No engine change; no descriptor read beyond the Tier A pick.

## Users / Stakeholders

- **Player:** sees the web, then becomes part of it; knows where the
  hub is before controls are live.
- **PO / ANALYST:** the graded composition is the boot shot.
- UX consulted: yes — hold duration, skip affordance, hint copy,
  marker visibility at distance, motion sickness bound (peak angular
  velocity of the look direction ≤ 30°/s during the dive).

## Roles

Author: PO. UX consulted (required if player-facing): yes — durations,
skip rules, hint, marker floor, angular-velocity bound (UX-1…UX-4 in
`plan.md`). ARCHITECT consulted (required if cross-module): yes —
touches the camera contract (FOV over time on `CosmicCamera`), the
marker path (`world_to_pixels` only), and the depth-window push terms;
no new pass or pipeline.

## Functional requirements

- FR1 (state): `VistaState { phase: Hold | Dive | Done, t: f64,
  from: Pose, to: Pose }` on `CosmicDemoState`; `Pose { eye: DVec3,
  target: DVec3, fov: f32, slab_half: f32, slab_center: f32, fog_l:
  f32 }`; `CosmicCamera` gains a `fov_y` override (default `FOV_Y`)
  and an `external_pose` used while the vista is active; `projection_
  matrix`/`px_scale` read the instance FOV (the `cosmic-depth-window`
  precedent on `MapOrbitCamera`).
- FR2 (vista pose): `hub = nearest Tier A node to home` (fallback: the
  most massive node within 150 Mpc, then the most massive overall);
  view direction = the direction from the hub toward the *home node's
  side* projected perpendicular to the hub→home vector's largest
  component (so home is in frame, not behind the hub), eye at `180
  Mpc`; FOV `25°`; slab `40 Mpc` at the hub's depth; fog off.
- FR3 (dive): `t ∈ [0, 6]`, `s = smoothstep(t/6)` (`engine::handoff::
  smoothstep` precedent); eye, target `lerp`; FOV `lerp(25°, 60°)`;
  `slab_half = lerp(20, R+50)` then off at `s = 1`; `fog_l = lerp(∞→
  clamp, 90)` implemented as `1/fog_l` lerp from `0` to `1/90` (linear
  in the shader term). Peak look-direction angular velocity ≤ 30°/s
  (test on the nominal seed; if exceeded, the eye path bows away from
  the hub — recorded).
- FR4 (skip/replay): inputs that skip: any `HeldThrust` flag, steer
  drag ≥ 4 px, `Esc`, mouse click, `E`; skip = `0.6 s` ease from the
  current interpolated pose to Chase (`t` remapped, same easing).
  `V` = replay from the vista pose (Hold `2 s` → Dive `6 s`); `R`
  (reseed) restarts. While active, `P` (camera cycle) and wheel are
  ignored (no mode fight); after `Done`, all controls as v0.3.2.
- FR5 (marker + hint): marker via `world_to_pixels` with a `6 px`
  minimum dot during the vista; hint text `press any key` (debug-shell
  font) fades in at `1 s`, out on skip/dive start; Console logs
  `vista: hold`, `vista: dive`, `vista: skipped`, `vista: done`.
- FR6 (preset): `CapturePreset::VISTA` = the FR2 pose at `t = 0`;
  `demo` preset unchanged (Chase at spawn, i.e. `Done`).
- FR7 (headless): `game_debug --headless` steps the vista to `Done`
  (8 s of ticks) and asserts the final pose equals the Chase pose
  within `1e-6` Mpc / `1e-6` rad (continuity pin).

## Non-functional requirements

- NFR1 (continuity): no frame in which the camera pose jumps by more
  than the per-frame ease step (test: max per-tick eye delta ≤ 1.1 ×
  the analytic derivative bound).
- NFR2 (invariants): projection un-flipped at any FOV; marker only via
  `world_to_pixels`; picking (node selection) disabled during the
  vista (`select_node_at` returns `None` while active — otherwise a
  click both skips and selects); bloom write-once untouched.
- NFR3 (determinism): the vista is a pure function of `(seed, t)`; the
  `vista` capture is byte-identical across runs (F0 gate).
- NFR4 (budgets): zero new GPU resources; CPU per tick negligible.
- NFR5 (controls doc): `V` and the skip rule in Settings › Controls +
  `controls.md`; no existing binding changes.

## Definition of Done

1. `vista-after.png` (the filled `vista` preset) is judged by ANALYST as
   the version's closest match to `target.jpeg` (report §2 composition:
   outside view, one dominant hub, dark voids, filaments radiating);
   this shot is the version's headline evidence.
2. Boot: hold `2 s` → dive `6 s` → Chase at spawn; continuity pin
   (FR7) green; peak angular velocity ≤ 30°/s recorded.
3. Skip on any listed input (`0.6 s` ease), `V` replays, `R` restarts;
   `P`/wheel ignored while active; selection disabled while active;
   Controls + `controls.md` updated; no key rebound.
4. Marker visible throughout (`≥ 6 px`), hint fades in/out, Console
   logs the four events.
5. Tests: pose selection fallback chain; ease monotone + endpoints;
   per-tick delta bound; angular velocity bound; skip remap continuity;
   headless `Done` pin; `vista` preset equals `t = 0` pose.
6. Docs: `rendering.md` camera-contract note (`CosmicCamera` instance
   FOV + external pose), `journey.md` boot sequence sentence,
   `controls.md`, techstack version bump; links resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit — which also closes v0.3.3 (branch merge to
   `main` per `plans/README.md` § 7).

## Constraints & Assumptions

- Depends on every other v0.3.3 feature (the vista shows the finished
  material; the slab/fog terms come from `cosmic-depth-window`; the
  framed hub from `cosmic-hub-hierarchy`). Eighth and last feature on
  branch `v0.3.3`.
- Durations (`2 s`, `6 s`, `0.6 s`), distance (`180 Mpc`), FOV (`25°`),
  slab (`40 Mpc`) are UX starting values; tuned from shots + a
  hands-on session, recorded as constants.
- The `press any key` hint is debug-shell text; the release-binary UI
  decision remains an open spec §10 item (unchanged by this feature).

## Open questions

- Should the vista be shown on **every** boot or only the first boot
  per seed (autosave flag)? v0.3.3: every boot, skippable — the demo is
  a showcase; a "seen" flag is a `game`-crate persistence question for
  a later version.
- Whether the dive should end in Chase or FirstPerson: Chase (marker
  visible, consistent with v0.3.2 boot).
