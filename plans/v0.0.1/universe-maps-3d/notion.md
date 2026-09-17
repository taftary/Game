# Notion — universe-maps-3d

## Status

`in-review` (plan: [`plan.md`](plan.md) — all M3D todos implemented,
full gate list green incl. both mobile compile-guards; needs ANALYST
DoD audit + SECURITY review, plus a manual windowed run for the
camera-feel half of DoD 4/8)

## Context

[`universe-maps`](../universe-maps/notion.md) (`in-review`) shipped
working Galaxy and System maps in `game_debug`, but both are 2D:
bespoke f64 ortho cameras (`GalaxyCamera` in
`crates/debug/src/galaxy_map.rs`, `SystemCamera` in
`crates/debug/src/system_map.rs`), a `vec2` point-sprite pipeline, and
2D inverse-affine click picking. The galaxy **data** is already 3D —
`StarDescriptor::position_ly[1]` carries rim-tapered disk thickness
(±1200 ly at the core, `crates/engine/src/universe/generate.rs`) but
every render/pick path drops it at upload (`upload_map`,
`crates/debug/src/main.rs`). The system data is planar by design
(`orbit_radius_au` scalar; anomaly is presentation per
[`journey.md`](../../../docs/game/journey.md) L3).

This feature ports both view layers to 3D perspective. No descriptor,
journey-machine, transit, or engine change — view code only.

## Problem & Needs

Player need: a galaxy disk you can tilt and orbit reads as a galaxy;
a system you can tilt reads as orbits in space. Top-down-only maps
flatten the universe the milestones below (descent, surface) will
spend the rest of v1 inside.

Developer need: a reusable 3D map camera + projection-pick primitive
in the debug lib, ready for later milestones' map needs, without
touching descriptor determinism or the release binary.

## Goals

1. Galaxy map in 3D perspective: orbit / pan / zoom with clamps;
   real disk thickness from `position_ly[1]` visible when tilted.
2. System map in 3D perspective: inclined orbit rings + planets
   (presentation-only tilts, deterministic per orbit index, never in
   descriptors/hashes).
3. Click-select parity with the 2D maps: same 8/10 px contract,
   lowest-index ties, headless-testable projection pick.
4. Top-down snap (`Home`) reproducing the classic 2D framing (north
   up, east right); toggle restores the previous tilt.
5. Rendering invariants intact; descriptor determinism untouched
   (existing hash tests green by construction).

## Non-goals

- Descriptor / generation / journey / transit changes of any kind
  (inclinations stay presentation-only).
- Depth-tested map rendering (draw order keeps reading correctly).
- Star size/dimming attenuation with distance.
- Touch/gamepad map input (M6 input pass).
- Release-binary shell changes (debug-hosted only, per the `ux.md`
  debug-screen exemption).

## Users / Stakeholders

- Player (v1, debug-hosted): orbit maps with the mouse, same travel
  flow as before (click select → E → system → planet → orbit).
- Developer (later milestones): stable 3D map camera + projection
  pick to reuse; same descriptors as before.

## Roles

Author: PO. UX consulted (required if player-facing): yes — input
scheme (left orbit / right pan), inclination scope, top-down snap all
confirmed by the user 2026-09-16.
ARCHITECT consulted (required if cross-module): yes — debug lib +
debug binary view layer only; engine and `game` untouched.

## Functional requirements

- FR-1 `MapOrbitCamera` (new pure debug-lib module): target /
  distance / yaw / pitch, per-instance clamps, `rotate` (0.01 rad/px,
  drag-up pitches up — the `OrbitCamera` convention), view-plane pan
  (content follows cursor), multiplicative zoom, top-down toggle with
  remembered tilt, perspective via glam `directx::perspective`
  (un-flipped — the binding convention).
- FR-2 World embedding `world = (east, up, −north)`: the engine
  ENU/player frame (y-up, north = −z), chosen so a south-side camera
  above the plane shows north-up + east-right like the old 2D maps.
- FR-3 Galaxy view: stars at `(p[0], p[1], −p[2])`; nebulae as plane
  haze `(x, 0, −z)`; L1 backdrop with a seeded third coordinate
  (view-only, appended after the existing stream draws).
- FR-4 System view: golden-angle anomaly (kept) + presentation-only
  inclination (≤ 10°) and ascending node per orbit index via a
  stream-free index hash; rings are inclined 3D world circles;
  `|slot| == orbit_radius` to f64 rounding (rigid rotation).
- FR-5 Point pipeline: `MapVertex.map_pos` becomes `vec3`;
  world-sized sprites scale by `px_scale / clip.w`; push layout stays
  68 B (`pplx → px_scale`); line pipeline unchanged (already `vec3`).
- FR-6 Picking: pure `project_to_screen(world, view_proj, vp)` in
  `picking.rs` (rejects `w ≤ 0`); nearest projected point within the
  existing 8/10 px; lowest-index ties.
- FR-7 Input: left-drag orbit, right/middle-drag pan, wheel zoom,
  click (≤ 4 px travel) select, `Home` top-down toggle; `F` focus
  becomes target = planet slot + distance clamp; `E/Q/T/R`/F-keys
  unchanged.
- FR-8 Selection/focus UI rings reposition via `project_to_screen`;
  hidden when the target is behind the camera.

## Non-functional requirements

- Determinism: zero descriptor/generation edits; presentation math is
  stream-free and index-driven (no rng streams, no wall-clock).
- Precision: f64 map logic (slots); f32 uploads and cameras
  (journey rule — rendering stays f32 camera-relative).
- Perf: same one-static-buffer draws; per-click projection loop is
  < 1 ms (25k `Mat4 × Vec4` on click only).
- Gates: full [`quality.md`](../../../docs/techstack/quality.md) gate
  list green, including the mobile compile-guards.

## Definition of Done

- [ ] Galaxy map renders in 3D perspective: orbit / pan / zoom with
  clamps; disk thickness visible when tilted, from real descriptor data.
- [ ] System map renders in 3D: inclined rings + planets,
  deterministic per orbit index, no descriptor or hash drift.
- [ ] Click-select parity: star/planet picks within 8/10 px via
  view-projection, lowest-index ties, headless-tested round-trips.
- [ ] `Home` top-down snap reproduces the classic framing (north up,
  east right); toggling restores the previous tilt.
- [ ] Rendering invariants intact: un-flipped perspective pinned
  against glam's Y-flipped constructor; NDC +1 = top;
  `OrbitCamera` untouched and its tests green.
- [ ] Descriptor determinism untouched: all existing
  determinism/hash tests green, no `engine` or `game` edits.
- [ ] Docs synced: [`rendering.md`](../../../docs/techstack/rendering.md)
  universe-maps paragraph + camera note,
  [`controls.md`](../../../docs/game/controls.md) map input,
  [`journey.md`](../../../docs/game/journey.md) L2/L3 annotation,
  techstack README version bump; every touched link resolves.
- [ ] Full quality gate list green incl. mobile compile-guards and
  the `--headless` self-test; windowed manual pass (orbit/zoom/pan/
  pick/snap on both maps; star→system→planet→orbit travel unchanged).

## Constraints & Assumptions

- Scale contract ([`journey.md`](../../../docs/game/journey.md)): galaxy
  compressed-ly, system AU — per-instance camera clamps mirror the
  old ortho ranges (galaxy effective half-extent 500 ly … 1.1× disk
  radius; system 0.05 AU … framed system).
- Binding: [`rendering.md`](../../../docs/techstack/rendering.md)
  camera & screen-space conventions; points/lines stay
  no-depth-draw-order (no new pipeline variants).
- Module boundaries ([`architecture.md`](../../../docs/techstack/architecture.md)):
  `game` bin stays clean-headless; no `vulkano` in `game`; engine
  untouched; debug screens stay in `crates/debug`.
- Parent feature [`universe-maps`](../universe-maps/notion.md) is
  `in-review`: its files are not edited here — cross-links only. Its
  2D camera/pick tests are superseded by the 3D equivalents in this
  feature (the view layer it shipped is replaced, not extended).

## Open questions

- OQ-1: presentation inclination ceiling — plan proposes **10°**
  with deterministic ascending nodes; TECHLEAD approves at plan
  breakdown.
- OQ-2: default opening tilt for each map — plan proposes ~50°
  pitch from the south side (3D read + map conventions); confirmed
  implicitly by the windowed manual pass (DoD).
