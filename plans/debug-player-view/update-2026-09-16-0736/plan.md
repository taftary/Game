# Update plan — debug-player-view / update-2026-09-16-0736 (UTC)

Parent update notion: [`notion.md`](notion.md)

Follow-up issue (camera flip + arrow visibility):
[`../issue-2026-09-16-0851-3d-view-y-flipped/specs.md`](../issue-2026-09-16-0851-3d-view-y-flipped/specs.md).

## Feature delta breakdown

1. `crates/debug/src/main.rs` — `UiTri` alias + `UiItems::tri` +
   emission, `arrow_tris`, `draw_player_marker`, sphere/flat marker
   blocks, PLAYER row `hdg`, re-authored headless walk.
2. Tests — `arrow_tris_points_along_dir`,
   `draw_player_marker_emits_dot_arrow_and_label`.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260916-101 | done | `tri` primitive + vertex emission + arrow geometry + tests | Requirements delta 1–2 |
| UPD-20260916-102 | done | Dot + arrow marker in both views | Requirements delta 1–2 |
| UPD-20260916-103 | done | Heading in PLAYER row + headless walk | Requirements delta 3–4 |
| UPD-20260916-104 | done | Quality gates | DoD delta |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | `tri` + arrow geometry implemented and tested | done | `UiItems::tri`, `arrow_tris` + `arrow_tris_points_along_dir` (exact shaft/head coords) and `draw_player_marker_emits_dot_arrow_and_label`; debug bin 16/16 |
| 2 | Dot + arrow in both views | done | `draw_player_marker` called from the sphere block (camera-projected facing tip) and the flat block (normalization-projected tip); `PLAYER_DOT` 10→6 px; no new pipeline |
| 3 | Heading in readout | done | Row `cam: {mode} +{loaded} hdg {deg}`; `viewer_ui_shows_player_readout_when_active` still needles `cam: follow` |
| 4 | Headless self-test green | done | `cargo run -p game_debug -- --headless` → `player_selftest=lon28.64 lat-0.00 loaded21873 ok` (turn 0.5 s to east, thrust 20 ticks) |
| 5 | Quality gates green | done | Same workspace gate run as the sibling update (fmt, clippy `-D warnings`, all tests, all three binary runs) |
| 6 | Follow-up 2026-09-16 (visual check): 3D projection un-flipped; arrow visible and correctly oriented | done | User report: no arrow + flipped player camera. Root causes: (a) `OrbitCamera::projection_matrix` used glam's Y-flipped `vulkan::perspective`, mirroring every 3D view vertically against the app's proven framebuffer convention (UI ortho + flat-map MVP + picking all treat NDC +1 as the top row) — now `directx::perspective` (same RH/ZO, no flip); fill-pipeline winding back to `FrontFace::CounterClockwise` (debug viewer + tools); (b) Follow opened north-side of the player (W walked down-screen) — now south-side looking north (`follow_pitch −0.5`); (c) FirstPerson projects the facing tip into its own eye plane (arrow mathematically invisible by design — screen-up IS the heading; dot also hidden, w ≤ 0); (d) flat viewpoint defaulted to the north pole, pinning the fresh marker at the rim — regenerate now opens the flat map on the player. Regression tests: `follow_marker_arrow_visible_and_oriented` (tip projects ≥16 px, north up-screen, D-turn swings right), `first_person_hides_marker_and_looks_along_heading`, `flat_marker_spawns_centered` |

## Acceptance criteria

- `U` + walk: dot + green arrow tracks the heading in SphereMain and
  ChunkFlat focus; arrow disappears (dot stays) when its tip leaves
  the projection.
- PLAYER row shows heading in degrees alongside mode and loaded count.

## Risks & Next steps

- Arrow pixel clamp (16–48 px) is a debug-viewer heuristic; revisit
  for extreme zooms.
- Next: orientation-preserving rim re-anchor basis (engine-side
  polish, optional), walk-speed/turn-rate playtest tuning.
