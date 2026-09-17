# Plan — Player Sphere Movement

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Player (`crates/game/src/player.rs`)

Lon/lat anchor on a perfect sphere, tangent-plane `MoveInput`
(north/east), speed-capped diagonal, pole-margin clamp, longitude wrap,
surface velocity + facing. 8 unit tests.

### Phase 2 — Cameras (`crates/game/src/camera.rs`)

`CameraMode::{Follow, FirstPerson, ThirdPerson, Global}` with a one-key
`cycle()`. Follow reuses orbit math around the player; first/third person
use facing + surface normal; global is the untouched `OrbitCamera`. Shared
Vulkan-NDC projection. 7 unit tests.

### Phase 3 — Streaming (`crates/game/src/streaming.rs`)

Tick-based `ChunkStreamer` grace cache over the caller-supplied desired
set (`visible_hemisphere` on the player position): immediate loads,
eviction after `grace_ticks`. Deterministic, sorted deltas. 5 unit tests.

### Phase 4 — Demo loop (`crates/game/src/main.rs`)

Headless 20 Hz scripted walk (east → north-east → west → idle) cycling
all four camera modes, asserting the flat map recenters on the player
every tick, printing per-tick lon/lat + load/unload counts, terminating
after 48 ticks.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| PSM-001 | done | Player lon/lat movement + tests | Functional requirements 1 |
| PSM-002 | done | Four camera modes + cycle + tests | Functional requirements 2 |
| PSM-003 | done | Grace-delay chunk streamer + tests | Functional requirements 3 |
| PSM-004 | done | Headless demo loop + flat recenter assert | Functional requirements 4 |
| PSM-005 | done | Docs: controls mapping + techstack version | Definition of Done |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Player moves on sphere with tangent input | done | `player.rs` + 8 tests (`cargo test -p game`) |
| 2 | Four camera modes switchable in one cycle | done | `camera.rs`, `cycle_visits_every_mode` test |
| 3 | Chunks load within player hemisphere | done | demo `want=69 +69` on t=01; `loads_desired_set_immediately` |
| 4 | Trail unloads after grace delay | done | demo `-1` lines (t=25, t=40); `unseen_chunks_survive_within_grace_then_evict` |
| 5 | Flat map recenters on player | done | `debug_assert` on `project_to_tangent(pos, pos)` every tick |
| 6 | 20 tests pass; demo terminates | done | `cargo test -p game` 20/20; `cargo run -p game` ends `done: …` |
| 7 | Docs updated | done | `docs/game/controls.md` surface-walk section; techstack `0.6.4` |

## Acceptance criteria

- `cargo test -p game` → 20 passed, 0 failed.
- `cargo run -p game` → 48 tick lines + `done:` summary, exit 0.
- Workspace gates (`fmt`, `clippy -D warnings`, `test --workspace`)
  stay green.

## Risks & Next steps

- Interactive `winit` input not bound yet — `MoveInput` is the seam;
  wire keyboard/stick to it without touching movement math.
- Tuning (walk speed, follow distance, grace) needs playtesting once
  rendered; current values are demo constants.
- M2/M3 will replace the perfect sphere with heightfield collision;
  `Player::position` stays the camera/streaming anchor.
