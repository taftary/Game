# Update plan — player-sphere-movement / update-2026-09-16-0736 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

1. `crates/game/src/player.rs` — `heading` field (default north),
   `MoveInput.right → turn`, `TURN_RATE`, turn-first update with
   parallel transport, heading-derived `facing()`, rewritten tests.
2. `crates/game/src/main.rs` — LEGS as (thrust, turn) pairs + `hdg`
   in the per-tick and final log lines.
3. `crates/debug/src/player_view.rs` — key-fold docs, `facing()` /
   `heading_deg()` accessors, rewritten key/stream tests.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260916-001 | done | Heading state + turn/thrust + transport + tests (`game::player`) | Requirements delta 1–3 |
| UPD-20260916-002 | done | Re-author headless demo legs + heading log | Requirements delta 4 |
| UPD-20260916-003 | done | `MoveKeys` mapping + facing/heading accessors + tests | Requirements delta 4 |
| UPD-20260916-004 | done | Docs (`controls.md`, techstack 0.6.7) + quality gates | DoD delta |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Heading + turn/thrust + transport in `game::player` | done | `player.rs`: `heading` field, `TURN_RATE`, transport block; `cargo test -p game --lib` 22/22 |
| 2 | Great-circle invariant pinned; tests pass | done | `straight_walk_traces_a_great_circle` (200-step plane check + bearing drift); `turning_in_place_does_not_move`, `idle_player_keeps_heading` |
| 3 | Demo re-authored, terminates, logs heading | done | `cargo run --bin game` ends `done: lon=1.30 lat=5.06 hdg=306.1 loaded=69`; legs show straight (hdg 0.0), curve (hdg 4.5→54.1), turn-in-place, idle |
| 4 | Key mapping + accessors | done | `player_view.rs`: `facing()`, `heading_deg()`; `turn_key_rotates_without_walking`, `thrust_key_walks_along_heading` pass (debug lib 89/89) |
| 5 | Docs + version bump | done | `controls.md` Surface-walk rewritten; `techstack/README.md` → 0.6.9 |
| 6 | Quality gates green | done | `fmt --check`, `clippy --workspace --all-targets --all-features -- -D warnings`, `test --workspace --all-targets` (24 game + 89 debug lib + 19 debug bin + 61 engine), `test --doc --workspace` (4 + 6), `game` + `game_debug --headless` (`player_selftest=lon28.64 lat-0.00`) + `game_tools --headless --tier low` all green |
| 7 | Follow-up 2026-09-16 (visual check): world tangent frame is true ENU; turn input steers toward the avatar's own right | done | User report: camera flipped. Root cause: the old `lon → (cos, ·, sin)` world frame made `(east, north, up)` left-handed (mirrored ENU), so positive turns rotated toward screen-left through the `facing × up` view construction. Frame flipped to `z = −R·cos(lat)·sin(lon)` (`position`/`north`/`east` + `atan2(−z, x)` inverse); pinned by `tangent_frame_is_east_north_up` and `turn_right_rotates_toward_own_right` (pre-fix: failed; post-fix: pass). Lon/lat semantics unchanged — headless demo output identical |

## Acceptance criteria

- W/S thrusts along the heading, A/D turns it; releasing keys keeps
  the heading (no north snap).
- Holding W traces a great circle (test-pinned), not a lat/lon axis.
- Headless demo + viewer self-test keep their streaming guarantees.

## Risks & Next steps

- Rim re-anchor cadence (~70° threshold) is tuned for debug speeds;
  playtest before locking.
- Next: arrow marker (done in the sibling `debug-player-view`
  update), touch/stick bindings, walk-speed tuning.
