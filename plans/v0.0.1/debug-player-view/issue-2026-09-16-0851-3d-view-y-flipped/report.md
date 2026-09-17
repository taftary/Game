# Issue report — debug-player-view / issue-2026-09-16-0851-3d-view-y-flipped (UTC)

Specs: [`specs.md`](specs.md)

## Root cause

Four stacked causes, one umbrella theme: **the app's screen-space
convention existed nowhere in writing** (the same gap
`issue-2026-09-15-1144-hover-y-inverted` called out for the inverse
mapping).

1. **Y-flipped projection (the flip).** The app's proven framebuffer
   convention is **NDC +1 = top row** (proven daily by the upright UI
   — `ortho_matrix` maps pixel row 0 to NDC +1 — and by working flat
   hover, `flat_point_from_cursor` ↔ `flat_mvp`). But
   `OrbitCamera::projection_matrix` built glam's `vulkan::perspective`,
   which bakes `yy = −h` into the matrix: view-space up lands at NDC
   −1 = the **bottom** row. Every 3D view (both binaries) was
   vertically mirrored from day one; a sphere is mirror-symmetric, so
   it went unnoticed until the player camera gave a horizon. (The
   historical winding issue `issue-2026-09-14-2113` compensated for
   this same flip with `FrontFace::Clockwise` instead of removing it.)
2. **Mirrored tangent frame (wrong turn sense).** The player's
   lon/lat → world map `z = +R·cos(lat)·sin(lon)` made
   `(east, north, up)` **left-handed** (`east × north = −up`): a
   mirrored ENU. Through the right-handed view construction, positive
   (clockwise) heading turns appeared counter-clockwise on screen —
   D steered toward the avatar's own *left* (FirstPerson screen-right
   is `facing × up`; with the mirrored frame that cross pointed west).
3. **Follow default side.** The Follow camera opened north of the
   north-facing player (offset `+Y`), so thrust walked *down*-screen —
   a second "flipped" read independent of the projection.
4. **Marker visibility.** (a) FirstPerson projects the facing tip into
   its own eye plane → the arrow is mathematically invisible there by
   design (screen-up IS the heading); the dot also hides (w ≤ 0).
   (b) The flat viewpoint defaulted to the north pole while the player
   spawns at the lon/lat origin (equator) → the fresh flat marker sat
   rim-clamped at the map edge.

## Evidence

- Pre-fix failing probes (kept as regression tests):
  `turn_right_rotates_toward_own_right` (player.rs) and
  `follow_marker_arrow_visible_and_oriented` (main.rs): north tip at
  py 414.5 vs origin 374.0 — down-screen, identical before/after any
  camera-default change, isolating the projection.
- glam 0.33.7 `camera/rh/proj.rs` + `camera_impl.rs`:
  `vulkan = <RH, ZO, YFLIP=true>` ⇒ `yy = −h`;
  `directx = <RH, ZO, YFLIP=false>` — identical RH + Z∈[0,1] matrix
  **without** the flip (the fix uses it).
- Chirality: at lon=0, old frame gave `east × north = (0,−1,0) = −up`.
- Post-fix: all probes pass; headless demo output byte-identical
  (lon/lat semantics unchanged by the frame flip);
  `player_selftest=lon28.64 lat-0.00` preserved.

## Alternatives considered

- Negative viewport height (`VK_AMD_negative_viewport_height` or core
  1.1 negative height): rejected — flips the framebuffer for every
  pass (UI + flat included), contradicting the proven convention.
- Keep the flipped projection, flip `world_to_pixels` instead:
  rejected — treats the symptom for markers only; the render itself
  stays upside down, and picking/UI already prove NDC +1 = top.
- Fix only the camera defaults (Follow side): tried first —
  insufficient; the projection flip dominated (probe values unchanged).
- Rhumb-line vs great-circle heading transport: unchanged by this
  issue (still great-circle).

## Fix direction

1. `engine::render::OrbitCamera::projection_matrix` → glam
   `directx::perspective` (framebuffer-true NDC); doc comment states
   the convention; test now asserts equality with `directx` and
   inequality with the Y-flipped `vulkan` constructor.
2. Winding follows the un-flipped projection:
   `FrontFace::CounterClockwise` (+ `CullMode::Back`) in the
   `game_debug` fill pipeline and the `game_tools` planet pipeline
   (comments updated; supersedes the `issue-2026-09-14-2113`
   compensation).
3. `game::player` world frame → true ENU (`z = −R·cos(lat)·sin(lon)`,
   `north`/`east` re-derived, inverse `atan2(−z, x)`); pinned by
   `tangent_frame_is_east_north_up` + `turn_right_rotates_toward_own_right`.
4. `PlayerCamera` Follow default: south of the player looking north
   (`follow_pitch −0.5`): thrust walks up-screen, east right-screen.
5. Marker behavior documented + pinned: FirstPerson hides dot+arrow by
   design; `regenerate` opens the flat map on the player
   (`chunk_flat_viewpoint = player.position()`).
6. The convention is now written down:
   `docs/techstack/rendering.md` § *Camera & screen-space conventions*,
   surfaced from `AGENTS.md`.
