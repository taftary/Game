# Issue plan — debug-player-view / issue-2026-09-16-0851-3d-view-y-flipped (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

Fix landed together with the heading-movement updates
([`../update-2026-09-16-0736/plan.md`](../update-2026-09-16-0736/plan.md),
follow-up rows) and is recorded here per the issue lifecycle.

## Fix breakdown

1. Engine projection un-flipped + winding follow-up (both binaries).
2. Player tangent frame made true ENU.
3. Follow camera default side; FirstPerson marker behavior documented.
4. Flat map opens on the player at regenerate.
5. Regression tests at every level (frame, turn sense, marker
   visibility/orientation, flat spawn).
6. Convention written into `docs/techstack/rendering.md` + `AGENTS.md`.

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260916-001 | done | `projection_matrix` → `directx::perspective`; test eq/ne updated | Suspected area 1 |
| ISS-20260916-002 | done | `FrontFace::CounterClockwise` in `game_debug` fill + `game_tools` planet pipelines | Suspected area 1 |
| ISS-20260916-003 | done | ENU frame in `player.rs` + chirality/turn-sense tests | Suspected area 2 |
| ISS-20260916-004 | done | Follow default south-of-player (`follow_pitch −0.5`) | Suspected area 3 |
| ISS-20260916-005 | done | Flat viewpoint opens on player; marker regression tests (Follow orientation, FirstPerson hidden, flat spawn) | Suspected area 4–5 |
| ISS-20260916-006 | done | Rendering conventions spec + AGENTS.md pointer + version 0.6.9 | Logs/Evidence |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | 3D views render right-side up | done | `follow_marker_arrow_visible_and_oriented`: north tip 40.5 px up-screen; projection == `directx::perspective`, != `vulkan::perspective` (engine test) |
| 2 | D turns toward the avatar's own right | done | `turn_right_rotates_toward_own_right` + `tangent_frame_is_east_north_up` (both failed pre-fix) |
| 3 | Heading arrow visible in Follow + ChunkFlat | done | Follow: tip ≥16 px up-screen (test-pinned); flat: tip through stored `chunk_flat_norm`; FirstPerson hides by design (`first_person_hides_marker_and_looks_along_heading`) |
| 4 | Flat marker no longer spawns at rim | done | `regenerate` sets `chunk_flat_viewpoint = player.position()`; `flat_marker_spawns_centered` |
| 5 | No regressions | done | Full gates green: 24 game + 89 debug lib + 19 debug bin + 61 engine, doc 4+6, `game` (output byte-identical), `game_debug --headless` (`player_selftest=lon28.64`), `game_tools --headless --tier low` |
| 6 | Convention written down for agents | done | `docs/techstack/rendering.md` § *Camera & screen-space conventions*; `AGENTS.md` § *Rendering invariants*; techstack 0.6.9 |

## Acceptance criteria

- Windowed viewer: `U` → dot + green arrow points up-screen (fresh
  north heading); `P` cycles right-side-up cameras; D turns clockwise.
- Flat focus (`V`): marker spawns inside the map with its arrow.
- Historical issues cross-linked: this fix supersedes the
  `issue-2026-09-14-2113` winding compensation and completes the
  convention gap called out by `issue-2026-09-15-1144`.
