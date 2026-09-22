# Plan — cosmic-vista-intro

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_demo.rs` (`VistaState`,
skip/replay, selection gate), `cosmic_camera.rs` (instance FOV +
external pose), `main.rs` (push terms from the interpolated pose,
hint text, `V` key, Console events), `cosmic_capture.rs` (fill
`vista`), `app.rs` (boot starts the vista). Reads `WebDescriptor`
(Tier A pick via the `cosmic-hub-hierarchy` tiering) — no engine
change, no new GPU resource. **Invariant rows:** un-flipped projection
at any FOV; marker via `world_to_pixels`; picking disabled while active
(and unchanged after); bloom write-once untouched; determinism per
`(seed, t)`.

### Phase 1 — Camera contract (`cosmic_camera.rs`)

`fov_y` override + `external_pose: Option<(eye, target)>`;
`projection_matrix`, `px_scale`, `eye()`, `target()` honour them;
tests: default path byte-identical to today (existing camera tests
untouched), 25° projection stays `directx::perspective`.

### Phase 2 — Vista state machine (`cosmic_demo.rs`)

`Pose`, `VistaState`, `vista_pose(&web)` with the fallback chain,
`tick_vista(dt)` (Hold → Dive → Done), `skip()` remap, `replay()`,
selection gate; ease via `engine::handoff::smoothstep`. Tests: pose
fallback; endpoints; monotone; per-tick delta bound; angular velocity
≤ 30°/s on the nominal seed; skip continuity; headless `Done` pin.

### Phase 3 — Wiring (`main.rs`, `app.rs`)

`cosmic_frame()` takes fog/slab/FOV from the interpolated pose while
active; boot → `replay()`; inputs → `skip()`; `V` → `replay()`; `R` →
restart; `P`/wheel ignored while active; marker `6 px` floor; hint
text fade; Console events; Controls list + `controls.md`.

### Phase 4 — Preset, shots, UX session, gates

Fill `CapturePreset::VISTA`; `vista-after.png` + `demo-after.png`;
hands-on UX pass on durations (recorded constants); docs; gates;
audit; single `done` commit; version close (merge `v0.3.3 → main`).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CVI-001 | done | `CosmicCamera` instance `fov_y` + `external_pose`; derived quantities honour them; default path identical (existing tests untouched); 25° projection pin | FR1, NFR2 |
| CVI-002 | done | `Pose`, `VistaState`, `vista_pose(&web)` (Tier A nearest home → fallbacks); tests for the fallback chain and home-in-frame (NDC, nominal) | Goals §1, FR2 |
| CVI-003 | done | `tick_vista` Hold `2 s` → Dive `8 s` (`handoff::smoothstep`), FOV/slab/fog lerps (`1/fog_l` linear), eye Bézier docked (`VISTA_DOCK_MPC` 120), target front-loaded eased (0.625); tests: endpoints, monotone, per-tick analytic bound, angular velocity 26.5°/s nominal | Goals §2, FR3, NFR1 |
| CVI-004 | done | `skip()` (idempotent, `0.6 s` remap, continuity test), `replay()`, reseed restart; thrust auto-skip in `tick`; selection gate; `P`/wheel ignored while active | Goals §3, FR4, NFR2 |
| CVI-005 | done | Wiring: boot starts vista; `cosmic_frame()` pushes pose terms; `V` key + Controls row; marker `6 px` floor via `world_to_pixels` (shared path); hint fade (`vista_hint_alpha` pin); Console events | Goals §4, FR5 |
| CVI-006 | done | Controls list (`VistaReplay` registry row) + `controls.md` (`V`, skip rule); `V` uniqueness pin (no rebinding) | NFR5, DoD 3 |
| CVI-007 | done | Headless: step to `Done` (600 ticks), assert pose == Chase pose (`1e-6`); log `vista=done` | FR7, DoD 2 |
| CVI-008 | done | `CapturePreset::VISTA` filled (25°, slab 40) + `pose_demo_camera_for_vista_capture` (= `t = 0` pin); capture determinism byte-identical; `vista-after.png` + `demo-after.png` + timed `hint`/`dive` shots | Goals §5, FR6, NFR3, DoD 1 |
| CVI-009 | done | Hands-on UX session: 6° home offset (19° heuristic out of frame; exact alignment whipped 560°/s; 10° traded edge sibling for corner glare); dock 120 + 8 s dive (sweep table in code history); peak 26.5°/s recorded | UX-1…UX-4 |
| CVI-010 | done | Docs: `rendering.md` camera-contract note, `journey.md` boot sentence, `controls.md` vista paragraph, techstack 0.46.0; link check | DoD 6 |
| CVI-011 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit; milestones row `done`; merge `v0.3.3 → main` | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — camera contract
extended the same way `cosmic-depth-window` extends `MapOrbitCamera`;
no new pass; marker/picking paths pinned; vista is a pure function of
`(seed, t)`) · Todos approved by: TECHLEAD (2026-09-20 — risk-first:
camera contract (CVI-001) and the state machine with its bounds
(CVI-003) before any wiring; the UX session is a todo with recorded
outputs, not an open loop) · UX acceptance rows: approved 2026-09-20 —
UX-1…UX-4 below (UX-2 revised 2026-09-22, see row) · DoD verified by:
ANALYST (2026-09-22 — per-row audit below) · Security reviewed by:
SECURITY (2026-09-22 — no new input surface beyond `V` + guarded
existing inputs, no dependency/unsafe; see review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `vista-after.png` = version headline match to target | done (outside hub-centered Tier-A view; dark voids; filament grain + mist; closest match among version shots — dominance partial: rank-22 hub among siblings, edge siblings noted; slab-after/inspector/demo compared) | `shots/vista-after.png` + `shots/vista-after-sprites.png` vs `target.jpeg` | ANALYST 2026-09-22 |
| 2 | Hold → dive → Chase; continuity pin; angular velocity recorded | done (2 s hold + 8 s dive; headless `vista=done events=hold,dive,done`; Done == Chase pose 1e-6; peak 26.5°/s nominal) | headless log + `real_dive_turns_slowly_on_nominal_seed` | ANALYST 2026-09-22 |
| 3 | Skip/replay/restart; gates while active; controls documented | done (thrust auto-skip + steer≥4px + Esc + click + E → 0.6 s ease; `V` replay; `R` restart; `P`/wheel ignored; selection gated; `VistaReplay` Controls row; `V` uniqueness pin) | `vista_skip_is_fast_and_live`, `vista_replay_mid_dive_targets_the_ship`, `vista_disables_selection`, `vista_key_is_never_rebound` | ANALYST 2026-09-22 |
| 4 | Marker visible; hint; Console events | done (home projects in-frame at t = 0/1.5/4 nominal → shared 6 px dot path; `vista_hint_alpha` pin (0 → 0.5 → 1); `vista: hold/dive/skipped/done` drained to Console; timed 3D captures at t = 0/1.5/4 — capture harness is 3D-only so UI pixels are covered by projection pin + shared draw path, recorded) | `nominal_marker_projects_through_hold_and_early_dive`, `hint_alpha_tracks_the_hold_clock`, `shots/vista-after-hint.png`, `shots/vista-after-dive.png` | ANALYST 2026-09-22 |
| 5 | Tests listed green | done | camera (10) + vista lib (10) + demo (15) + capture preset pins + `vista_capture_pose_matches_t0_pose`; full workspace suite green | ANALYST 2026-09-22 |
| 6 | Docs + links | done | `rendering.md` camera note, `journey.md` boot sentence, `controls.md` vista paragraph, techstack 0.46.0; link check green | ANALYST 2026-09-22 |
| 7 | Gates + audit + review + one commit + merge | done | fmt/clippy/build/workspace-tests/doc-tests/`game`/headless/tools-smoke/mobile guards green; this audit + SECURITY note; single commit + merge | ANALYST + SECURITY 2026-09-22 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `CosmicCamera::projection_matrix` is `directx::perspective` at
  25° and 60° (RH, Z ∈ [0,1], no Y-flip) [T].
- A-2. Marker drawn only via `world_to_pixels` (+ `6 px` floor) [T:
  existing marker pin + floor test].
- A-3. `select_node_at` returns `None` while the vista is active; after
  `Done` the existing pick tests pass unchanged [T].
- A-4. Vista pose + interpolation are pure functions of `(seed, t)`
  [T: replay test + capture gate].
- A-5. No new GPU resource, pass, or pipeline; bloom write-once
  untouched.
- A-6. No engine / `game` / `tools` diff.

UX acceptance rows:

- UX-1. Boot: within the first 2 s the player sees the whole web from
  outside with one obvious bright hub and dark voids (the reference
  composition); the `press any key` hint appears after 1 s.
- UX-2. The dive is one continuous motion (no cut, no snap); the hub
  stays in frame through the hold and the opening third of the dive
  (measured exit s ≈ 0.41 nominal — it drifts out as the look commits
  to the marker for the dolly-in); look direction never turns faster
  than 30°/s (measured 26.5°/s nominal). REVISED 2026-09-22 (was:
  "hub never leaves the frame until Chase"): geometrically infeasible
  — the look must swing 107° hub→marker on the nominal seed while the
  frame spans 25–44°, so the hub MUST cross the frame edge mid-dive;
  duration only sets the rate, not the exit. The bound (safety) wins
  over the framing (aesthetic); the exit is drift, never snap.
- UX-3. Any input skips smoothly (≤ 0.6 s) to Chase; controls respond
  immediately after; `V` replays; nothing else about the demo
  controls changed.
- UX-4. The marker is findable at every moment of the vista (≥ 6 px
  dot), so the player knows where "they" are before controls are live.

## Risks & Next steps

- R-1 (hub choice): the nearest Tier A hub may be far (> 150 Mpc) from
  home on some seeds, making the dive long/fast; the fallback chain
  and the angular-velocity test catch it. INVOKED 2026-09-22 in spirit:
  the 6 s dive whipped 35°/s on the nominal seed, so the dive is 8 s
  (total intro 10 s) — durations were tunable starting values.
- R-1b (dive geometry, found 2026-09-22): the straight eye lerp flies
  past the hub it stares at (103°/s); a Bézier bow made it worse
  (longer/faster path, 146°/s). Shipped: eye Bézier docked along the
  Chase look axis (`VISTA_DOCK_MPC` = 120) + front-loaded eased target
  (done by s = 0.625) + 8 s dive → 26.5°/s nominal. The 30°/s bound is
  scoped to the full dive; the 0.6 s user-initiated skip traverses the
  same path 10× faster by design (never a cut).
- R-2 (slab edge visible during the dive): the slab widens as FOV
  opens; if a hard edge shows mid-dive, widen the smoothstep edge
  from 5 to 10 Mpc during the dive (constant in the window snippet).
- R-3 (hint in the release UI question): the hint is debug-shell text;
  if a release build ever ships the demo, the open spec §10 UI item
  decides — recorded, not resolved here.
- Next (post-v0.3.3): "seen" flag persistence (`game` crate), LOD /
  culling hardening from the version's measured costs, quad hub
  impostors if the 256 px clamp ever shows.
