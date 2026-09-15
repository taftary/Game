# Controls, audio, UI, accessibility (v1 minimums)

## Input

Unified action map (`engine::input`), not per-device logic in gameplay:

- Touch: virtual joystick + tap-to-move/order, pinch zoom in maps/orbit, two-finger orbit on surface.
- Mouse/keyboard: WASD + click orders, wheel zoom, Esc menu.
- Gamepad: left stick move, right stick camera, face buttons confirm/cancel/orders.
- All actions rebindable on desktop; touch layout has large targets (≥44pt).
- Future XR poses map to the same actions; no v1 XR code.

## Surface walk (player-sphere-movement, implemented in `game`)

- Move: WASD / arrows / left stick → tangent-plane intent (W = north,
  D = east); diagonal input is speed-capped, never faster than full tilt.
- Camera: one key/stick-button cycles Follow → First-person →
  Third-person → Global orbit → Follow (`CameraMode::cycle`); wheel/pinch
  zooms the orbit modes.
- Chunks: the flat map recenters on the player every tick; the player
  hemisphere loads immediately, the trail behind unloads after a grace
  delay — same hemisphere rule as the debug flat view.
- Headless demo: `cargo run -p game` runs a scripted 20 Hz walk through
  all four modes (interactive `winit` binding deferred).
- Debug viewer (`cargo run -p game_debug`, Sphere Viewer screen):
  `U` toggles player mode, `WASD`/arrows walk the player on the sphere,
  `P` cycles Follow → First-person → Third-person, `V` cycles main/thumb
  focus, `G`/`T`/`B`/`R` snap the global camera to
  Perspective/Top/Bottom/Right (same as the panel VIEW buttons).

## Audio / UI

- Audio: ambient pad + UI + hazard stingers; full music/sfx pass is stretch. Mix buses with mute; no audio-codec crash on any target.
- UI: touch-first, readable at phone distance, color-blind-safe resource colors, scalable text (100–150%). Text rasterization via `fontdue`; shaders via `naga` SPIR-V (see [`../techstack/stack.md`](../techstack/stack.md)).
- Localization-ready strings (all UI via keys), but v1 ships English only.

## Accessibility minimums

Remappable desktop controls, hold-to-repeat alternatives, no info conveyed by
color alone, photosensitivity-safe transitions (no hard strobes on descent fades).
