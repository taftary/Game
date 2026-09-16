# Issue specs — debug-player-view / issue-2026-09-16-0851-3d-view-y-flipped (UTC)

Parent feature: [`../../notion.md`](../../notion.md)

Related issues: [`../../../debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored/report.md`](../../../debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored/report.md)
(same projection's winding mirror), [`../../../cell-chunks/issue-2026-09-15-1144-hover-y-inverted/report.md`](../../../cell-chunks/issue-2026-09-15-1144-hover-y-inverted/report.md)
(same undocumented NDC convention, inverse side).

## Status

`done`

## Observed vs Expected

- Observed (user, windowed, after the heading-movement update): the
  player camera is "flipped" and the heading arrow is invisible.
- Expected: Follow/FirstPerson/ThirdPerson render right-side up (north
  up-screen for a north-facing player); the dot + green arrow shows the
  heading in SphereMain and ChunkFlat focus; D turns the view clockwise
  (to the avatar's own right).

## Reproduction steps

1. `cargo run -p game_debug`, Sphere Viewer screen.
2. `U` (player mode): the marker shows no readable arrow in Follow /
   FirstPerson; the flat marker sits at the map rim.
3. `P` through the player cameras: the horizon/world reads upside
   down; turning with D feels mirrored.

## Scope & Impact

- Every 3D view in both windowed binaries (`game_debug` sphere fill +
  wireframe, `game_tools` planet) rendered through
  `OrbitCamera::projection_matrix` — i.e. all of them were vertically
  mirrored since the viewer exists; invisible on a featureless sphere
  until the player camera supplied a horizon reference.
- Player camera ergonomics (Follow default side, FirstPerson marker).
- Flat-map marker spawn position (viewpoint default vs player spawn).

## Logs / Evidence

- `follow_marker_arrow_visible_and_oriented` failed pre-fix:
  north-facing tip projected at py 414 vs origin 374 (down-screen).
- `turn_right_rotates_toward_own_right` failed pre-fix: positive turn
  rotated away from `facing × up` (screen-right).
- glam 0.33.7 source: `vulkan::perspective` =
  `camera_impl::perspective::<true, true, true>` with
  `let yy = if YFLIP { -h } else { h };` — the projection itself flips
  Y; the app's proven convention (UI ortho, flat MVP, picking) is
  NDC +1 = top row.

## Suspected area

- `crates/engine/src/render/camera.rs` (`projection_matrix`).
- `crates/game/src/player.rs` (lon/lat → world frame chirality).
- `crates/game/src/camera.rs` (Follow default yaw/pitch).
- `crates/debug/src/sphere_viewer.rs` (flat viewpoint default) and
  `crates/debug/src/main.rs` (marker projection path).
