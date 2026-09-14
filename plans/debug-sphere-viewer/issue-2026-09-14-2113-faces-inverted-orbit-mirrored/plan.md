# Issue plan — debug-sphere-viewer / issue-2026-09-14-2113-faces-inverted-orbit-mirrored (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

1. `crates/debug/src/main.rs`:
   - `build_fill_pipeline`: `front_face: FrontFace::Clockwise` in the
     `RasterizationState` (import `FrontFace`), with a comment pinning
     the Y-down-projection ⇒ mirrored-framebuffer-winding reasoning.
   - `FILL_VERT`: `v_normal = normal;` — drop the inward-normal hack
     and replace its comment with the real explanation.
2. `crates/engine/src/render/camera.rs`:
   - `OrbitCamera::rotate`: `pitch -= dy · 0.01` (drag up ⇒ pitch up,
     FPS-style non-inverted); doc comment states the convention.
   - `pitch_clamps_away_from_poles` test updated to the new sign.

No mesh builder or API changes.

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260914-001 | [x] | `FrontFace::Clockwise` in fill pipeline rasterization | Suspected area |
| ISS-20260914-002 | [x] | Revert `v_normal = -normal` hack in `FILL_VERT` | Logs / Evidence |
| ISS-20260914-003 | [x] | `cargo test` + `quality.md` gates green | Scope & Impact |
| ISS-20260914-004 | [x] | Windowed run: near-side faces render (user-confirmed) | Reproduction steps |
| ISS-20260914-005 | [x] | `OrbitCamera::rotate` pitch sign flip + test update | Observed vs Expected |
| ISS-20260914-006 | [x] | Windowed run: drag up orbits toward the top (user-confirmed) | Reproduction steps |
| ISS-20260914-007 | [x] | Same `FrontFace::Clockwise` fix in `game_tools` smoke (user-approved) | Scope & Impact |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Near-side outward faces visible (no inside-out read) | [x] | Windowed run, user-confirmed fixed |
| 2 | Vertical drag orbits in the drag direction (drag up ⇒ top) | [x] | `OrbitCamera::rotate` sign flip; user-confirmed after fix |
| 3 | No mesh/API/gate impact | [x] | `cargo test`, clippy, fmt green; mesh hash8 `9f087a31` unchanged |

## Acceptance criteria

- `cargo run -p game_debug`: convex outside view at N=4 default;
  near-side pentagons tinted; wireframe aligned.
- Drag up/down orbits the camera in the drag direction (up ⇒ toward
  the sphere's top); drag left/right unchanged; zoom unchanged.
- `cargo run -p game_debug -- --headless` stats line unchanged
  (`hash8=9f087a31`).
