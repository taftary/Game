# Controls, audio, UI, accessibility (v1 minimums)

## Input

Unified action map (`engine::input`), not per-device logic in gameplay:

- Touch: virtual joystick + tap-to-move/order, pinch zoom in maps/orbit, two-finger orbit on surface.
- Mouse/keyboard: WASD + click orders, wheel zoom, Esc menu.
- Gamepad: left stick move, right stick camera, face buttons confirm/cancel/orders.
- All actions rebindable on desktop; touch layout has large targets (≥44pt).
- Future XR poses map to the same actions; no v1 XR code.

## Surface walk (player-sphere-movement, implemented in `game`)

- Move: WASD / arrows / left stick → heading-relative intent (W/S =
  thrust forward/backward along the stored heading, A/D = turn the
  heading left/right at `TURN_RATE`); the heading persists while idle.
  Thrust without turn traces a great circle (parallel-transported
  heading), never a lat/lon axis walk.
- Camera: one key/stick-button cycles Follow → First-person →
  Third-person → Global orbit → Follow (`CameraMode::cycle`); wheel/pinch
  zooms the orbit modes. First/Third-person track the stored heading,
  so stopping no longer snaps the view to north.
- Chunks: the desired set is the player's own hemisphere, refreshed on
  a 100 ms throttle; the trail behind unloads after a grace delay.
  The flat map viewpoint stays fixed while the player walks inside its
  hemisphere and re-anchors on the player only near the rim (~70°),
  so marker movement is continuous and never interrupted.
- Headless demo: `cargo run -p game` runs a scripted 20 Hz walk
  (straight leg, curving leg, turn-in-place leg, idle) through all four
  modes, logging lon/lat/heading (interactive `winit` binding deferred).
- Debug viewer (`cargo run -p game_debug`, Sphere Viewer screen):
  `U` toggles player mode, `WASD`/arrows drive thrust/turn on the
  sphere, `P` cycles Follow → First-person → Third-person, `V` cycles
  main/thumb focus, `G`/`T`/`B`/`R` snap the global camera to
  Perspective/Top/Bottom/Right (same as the panel VIEW buttons).
  The player marker is a center dot plus a heading arrow (both views);
  the PLAYER panel row shows `cam / loaded / heading`.

## Audio / UI

- Audio: ambient pad + UI + hazard stingers; full music/sfx pass is stretch. Mix buses with mute; no audio-codec crash on any target.
- UI: touch-first, readable at phone distance, color-blind-safe resource colors, scalable text (100–150%). Text rasterization via `fontdue`; shaders via `naga` SPIR-V (see [`../techstack/stack.md`](../techstack/stack.md)).
- Localization-ready strings (all UI via keys), but v1 ships English only.

## Accessibility minimums

Remappable desktop controls, hold-to-repeat alternatives, no info conveyed by
color alone, photosensitivity-safe transitions (no hard strobes on descent fades).
