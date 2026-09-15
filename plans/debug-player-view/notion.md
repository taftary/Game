# Notion — Debug Player View

## Status

`done`

## Context

`plans/player-sphere-movement` (done) built pure player logic
(`Player`, `PlayerCamera`, `ChunkStreamer`) in the `game` binary plus a
headless scripted demo. The debug sphere viewer (`game_debug`) still
shows a player-less planet: orbit camera only, flat viewpoint orbited by
arrow keys, no visible player, no lat/lon readout. This feature wires
the two together so movement and streaming can be seen and driven
interactively.

Parent: [`../player-sphere-movement/notion.md`](../player-sphere-movement/notion.md).

## Problem & Needs

Developers (and early playtesters) need to *see* the player on both the
3D sphere and the flat chunk map, move him with the keyboard, read his
longitude/latitude, switch between global and player cameras, and watch
chunks stream as he walks — all inside the debug viewer, without
guessing from headless logs.

## Goals

- Visible player marker on the 3D sphere and on the flat chunk map.
- `WASD`/arrows move the player on the sphere surface.
- Panel readout: longitude/latitude (degrees), camera mode, loaded
  chunks.
- `U` toggles player mode; `P` cycles Follow → FirstPerson →
  ThirdPerson; `V` keeps the old focus-cycle.
- Flat map auto-follows the player while active (same hemisphere rule).
- Global camera preset views (Top / Bottom / Right / Perspective),
  clickable in the panel (+ keys `G/T/B/R`).
- All viewer logic stays window-/GPU-free and unit-tested; the binary
  only projects markers and routes input.

## Non-goals

- No dedicated player mesh pipeline (markers reuse the UI quad pass).
- No interactive input in the release `game` binary (still headless).
- No terrain, collision, or player physics (still perfect sphere, M3).
- No Streaming-grace tuning (reuse the 10-tick default).

## Users / Stakeholders

- Primary: developers verifying movement + streaming visually.
- Secondary: playtesters learning the camera modes.

## Functional requirements

1. `game` modules (`player`, `camera`, `streaming`) exposed as a
   library so `game_debug` can reuse them (binary stays thin).
2. `PlayerViewState` (debug lib, pure): owns player + player camera +
   streamer + key flags; per-frame `update(dt, desired)`; lon/lat,
   mode, loaded-count accessors.
3. `U` toggles player mode; `P` cycles player cameras; `WASD` + arrows
   move; `V` cycles main/thumb focus (was `U`).
4. Sphere viewport renders through the player camera while active
   (all three modes); drag/wheel route to it.
5. Player dot on the sphere (projected UI marker) and on the flat map
   (visible-chunk center lookup).
6. Flat viewpoint follows the player while active (throttled reload).
7. `CHUNK` panel keeps working; new `PLAYER` section (lon/lat/mode/
   chunks) and `VIEW` section (4 clickable preset buttons).
8. Headless self-test covers the player path (CI-safe, GPU-free).

## Non-functional requirements

- Deterministic lib logic (tick-based streaming, tested).
- Flat reload while following throttled to 10 Hz (frame-cost bound).
- `cargo fmt`, `clippy -D warnings`, full workspace gates green.

## Definition of Done

- [x] `game` exposes `player`/`camera`/`streaming` as a lib; demo
  unchanged (`cargo run --bin game` ends `done: …`).
- [x] `U` toggles player mode; `P` cycles the three player cameras.
- [x] `WASD`/arrows walk the player; lon/lat readout updates
  (`viewer_ui_shows_player_readout_when_active`).
- [x] Player dot renders on sphere (`world_to_pixels`) and flat map
  (`flat_uv_to_pixels`, chunk-center lookup).
- [x] Flat map follows the player (10 Hz throttle); headless
  self-test walks 28.64° east with the hemisphere fully loaded.
- [x] Preset buttons + `G/T/B/R` move the global camera
  (`global_presets_frame_the_planet`, `split_row_4` test).
- [x] New unit tests (7 lib + 5 bin); workspace green
  (20 game + 85 debug lib + 14 debug bin + 61 engine, 10 doc).
- [x] Docs updated (controls key map, techstack 0.6.5).

## Constraints & Assumptions

- Markers reuse the UI quad pipeline (no new Vulkan pipeline).
- Panel already overflows at 720p (pre-existing); new sections follow
  the same pattern (fine at normal window sizes).
- `U` rebinding changes existing viewer UX (documented here).

## Open questions

- None blocking. Tuning (walk speed, follow distances, reload rate)
  after interactive feel.
