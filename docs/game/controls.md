# Controls, audio, UI, accessibility (v1 minimums)

## Input

Unified action map (`engine::input`), not per-device logic in gameplay:

- Touch: virtual joystick + tap-to-move/order, pinch zoom in maps/orbit, two-finger orbit on surface.
- Mouse/keyboard: WASD + click orders, wheel zoom, Esc menu.
- Gamepad: left stick move, right stick camera, face buttons confirm/cancel/orders.
- All actions rebindable on desktop; touch layout has large targets (≥44pt).
- Future XR poses map to the same actions; no v1 XR code.

## Audio / UI

- Audio: ambient pad + UI + hazard stingers; full music/sfx pass is stretch. Mix buses with mute; no audio-codec crash on any target.
- UI: touch-first, readable at phone distance, color-blind-safe resource colors, scalable text (100–150%). Text rasterization via `fontdue`; shaders via `naga` SPIR-V (see [`../techstack/stack.md`](../techstack/stack.md)).
- Localization-ready strings (all UI via keys), but v1 ships English only.

## Accessibility minimums

Remappable desktop controls, hold-to-repeat alternatives, no info conveyed by
color alone, photosensitivity-safe transitions (no hard strobes on descent fades).
