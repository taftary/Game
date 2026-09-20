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
| CVI-001 | pending | `CosmicCamera` instance `fov_y` + `external_pose`; derived quantities honour them; default path identical (existing tests untouched); 25° projection pin | FR1, NFR2 |
| CVI-002 | pending | `Pose`, `VistaState`, `vista_pose(&web)` (Tier A nearest home → fallbacks); tests for the fallback chain and "home in frame" | Goals §1, FR2 |
| CVI-003 | pending | `tick_vista` Hold `2 s` → Dive `6 s` (`smoothstep`), FOV/slab/fog lerps (`1/fog_l` linear); tests: endpoints, monotone, per-tick delta bound, angular velocity ≤ 30°/s | Goals §2, FR3, NFR1 |
| CVI-004 | pending | `skip()` (`0.6 s` remap, continuity test), `replay()`, reseed restart; selection gate while active; `P`/wheel ignored while active | Goals §3, FR4, NFR2 |
| CVI-005 | pending | Wiring: boot starts vista; `cosmic_frame()` pushes pose terms; `V` key; marker `6 px` floor via `world_to_pixels`; hint fade; Console events | Goals §4, FR5 |
| CVI-006 | pending | Controls list + `controls.md` (`V`, skip rule); assert no rebinding | NFR5, DoD 3 |
| CVI-007 | pending | Headless: step to `Done`, assert pose == Chase pose (`1e-6`); log `vista=done` | FR7, DoD 2 |
| CVI-008 | pending | Fill `CapturePreset::VISTA` (= `t = 0` pose); capture determinism gate; `vista-after.png` + `demo-after.png` | Goals §5, FR6, NFR3, DoD 1 |
| CVI-009 | pending | Hands-on UX session: durations, distance, FOV, slab; record final constants + peak angular velocity | UX-1…UX-4 |
| CVI-010 | pending | Docs: `rendering.md` camera-contract note, `journey.md` boot sentence, techstack version bump; link check | DoD 6 |
| CVI-011 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit; milestones row `done`; merge `v0.3.3 → main` | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — camera contract
extended the same way `cosmic-depth-window` extends `MapOrbitCamera`;
no new pass; marker/picking paths pinned; vista is a pure function of
`(seed, t)`) · Todos approved by: TECHLEAD (2026-09-20 — risk-first:
camera contract (CVI-001) and the state machine with its bounds
(CVI-003) before any wiring; the UX session is a todo with recorded
outputs, not an open loop) · UX acceptance rows: approved 2026-09-20 —
UX-1…UX-4 below · DoD verified by: ANALYST _(pending)_ · Security
reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `vista-after.png` = version headline match to target | pending | shot + ANALYST note vs report §2 | ANALYST _(pending)_ |
| 2 | Hold → dive → Chase; continuity pin; angular velocity recorded | pending | headless log + numbers | ANALYST _(pending)_ |
| 3 | Skip/replay/restart; gates while active; controls documented | pending | tests + Controls screenshot | ANALYST _(pending)_ |
| 4 | Marker visible; hint; Console events | pending | shots at `t = 0.5 s`, `1.5 s`, `4 s` + log | ANALYST _(pending)_ |
| 5 | Tests listed green | pending | test names | ANALYST _(pending)_ |
| 6 | Docs + links | pending | file list | ANALYST _(pending)_ |
| 7 | Gates + audit + review + one commit + merge | pending | gate log, commit, merge hash | ANALYST + SECURITY _(pending)_ |

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
  never leaves the frame until the Chase pose is reached; look
  direction never turns faster than 30°/s.
- UX-3. Any input skips smoothly (≤ 0.6 s) to Chase; controls respond
  immediately after; `V` replays; nothing else about the demo
  controls changed.
- UX-4. The marker is findable at every moment of the vista (≥ 6 px
  dot), so the player knows where "they" are before controls are live.

## Risks & Next steps

- R-1 (hub choice): the nearest Tier A hub may be far (> 150 Mpc) from
  home on some seeds, making the dive long/fast; the fallback chain
  and the angular-velocity test catch it; if still too fast, extend the
  dive to 8 s (constant).
- R-2 (slab edge visible during the dive): the slab widens as FOV
  opens; if a hard edge shows mid-dive, widen the smoothstep edge
  from 5 to 10 Mpc during the dive (constant in the window snippet).
- R-3 (hint in the release UI question): the hint is debug-shell text;
  if a release build ever ships the demo, the open spec §10 UI item
  decides — recorded, not resolved here.
- Next (post-v0.3.3): "seen" flag persistence (`game` crate), LOD /
  culling hardening from the version's measured costs, quad hub
  impostors if the 256 px clamp ever shows.
