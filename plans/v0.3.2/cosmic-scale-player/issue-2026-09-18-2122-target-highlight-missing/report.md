# Issue report — cosmic-scale-player / issue-2026-09-18-2122-target-highlight-missing (UTC)

Specs: [`specs.md`](specs.md)

## Status

`in-review`

## Roles

Investigated by (role: DEV): codebase exploration + projection-path
verification · Fix direction reviewed by TECHLEAD: approved (overlay-only,
no budget risk) · ARCHITECT review (if invariant touch): n-a — no camera,
projection, or picking invariant is touched; the fix reuses the pinned
`world_to_pixels` / `project_to_screen` paths verbatim.

## Root cause

The picking side is complete and tested; the rendering side never
consumes the selection:

- Game Demo: `select_node_at` (`cosmic_demo.rs:231–263`) sets
  `player.target_node` and queues `TargetSelected`. The `draw_main`
  demo branch (`main.rs:5952–6046`) uploads links + node point cloud
  with no per-node selection input, and `build_demo_ui` /
  `compose_overlay_ui` never project `target_node` — so the picked node
  is indistinguishable from every other point sprite.
- Cosmic Web tab: `CosmicWebInspector::select_at`
  (`cosmic_web.rs:63–89`) sets `selected`; `build_cosmic_web_ui`
  renders the SELECTION readout from it but projects nothing — unlike
  `build_galaxy_ui` / `build_system_ui`, which both draw a 4-rect ring
  around their `selected` projection.

Net: spec promised "highlight + target line"; only the target line
(HUD text, live solely during a fly-to leg) shipped.

## Evidence

- `rg target_node crates/debug/src/main.rs` → zero hits: the bin's
  overlay code never reads the demo selection.
- `rg "inspector\.selected|selected" build_cosmic_web_ui` → readout
  only (`main.rs:1344`), vs galaxy ring at `main.rs:1937–1978`.
- Click dispatch (`main.rs:4655–1679`): inspector miss leaves `selected`
  untouched (sticky); demo miss clears `target_node` — both behaviors
  are compatible with a ring (sticky ring on inspector, transient on
  demo).

## Alternatives considered

- **In-shader point tint** (recolor the picked sprite in the map
  pipeline, planet-cell precedent): rejected — requires a per-frame
  push-constant + shader branch for a single point, and the demo's
  upload-origin rebase makes index-stable tinting fragile.
- **3D line-loop geometry around the node**: rejected — new pipeline
  state for a 2D affordance; the map tabs already proved the UI-pass
  ring reads clearly.
- **UI-pass 4-rect ring (chosen)**: zero pipeline changes, same code
  path as the existing selection rings, auto-hides off-screen via the
  existing projectors, GPU-free and unit-testable (lib-side builders +
  `world_to_pixels` / `project_to_screen` are pure).

## Fix direction

1. New `draw_target_ring` helper in `main.rs` (the 4-rect ring geometry,
   amber `C_WARN`, r = 9 px — PO/UX choice).
2. Game Demo: in `compose_overlay_ui`'s `Screen::GameDemo` block,
   resolve `target_node` → `recenter(node.position_mpc,
   upload_origin)` → `world_to_pixels(main_vp, …, vp)` → ring.
3. Cosmic Web tab: at the end of `build_cosmic_web_ui`, resolve
   `inspector.selected` → `project_to_screen` through
   `inspector.view_proj` at `DVec3::ZERO` origin → clamp to viewport →
   ring.
4. Tests: helper geometry unit test + two headless overlay tests
   (deterministic via `DebugApp::new()` fixed seed).
5. Docs: `docs/game/controls.md` click-target line mentions the ring.
