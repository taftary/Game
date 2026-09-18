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
- Debug viewer (`cargo run -p game_debug`, one window, ADR-022 as
  extended by ADR-023): top bar `F1` = Game Demo (default tab —
  interactive, presentation-accurate, HUD on, chrome hidden),
  `F2` = Dimensions dropdown (ten waypoints, digits `1`–`0` pick
  while open, `●` marks the active journey layer), `F3` = Settings
  (Controls section lists every action with its key; every row is
  clickable). Milky Way / Solar System / Earth dimension tabs mount
  the absorbed Galaxy Map / System Map / Planet View with their
  docks (260/300 px + padding) and content keys; the Cosmic Web tab
  (digit `1`) mounts the absorbed cosmic-web inspector (orbit/pan/
  log-zoom, click-node readout, live player point, `Home` top-down
  snap); other dimension tabs are placeholders with `INACTIVE`
  badges.
- Game Demo tab (v0.3.2 `cosmic-scale-player` — the main game
  notion: a player in space navigating the Cosmic Scale, amended by
  update-2026-09-18-2027): the player is a marker (`YOU` dot + heading
  arrow) with its own camera in the generated cosmic web, spawned
  inside a filament near the home galaxy. Mouse drag steers the nose;
  `W`/`S` = cruise forward/backward, `A`/`D` = lateral cruise (arrows
  mirror WASD) — full input crosses the local scale length (nearest-node
  distance, r_vir-scaled floor) in ~20 s with eased momentum, release
  coasts; `Shift`+wheel adjusts the pace (2–600 s crossing time);
  `P` cycles Chase (default) → Orbit (drag orbits, wheel zooms) →
  FirstPerson (marker hidden); wheel zooms the Chase/Orbit camera;
  click a node to target it (amber ring marker, also on the Cosmic Web
  tab's clicked node), `E` engages/cancels the eased fly-to (any
  cruise input also cancels); `R` re-seeds the web. The shipping HUD
  shows frame (cosmological/Mpc), time (real-time or compressed ratio),
  live cruise speed (Mpc/s), and the fly-to target with distance + ETA;
  SOI stays `—` (no handoffs at this scale). `E` is context-dependent
  by design: fly-to in the demo, travel-begin on the galaxy/system tabs
  (the `T` dual-binding precedent: travel offer / top preset).
- Dev widget (`` ` `` toggle; `F6`/`F7`/`F8` = FPS/Console/Inspector
  sub-tabs): floats over every tab. Console carries the transition-
  event log; a transition pill floats bottom-center while a waypoint
  transition is in flight. `F9`/`F10` toggle left dock / right dock;
  the tab bar is always visible and hosts three toggle buttons at its
  right end (left dock, right dock, dev widget with FPS value).
  `Esc` unwinds UI focus (dropdown → widget); closing the window exits.
- Every key has a button showing that key and vice versa (Controls
  section is the full registry). Chrome keys never shadow content
  keys: `U` toggles player mode, `WASD`/arrows drive thrust/turn on
  the planet, `P` cycles Follow → First-person → Third-person, `1`–`6`
  select the debug-shader mode (planet content), `F5` cycles the
  twilight stage, `G`/`T`/`B` snap the global camera to
  Perspective/Top/Bottom (planet content only, same as the panel VIEW
  buttons; `R` = Right preset there). Map content
  (`plans/v0.0.1/universe-maps`,
  `plans/v0.0.1/universe-maps-3d`): 3D perspective views — wheel = log zoom,
  left-drag = orbit, right/middle-drag = pan, click = select
  star/planet, `Home` = top-down snap toggle (classic north-up /
  east-right framing); Milky Way tab
  `E` drills into the selected star's system, `R` re-rolls the seed, seed
  field + Load (or Enter) loads a typed universe (`--seed N` flag does
  the same at startup); Solar System tab `F` toggles L4 planet focus, `T`
  arms/withdraws the travel offer, `E` begins the timed transit
  (cancellable with `T`/`Q` before commit), `Q` ascends one journey
  layer. The player marker is a center dot plus a heading
  arrow on the planet; the SELECTION panel shows `lon / lat /
  cam + loaded / heading` plus the walk keys while the player is on.

### Navigation HUD

The shipping HUD view model exposes four readouts without changing simulation:
the active frame and local units, real-time/compressed time state, SOI handoff
status with hysteresis, and the active select-to-focus target with distance and
ETA. The target uses its caller label or canonical frame coordinates. The
current headless release binary prints a deterministic `hud-trace`; pixel
placement binds when the windowed release shell lands.

### Autosave

Sessions autosave (ADR-004) on: confirmed frame transitions, completed SOI
handoffs, fly-to start/completion, clean quit, and a periodic interval
(default 120 s, configurable 30–600 s; `game --autosave-interval SECONDS`
in the headless build). A successful save shows a transient notice; a
failure shows a distinct warning and never blocks play. Corrupt saves are
quarantined and the previous snapshot loads — never a boot-loop. Saves are
local (`saves/` in dev) and hold ship + navigation + metadata only:
procedural content regenerates from seeds.

## Audio / UI

- Audio: ambient pad + UI + hazard stingers; full music/sfx pass is stretch. Mix buses with mute; no audio-codec crash on any target.
- UI: touch-first, readable at phone distance, color-blind-safe resource colors, scalable text (100–150%). Text rasterization via `fontdue`; shaders via `naga` SPIR-V (see [`../techstack/stack.md`](../techstack/stack.md)).
- Localization-ready strings (all UI via keys), but v1 ships English only.

## Accessibility minimums

Remappable desktop controls, hold-to-repeat alternatives, no info conveyed by
color alone, photosensitivity-safe transitions (no hard strobes on descent fades).
