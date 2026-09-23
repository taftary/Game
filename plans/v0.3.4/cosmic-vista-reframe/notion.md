# Notion — cosmic-vista-reframe

## Status

`done` (PO sign-off 2026-09-23; UX consulted — UX-1/UX-2 judged off
the reframed captures + projection/dive pins, limitation L-1 in
`plan.md` (no interactive session drivable; static-shot review only);
ARCHITECT consulted — camera-contract reuse, ADR-026 §5; DEV
CVR-001..010 done 2026-09-23; ANALYST audit + SECURITY review
recorded in `plan.md` DoD table; single commit on branch `v0.3.4`;
version closes with this feature — merge `v0.3.4 → main`)

## Context

`cosmic-vista-intro` (v0.3.3) frames the **whole sphere from
outside** (25° FOV, 40 Mpc slab, eye docked 120 Mpc beyond the rim
on the Chase axis). Consequences on `vista-after.png` and
`slab-after-*.png`: the sphere limb is in frame as a disc, the
composition is "an object on black" rather than the reference's
"endless network with no centre and no edge" (report §2 viewpoint,
§9 flow), and the chosen Tier A hub sits wherever the home node
happens to be — often near the rim.

The camera contract already supports instance FOV and an external
pose (`CosmicCamera`, `cosmic-vista-intro` Phase 1); the depth window
supports slab thickness and fog; `cosmic-void-contrast` fades the rim.
What is missing is a **composition rule**: frame an interior window
whose frustum footprint at the slab lies inside the sphere
cross-section, centred on a hub chosen for composition, not
proximity.

## Problem & Needs

- **Headline shot:** the version's DoD match against the reference is
  judged on `vista-after.png`; a disc-on-black can never match a
  frame-filling web.
- **Player intro:** the first 2 s should show the web as a place
  ("I am somewhere inside this"), not as a ball ("I am looking at
  that").
- **Evidence presets:** `slab` and `vista` must never include the
  limb, or every later grading round fights the edge.

## Goals

1. **Interior window rule.** For the `vista` and `slab` presets and
   the boot vista: with slab thickness `T` and a target footprint
   `W × H` Mpc at the slab centre plane, choose eye distance `d` and
   FOV such that the footprint's half-diagonal plus `T/2` stays inside
   the sphere cross-section at the slab centre: `sqrt((W/2)² + (H/2)²)
   ≤ sqrt(R² − c²) − margin`, with `c` the slab centre's distance from
   the sphere centre. Starting values: `W × H = 300 × 170` Mpc,
   `T = 40`, `margin = 15`, FOV 25° (⇒ `d ≈ 383` Mpc from the slab
   centre — outside the sphere, as today).
2. **Hub choice by composition.** The vista hub is the Tier A node
   (top 1 %) maximizing `rank_score − λ·|c|/R` among hubs whose
   footprint satisfies Goal 1 (prefer bright *and* central); the
   footprint centre is offset so the hub sits at the report's focal
   position (slightly right of centre, ≈ 0.55 × width) — deterministic
   per seed.
3. **Dive continuity preserved.** Hold → Dive → Chase stays one
   motion; the dive path re-docks from the new eye; angular-velocity
   bound (≤ 30°/s) and skip / replay rules unchanged.
4. **No limb in frame** on `vista` and `slab`: corner-region scan
   shows continuous field to the frame edge (no radial luminance
   step; F4's rim fade is the safety net, not the mechanism).
5. **Headline match.** `vista-after.png` is compared against the
   reference on the report's six readings (§2 table): background,
   palette, luminance tiers, depth cue, viewpoint, focal hierarchy —
   each marked met / partial / unmet by ANALYST with a one-line
   reason; the version closes with ≥ 5 met.

## Non-goals

- No change to Chase / Follow / FirstPerson camera modes, controls,
  picking, or the marker path.
- No change to slab thickness semantics or fog math (F3 of v0.3.3).
- No new preset (the four names stay); `inspector` and `demo` are
  untouched.
- No engine change.

## Users / Stakeholders

- Player: boot shows a place, then dives into it.
- ANALYST / PO: one reproducible headline shot per seed for every
  future grading round.
- UX consulted: yes — UX-1 (place, not object), UX-2 (hub at the
  focal position, dive still readable).

## Roles

Author: PO. UX consulted (required if player-facing): yes. ARCHITECT
consulted (required if cross-module): yes — reuses the
`cosmic-vista-intro` camera contract; the composition rule is a pure
function of `(WebDescriptor, constants)`.

## Functional requirements

- FR1 (rule): `interior_window(R, c, W, H, T, margin) -> Option<(d,
  fov)>` pure; `None` when the footprint cannot fit (then fall back
  to the smallest fitting `W` keeping the 16:9 ratio, recorded).
- FR2 (hub choice): `vista_hub(&web) -> node_index` per Goal 2 with
  `λ = 0.5` (starting value); fallback chain from v0.3.3 kept for
  degenerate seeds.
- FR3 (pose): `vista_pose(&web)` uses FR1/FR2; the focal offset
  places the hub at `(0.55, 0.5)` of the frame; the look axis is
  perpendicular to the slab plane; slab centre `c` chosen on the
  segment sphere-centre → hub so `|c| ≤ 0.4·R`.
- FR4 (dive): the eye Bézier docks from the new pose; existing
  `tick_vista` bounds and tests hold; `VISTA_DOCK_MPC` re-tuned if
  needed (recorded).
- FR5 (presets): `CapturePreset::VISTA` and `::SLAB` take their pose
  from FR3 (slab additionally sets the inspector slab mode as today).
- FR6 (readings): a `docs/reports/` addendum records the six
  readings for `vista-after.png` with met / partial / unmet.

## Non-functional requirements

- NFR1 (determinism): pose is a pure function of `(seed)`; capture
  byte-identical per build + seed.
- NFR2 (invariants): un-flipped projection at 25°; marker via
  `world_to_pixels`; picking disabled during the vista, unchanged
  after.
- NFR3 (bounds): angular velocity ≤ 30°/s nominal; per-tick delta
  bound test unchanged.

## Definition of Done

1. `vista-after.png`, `slab-after.png`: corner scans show no limb
   (no radial step > 20 % within 10 px of any edge) — numbers recorded.
2. `vista-after.png`: focal hub at `(0.55 ± 0.05, 0.5 ± 0.05)` of the
   frame (projection test on the nominal seed); six-reading table in
   the report addendum with ≥ 5 met.
3. Dive: headless `vista=done`, Done == Chase pose (1e-6), angular
   velocity ≤ 30°/s nominal, skip continuity — all existing tests
   green with the new pose.
4. Tests: `interior_window` fit / `None` / fallback; `vista_hub`
   determinism + Tier A + footprint fit; focal projection.
5. Docs: `rendering.md` camera-contract note (interior window),
   `journey.md` boot sentence, report addendum, techstack bump; links
   resolve.
6. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit; version close (merge `v0.3.4 → main`).

## Constraints & Assumptions

- Last feature of the version; depends on every earlier one (the
  headline shot is judged on the finished materials).
- Assumes a Tier A hub with `|c| ≤ 0.4·R` exists on the nominal seed
  set (≈ 60 Tier A nodes in a bounded sphere — near-certain; the
  fallback chain covers the rest).

## Open questions

- Should the boot vista *also* be the interior window (yes by this
  notion) or keep the whole-sphere reveal as a 1 s prelude? PO
  decision: interior only — the sphere is a generation bound, not a
  place the player should see as an object.
