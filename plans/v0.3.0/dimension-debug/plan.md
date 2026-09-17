# Plan — dimension-debug

## Roles

| Role | Sign-off |
|---|---|
| ARCHITECT | pending |
| TECHLEAD | pending |
| DEV | pending |
| ANALYST | pending |
| SECURITY | pending |

## Tasks

- [x] Add read-only `DimensionTab` and waypoint graph projection.
- [x] Add Dimensions tools section and tab navigation.
- [x] Render L1-L5 using existing map, line, and planet pipelines.
- [x] Render Connections using the existing line and point pipelines.
- [x] Add unit coverage for tab order and graph shape.
- [x] Update journey and rendering documentation.
- [x] Record final quality-gate evidence.

## DoD verification

| Criterion | Status | Evidence |
|---|---|---|
| Six tabs reachable | done | `ToolsScreen::Dimensions`, `DimensionTab::ALL` |
| Existing 3D data reused | done | `draw_tools` dimensions render branch |
| Connections graph | done | `graph_positions`, `graph_legs`, GPU uploads |
| Read-only behavior | done | `DimensionsState` owns tab only; source state is borrowed |
| Quality gates | done | `cargo test --workspace --all-targets`, `cargo clippy -p game_debug --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` |
