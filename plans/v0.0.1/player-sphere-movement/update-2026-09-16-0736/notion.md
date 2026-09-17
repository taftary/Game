# Update notion — player-sphere-movement / update-2026-09-16-0736 (UTC)

Parent feature: [`../notion.md`](../notion.md)

Related: [`../../debug-player-view/update-2026-09-16-0736/notion.md`](../../debug-player-view/update-2026-09-16-0736/notion.md),
[`../../chunk-flat-view/notion.md`](../../chunk-flat-view/notion.md) (reopened for the marker/viewpoint half).

## Status

`done`

## Reason for update

Movement was coupled to geographic axes: `MoveInput { forward, right }`
drove the north/east tangent frame directly (W = +latitude, D =
+longitude), so "player direction" did not exist — `facing()` was
velocity-derived and snapped to north at idle, and stopping snapped the
First/Third-person cameras with it. This update makes movement
heading-based: a stored compass bearing, thrust/turn input, and
great-circle straight walks.

## Scope

### In-scope

- `game::player`: stored `heading`, `MoveInput { forward, turn }`,
  `TURN_RATE`, parallel-transported heading, heading-derived `facing()`.
- `game` headless demo legs re-authored (straight / curving /
  turn-in-place / idle) + heading in the log.
- `debug::player_view`: `MoveKeys` folds to thrust/turn;
  `facing()`/`heading_deg()` accessors.
- Unit tests rewritten to the new semantics (incl. the great-circle
  invariant).

### Out-of-scope

- Marker rendering (arrow + dot) → `debug-player-view` update.
- Flat-map viewpoint policy + exact marker projection →
  `chunk-flat-view` amendment (reopened to `in-progress`).
- Camera code changes (none needed — `facing()` consumers improve
  automatically).
- Touch/stick bindings, rebind UI (viewer stays keyboard-only).

## Requirements delta

1. Player stores a heading (compass bearing, 0 = north, + toward east,
   wraps `[0, TAU)`); thrust walks along it, turn rotates it at
   `TURN_RATE` (π rad/s at full deflection).
2. Heading persists while idle; `facing()` derives from it (velocity
   fallback removed).
3. Straight walks trace great circles (parallel transport after every
   step), not rhumb lines.
4. `MoveInput::new(forward, turn)`; `MoveKeys` north/south = thrust,
   west/east = turn.

## Definition of Done delta

- [x] Heading state + turn/thrust update + transport in `game::player`.
- [x] Great-circle invariant pinned by test; rewritten unit tests pass.
- [x] Headless demo re-authored, terminates, logs heading.
- [x] `debug::player_view` maps keys to thrust/turn; accessors added.
- [x] `docs/game/controls.md` rewritten; techstack bumped to 0.6.7.
- [x] All quality.md gates green.
