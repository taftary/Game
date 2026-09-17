# Plan — frame-hierarchy

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Pure frame kernel (`engine::frames`)

Owning module: new `engine::frames` (pure math over `glam` `DVec3`/`DQuat`,
headless-testable, no GPU — same shape as `render::camera`). No new
dependencies (`glam` already provides `f64` types). No `unsafe`, no I/O,
no input surfaces.

- `FrameId` enum: the 7 spec §3 frames, root → leaf. `Planetocentric`
  and `LocalEnu` carry an opaque `BodyId(u64)` (placeholder until
  `star-catalog-streaming` defines canonical body ids — recorded in
  Risks).
- Per-frame unit system (`meters_per_unit`, `unit_name`) + nominal
  `extent_meters` per the spec §5 non-dimensionalization examples.
- `FrameLink` (rotation + origin-in-parent-units) with `to_parent` /
  `from_parent`; conversion API exists **only** active ⇄ parent —
  there is deliberately no global-flatten function (ADR-012).
- `FrameChain`: active frame + local state (position, orientation) +
  parent links; `step_to_parent`, diagnostic `resolve_root`,
  `commit_transition` (adjacent frames only; anything else is
  `TransitionError::NotAdjacent`).
- Invariant row: `LocalEnu` preserves `east × north == up`
  (rendering invariants, `docs/techstack/rendering.md`).

### Phase 2 — Floating origin in the render path

- `frames::recenter(world: DVec3, camera: DVec3) -> Vec3`:
  camera-relative `f32` for GPU upload (ADR-013).
- `OrbitCamera` gains an `anchor: DVec3` (default `ZERO` = current
  behavior, all existing tests keep passing); `view_matrix()` is
  computed from recentered eye/target.
- Invariant row: projection stays `directx::perspective` (RH,
  Z ∈ [0, 1], no Y-flip); `FrontFace::CounterClockwise` untouched.
  `projection_uses_vulkan_ndc` test must keep passing unmodified.

### Phase 3 — Explicit transition commit

- `commit_transition` converts the state vector and returns a
  `FrameTransition` event (`from`, `to`, `reason`, `sim_time_s`).
- No `engine::save` module exists yet, so the event struct is the
  ADR-004 autosave / ADR-015 time-state hook: `autosave-persistence`
  (v0.3.0) consumes it. Recorded as accepted gap, not scope creep.

### Phase 4 — Docs + debug-consumable API

- Waypoint (spec §1, 10) ⇄ journey-level (`docs/game/journey.md`, 8)
  ⇄ frame mapping table in `docs/game/journey.md`.
- Display accessors (`FrameId::name`, `unit_name`, `depth`;
  `FrameChain::active`, `position`): the surface `scale-debug-screens`
  (v0.3.0) reads from.
- ADR-012 / ADR-013 `draft` → binding; milestones table → `done`;
  `docs/techstack/README.md` version bump.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| FRM-001 | done | `FrameId` + `BodyId` + per-frame unit systems (`meters_per_unit`, `unit_name`, `extent_meters`) | Functional requirements |
| FRM-002 | done | `FrameLink` + `FrameChain`, active ⇄ parent conversion only (no global flatten) | Goals 1–2, Functional requirements |
| FRM-003 | done | Round-trip precision tests at all 6 frame boundaries (≤ 1e-9 relative) | DoD 1 |
| FRM-004 | done | Solar-subtree adjacent walk test (< 1 mm; PO-amended, see Risks) | DoD 1, NFR |
| FRM-005 | done | `recenter` helper + extent test + mm-resolution-near-camera test | DoD 2 |
| FRM-006 | done | `OrbitCamera` anchor; view matrix via recentered coords; projection test unmodified | DoD 2, Constraints |
| FRM-007 | done | `commit_transition` + `FrameTransition` event (ADR-004/015 hook) | Functional requirements |
| FRM-008 | done | Waypoint ⇄ journey-level ⇄ frame mapping in `docs/game/journey.md` | DoD 3 |
| FRM-009 | done | Display accessors (`name`, `depth`, `active`) for `scale-debug-screens` | DoD 4 |
| FRM-010 | done | ADR-012/013 → binding; milestones table; version bump; full gate list green | NFR, lifecycle |

Gate commands for DEV (`docs/techstack/quality.md`):
`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --workspace`, `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`,
`cargo run -p game_tools -- --headless --tier low`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — phases map to
`engine::frames` + `render::camera`; dependency direction legal (pure
math only); invariants listed explicitly; foundations before
surfaces)_ · Todos approved by: TECHLEAD _(signed 2026-09-17 — todos
risk-first (precision tests before render wiring), each independently
verifiable with a named test; no budget risk (no draw calls, no tick
cost — pure math); gate list linked above)_ · UX acceptance rows:
_(n-a — no player-facing surface; consult recorded in notion.md
`Roles`)_ · DoD verified by: ANALYST _(signed 2026-09-17 — every row re-checked,
gate suite re-run green on this tree: fmt, clippy `-D warnings`,
workspace build, workspace tests 32+118+18+103, doc tests 6+19,
`game` / `game_debug --headless` / `game_tools --headless --tier low`
runs; E2E = scripted journey in `game` binary output; save round-trip
n-a (no I/O touched); cosmic-limit finding resolved as PO amendment,
not a defect)_ · Security reviewed by: SECURITY _(signed 2026-09-17 —
no new input surface, dependency, `unsafe`, serialization, or file I/O;
pure-math module + camera anchor defaulting to legacy behavior; no
findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Frame tree + chain type implemented with unit tests at every frame boundary (round-trip precision evidence) | done | `cargo test -p game_engine frames::` — 11 passed: `every_boundary_round_trips_within_tolerance` (6 boundaries ≤ 1e-9), `solar_subtree_walk_preserves_millimeters` (< 1 mm, PO-amended) | ANALYST _(signed 2026-09-17)_ |
| 2 | Floating-origin upload active; no world-space `f32` position exceeds one frame's extent | done | `recentered_upload_stays_within_frame_extent`, `recenter_preserves_millimeter_detail_near_camera`, `render::camera::anchored_view_matches_world_view`; `projection_uses_vulkan_ndc` passes unmodified | ANALYST _(signed 2026-09-17)_ |
| 3 | Waypoint ⇄ journey-level mapping documented in `docs/game/journey.md` or linked doc | done | `docs/game/journey.md` § *Waypoint ⇄ level ⇄ frame mapping* (W1–W10 → L1–L8 → frames) | ANALYST _(signed 2026-09-17)_ |
| 4 | `scale-debug-screens` can display active frame + chain from this API | done | `display_accessors_cover_debug_screens` + public `name`/`unit_name`/`depth`/`body`/`active` accessors with doc tests | ANALYST _(signed 2026-09-17)_ |
| S | SECURITY review (no new input surface / dep / `unsafe`) | done | No new surfaces, deps, `unsafe`, serialization, or I/O — pure math + default-`ZERO` anchor | SECURITY _(signed 2026-09-17)_ |

## Acceptance criteria

- `cargo test -p game_engine frames::` green: 6 boundary round-trips
  (≤ 1e-9), solar-subtree walk (< 1 mm), recenter extent + resolution.
- `render::camera` existing tests pass unmodified (anchor defaults to
  `ZERO`).
- Projection invariant: `projection_uses_vulkan_ndc` passes unmodified.
- Doc tests for all new public `engine::frames` APIs (`quality.md`
  test policy).
- ENU: `LocalEnu` rotation convention documented as `east × north == up`.

## Risks & Next steps

- Cosmic precision limit (DEV finding, PO-amended 2026-09-17): `f64`
  absolute coordinates at Mpc scale resolve ~2600 km (error budget:
  eps × 0.39 Mpc ≈ 8.6e5 m per top-level subtraction; measured 5.2e6 m
  end-to-end on a literal cosmological → facility float resolve). No
  implementation recovers information destroyed there — hence the amended
  NFR: subtree round trip + boundary conversions + SOI re-anchoring.
- `BodyId(u64)` is a placeholder; canonical body ids land with
  `star-catalog-streaming` (v0.2.0). Migration = newtype swap, no
  chain-logic change.
- No `engine::save` yet: the autosave hook is an event struct, wired
  in `autosave-persistence` (v0.3.0). `FrameTransition` shape is frozen
  by this feature to protect that consumer.
- Next features in version order: `scale-physics` (independent),
  `soi-handoff` + `time-compression` (consume the transition/occupancy
  API), `free-flight-navigation` (consumes ship state in active frame).
