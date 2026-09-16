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
- Headless demo: `cargo run -p game` runs a scripted 20 Hz walk
  (straight leg, curving leg, turn-in-place leg, idle) through all four
  modes, logging lon/lat/heading (interactive `winit` binding deferred).
- Debug viewer (`cargo run -p game_debug`, two windows): viewer window
  `F1` = Sphere Viewer, `F2` = UV Net, `F3` = Galaxy Map, `F4` = System
  Map, `F5` reopens the tools window
  (FPS / Console / Inspector tabs on window-local `1/2/3`); `U`
  toggles player mode, `WASD`/arrows drive thrust/turn on the sphere,
  `P` cycles Follow → First-person → Third-person, `1`–`6` select the
  debug-shader mode, `G`/`T`/`B` snap the global camera to
  Perspective/Top/Bottom (sphere screen only, same as the panel VIEW
  buttons; `R` = Right preset there). Map screens (`plans/universe-maps`):
  wheel = log zoom, drag = pan, click = select star/planet; Galaxy Map
  `E` drills into the selected star's system, `R` re-rolls the seed, seed
  field + Load (or Enter) loads a typed universe (`--seed N` flag does
  the same at startup); System Map `F` toggles L4 planet focus, `T`
  arms/withdraws the travel offer, `E` begins the timed transit
  (cancellable with `T`/`Q` before commit), `Q` ascends one journey
  layer. The player marker is a center dot plus a heading
  arrow on the sphere; the SELECTION panel shows `lon / lat /
  cam + loaded / heading` plus the walk keys while the player is on.

## Audio / UI

- Audio: ambient pad + UI + hazard stingers; full music/sfx pass is stretch. Mix buses with mute; no audio-codec crash on any target.
- UI: touch-first, readable at phone distance, color-blind-safe resource colors, scalable text (100–150%). Text rasterization via `fontdue`; shaders via `naga` SPIR-V (see [`../techstack/stack.md`](../techstack/stack.md)).
- Localization-ready strings (all UI via keys), but v1 ships English only.

## Accessibility minimums

Remappable desktop controls, hold-to-repeat alternatives, no info conveyed by
color alone, photosensitivity-safe transitions (no hard strobes on descent fades).
