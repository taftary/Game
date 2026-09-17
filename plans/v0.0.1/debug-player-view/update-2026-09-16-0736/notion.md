# Update notion — debug-player-view / update-2026-09-16-0736 (UTC)

Parent feature: [`../notion.md`](../notion.md)

Related: [`../../player-sphere-movement/update-2026-09-16-0736/notion.md`](../../player-sphere-movement/update-2026-09-16-0736/notion.md),
[`../../chunk-flat-view/notion.md`](../../chunk-flat-view/notion.md) (reopened for the flat half).

## Status

`done`

## Reason for update

The player marker was a directionless 10 px square: with heading-based
movement (sibling `player-sphere-movement` update) the viewer must show
*where the player faces*. This update replaces the dot with a
dot + heading-arrow marker in both the sphere and flat views, adds a
screen-space triangle primitive to the UI batch, and surfaces the
heading in the PLAYER panel row.

## Scope

### In-scope

- `UiItems::tri` primitive + emission in `ui_items_to_vertices` (no new
  Vulkan pipeline; reuses the solid white-texel path).
- Pure `arrow_tris` geometry helper (shaft quad + head) + unit tests.
- `draw_player_marker` (dot + clamped 16–48 px arrow + "YOU") used by
  both views; sphere tip from the player-camera projection, flat tip
  through the flat normalization.
- PLAYER row shows `cam +loaded hdg`.
- Headless self-test re-authored for turn-then-thrust (still 28.64° east).

### Out-of-scope

- Heading movement itself → `player-sphere-movement` update.
- Flat-map viewpoint policy, exact marker projection, streaming
  decouple → `chunk-flat-view` amendment.
- Player mesh / sprites (still UI quads).

## Requirements delta

1. Marker shows facing: arrow along the projected heading + small
   center dot at the exact position, player-green, both views.
2. Arrow hides gracefully when the tip doesn't project (dot remains).
3. Heading visible in degrees in the PLAYER panel row.
4. Headless self-test keeps walking 28.64° east with a fully loaded
   hemisphere.

## Definition of Done delta

- [x] `tri` primitive + arrow geometry implemented and unit-tested.
- [x] Dot + arrow marker in sphere and flat views.
- [x] Heading in the PLAYER readout.
- [x] Headless self-test green with turn-then-thrust walk.
- [x] All quality.md gates green.
