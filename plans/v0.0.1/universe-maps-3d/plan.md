# Plan — universe-maps-3d

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Shared 3D map camera + projection pick (debug lib, pure)

New module `crates/debug/src/map_camera.rs`: `MapOrbitCamera`
(target / distance / yaw / pitch as f32, per-instance min/max
distance, scene radius for near/far). Methods: `new`, `rotate`
(0.01 rad/px, drag-up pitches up — `OrbitCamera` convention), clamped
pitch `±(π/2 − 0.02)` so the pole never degenerates, `pan_screen`
(world-per-pixel at target distance, content follows cursor),
`zoom_by` (multiplicative, clamped), `toggle_top_down` (yaw = south
approach `π/2`, pitch = max, remembers previous tilt), `eye`,
`view_matrix` (`look_at_mat4`, up +Y),
`projection_matrix(aspect)` (glam `directx::perspective`, FOV 60°,
near/far from distance + scene radius), `view_proj`,
`world_per_pixel(vp_h)`, `px_scale(vp_h)` for the point shader.

Shared default framing: yaw = `π/2` (camera south of target →
north-up) + pitch ≈ 50° (OQ-2 proposal). Per-map constructors:
`framing_galaxy()` (compressed-ly clamps mirroring the old
500 ly … 1.1×disk range), `framing_system(outer_orbit_au)` (AU
clamps mirroring 0.05 AU … framed system).

New pure helper in `crates/debug/src/picking.rs`:
`project_to_screen(world: Vec3, view_proj: Mat4, vp: Rect) ->
Option<(f32, f32)>` — rejects `w ≤ 0`, NDC→px per the binding
`ndc = (2u−1, 1−2v)` convention.

Unit tests (both headless, CPU-only): zoom/pan/pitch clamps; rotate
directions; top-down toggle sets pitch max + south yaw and restores
tilt; projection equals `directx::perspective` and differs from glam's
Y-flipped `vulkan::perspective` (invariant pin, mirrors the
`OrbitCamera` test); target projects to viewport center; at default
framing a point north of target projects up-screen and a point east
projects right-screen (convention pins); `project_to_screen`
round-trips through cursor→ray math and rejects behind-camera points.

Modules: `game_debug` lib only. Consumes `glam` (already a
dependency). No engine / `game` touch.

Invariants touched: **rendering camera & screen-space conventions**
([`rendering.md`](../../../docs/techstack/rendering.md)) — un-flipped
perspective, NDC +1 = top, `ndc=(2u−1,1−2v)` — pinned by the new
tests; `OrbitCamera` and its tests untouched.

### Phase 2 — Galaxy view port (debug lib)

`crates/debug/src/galaxy_map.rs`: delete `GalaxyCamera` (+ 2D tests);
`GalaxyMapView.camera: MapOrbitCamera` (`framing_galaxy()` on
new/regenerate). Star world helper: `(p[0], p[1], −p[2])` (FR-2
embedding). `NebulaSprite` gains implicit plane height (upload emits
`y = 0`); `BackdropSprite` gains a seeded `y` (drawn from the same
stream after x/z — view-only, deterministic). `select_at` becomes
view-projection pick over all stars (nearest within `PICK_RADIUS_PX`,
lowest-index ties). Ported tests: pick round-trip
(project→cursor→select), behind-camera rejection, regenerates reset
camera/selection/field, sprite seed-stability (incl. new y ranges).

### Phase 3 — System view port (debug lib)

`crates/debug/src/system_map.rs`: delete `SystemCamera`;
`SystemMapView.camera: MapOrbitCamera`
(`framing_system(outer)` on new/load). Presentation orientation
(OQ-1 proposal: ≤ 10° inclination, deterministic ascending node via a
stream-free index hash — `splitmix64(index) → dyadic fractions`):
`planet_slot(orbit_au, index) -> (f64, f64, f64)` world slot with
golden-angle anomaly (kept) + tilt, `|slot| == orbit_radius` exactly;
`orbit_ring_points(orbit_au, index) -> Vec<(f32,f32,f32)>` inclined
ring (same `RING_SEGMENTS` count, closes). `pick_planet` becomes
projection pick over slots; `toggle_focus` sets target = planet slot
and distance = clamped `0.6 × orbit_radius`; `arrival_for` untouched.
Ported tests: tilt determinism + per-index distinctness, radius
exactness, ring closure/count, pick round-trip, focus framing,
load-clears-state.

Invariants touched: **descriptor determinism** — none (no engine
edits; orientations are computed in view functions only, never hashed).

### Phase 4 — GPU + input + UI (debug binary)

`crates/debug/src/main.rs`:
`MapVertex.map_pos` → `[f32; 3]` (`R32G32B32_SFLOAT`); `MAP_VERT`
vec3 position + `px = kind0 ? misc.x : misc.x * pc.px_scale /
gl_Position.w` (clamped 1..256 as today); `MapPush { mvp, px_scale }`
(same 68 B). `upload_map` emits `(x, y, −z)` stars, plane nebulae,
3D backdrop; `upload_system_points`/`upload_system_lines` emit world
positions. Draw sites (`GalaxyMap`/`SystemMap` arms) push
`camera.view_proj` + `camera.px_scale(vp_h)`. Input routing: map
screens left-drag → `rotate`, right/middle-drag → `pan_screen`,
wheel → `zoom_by`, `Home` → `toggle_top_down`, click → new
`select_at(cursor, vp)` (viewport-aware); selection/focus rings and
camera readouts repositioned via `project_to_screen`. `--headless`
self-test stays green (push-size asserts hold).

Modules: debug binary only. No new pipelines, no depth-state changes.

### Phase 5 — Gates + docs + manual pass

Full [`quality.md`](../../../docs/techstack/quality.md) gate list
(cargo test workspace, clippy, fmt, mobile compile-guards,
`--headless`). Docs: `rendering.md` universe-maps paragraph rewrite
+ camera note; `controls.md` map input; `journey.md` L2/L3 annotation;
techstack README version bump; link check on every touched link.
Windowed manual pass per DoD (orbit/zoom/pan/pick/snap both maps;
star→system→planet→orbit travel unchanged).

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| M3D-001 | done | Notion + user-confirmed input/inclination/snap | all |
| M3D-002 | done | `map_camera.rs`: `MapOrbitCamera` + framing helpers + 11 tests | FR-1, FR-2 |
| M3D-003 | done | `picking.rs`: `project_to_screen` + 2 tests | FR-6 |
| M3D-004 | done | `galaxy_map.rs` 3D port + 8 tests | FR-3, FR-6 |
| M3D-005 | done | `system_map.rs` 3D port (tilts, rings, focus) + 9 tests | FR-4, FR-6, FR-7 |
| M3D-006 | done | `main.rs` GPU: vertex/shader/push/uploads/draws | FR-5 |
| M3D-007 | done | `main.rs` input + UI rings/readouts + headless | FR-7, FR-8 |
| M3D-008 | done | Gates green (test/clippy/fmt/mobile guards) | NFR |
| M3D-009 | done | Docs sync + link check | DoD 7 |
| M3D-010 | pending | Windowed manual pass (user): orbit/zoom/pan/pick/snap both maps; travel flow | DoD |

## Role sign-off

Breakdown approved by: ARCHITECT _(executed 2026-09-16 per the
user-approved 3D design)_ · Todos approved by: TECHLEAD _(executed
2026-09-16 — M3D-001…M3D-009 done, M3D-010 windowed pass with the
user)_ · UX acceptance rows (if player-facing): _(user-confirmed 2026-09-16 —
left orbit / right pan, presentation inclinations, Home top-down snap)_ ·
DoD verified by: ANALYST _(pending)_ · Security reviewed by: SECURITY
_(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Galaxy 3D: orbit/pan/zoom, real thickness | done | `map_camera` (11 tests) + `galaxy_map` (8 tests incl. `disk_thickness_separates_on_screen`); `upload_map` emits `position_ly[1]`; headless `pick_selftest=star0 ok` | DEV (tests) |
| 2 | System 3D: inclined rings/planets, no hash drift | done | `system_map` (9 tests incl. `slots_keep_radius_and_sit_on_their_rings`, determinism/bounds); headless `system_hash=29393b57504e021d` unchanged path, `focus_toggle=ok` | DEV (tests) |
| 3 | Click-select parity, headless round-trips | done | `select_at_picks_the_projected_star` (both maps), `select_at_honors_pixel_threshold`, picking round-trip tests, headless project-then-pick asserts | DEV (tests) |
| 4 | Home top-down snap north-up/east-right, toggle | done | `top_down_toggle_snaps_and_restores` (camera) + `top_down_snap_keeps_conventions` (galaxy); `Home` handler + dock hints; windowed pass pending (M3D-010) | DEV (tests) |
| 5 | Rendering invariants pinned, OrbitCamera untouched | done | `projection_uses_unflipped_perspective` (directx ==, vulkan !=), NDC+1-top pins in framing tests; `git status` shows no engine/`game` edits; `OrbitCamera` file untouched | DEV (tests) |
| 6 | Descriptor determinism untouched | done | No `engine`/`game` edits; all pre-existing determinism/hash/journey tests green (`cargo test --workspace --all-targets`: 32+118+19+91 pass) | DEV (gates) |
| 7 | Docs synced, links resolve | done | `rendering.md` universe-maps paragraph, `controls.md` map input, `journey.md` L2/L3, techstack README 0.12.0; all 13 touched links `Test-Path` verified | DEV |
| 8 | Gates green + manual windowed pass | partial | Full `quality.md` list green (fmt/clippy/test/doc/`game` bin/debug+tools headless/android+ios guards); windowed manual pass pending user (M3D-010) | DEV (gates) |

## Acceptance criteria

- `cargo test` (workspace) green; all pre-existing determinism /
  journey / camera tests untouched and passing; new 3D camera/pick
  tests green.
- Windowed: tilt the galaxy and see disk depth; tilt the system and
  see inclined rings; `Home` shows the classic map; pick star →
  `E` → system → pick planet → focus → travel → orbit arrival works.
- `git status` shows no `engine`/`game` crate edits.

## Risks & Next steps

- Risk: top-down snap must read exactly like the old 2D map
  (north-up/east-right) — mitigated by the south-approach yaw with
  convention pins in tests; verified in the manual pass.
- Risk: 25k-star projection pick cost — click-only, ~25k
  `Mat4×Vec4`, negligible; no hover path added.
- Risk: `universe-maps` ANALYST audit overlap — this feature
  supersedes only its view layer; its descriptor/journey evidence
  stays valid (no engine/`game` edits).
- Next: implement M3D-002 → M3D-007, then gates, docs, DoD table.
