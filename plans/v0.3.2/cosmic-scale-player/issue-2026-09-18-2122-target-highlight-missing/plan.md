# Issue plan — cosmic-scale-player / issue-2026-09-18-2122-target-highlight-missing (UTC)

Specs: [`specs.md`](specs.md) · Report: [`report.md`](report.md)

## Fix breakdown

Overlay-only fix in `crates/debug/src/main.rs` (risk-first: helper +
Game Demo ring first, inspector tab second, tests lock both). No
camera / projection / picking invariant is touched — both draw sites
reuse the pinned projectors (`world_to_pixels`, `project_to_screen`)
and the upload-origin frames the picking code already uses.

## Role sign-off

Fix reviewed by TECHLEAD: approved (no budget impact — UI pass, +8
quads worst case) · Verified by ANALYST: _(pending)_ · Security
reviewed by SECURITY: _(pending)_

## Todo

| ID | Status | Task | Ref specs § |
|----|--------|------|-------------|
| ISS-20260918-001 | done | `draw_target_ring` helper + amber Game Demo ring on `target_node` | Observed vs Expected |
| ISS-20260918-002 | done | Amber ring on `inspector.selected` in `build_cosmic_web_ui` | Observed vs Expected |
| ISS-20260918-003 | done | Helper geometry unit test + 2 headless overlay tests | Logs / Evidence |
| ISS-20260918-004 | done | Gates green (`fmt`, `clippy`, `test --workspace`) + `controls.md` ring mention | Scope & Impact |

## DoD verification

| DoD # | Criterion (from specs.md) | Status | Evidence |
|-------|---------------------------|--------|----------|
| 1 | Clicking a node in Game Demo shows an in-world marker on it | ✅ done | `draw_target_ring_emits_four_rects`, `demo_target_ring_overlays_selected_node`; manual: F1 click → amber ring |
| 2 | Ring persists through fly-to until arrival / cancel / re-click | ✅ done | By construction: ring reads `target_node`, cleared only on arrival (`cosmic_player.rs:248`), cancel (`:306`), miss (`cosmic_demo.rs:259`) |
| 3 | Cosmic Web tab click shows an in-viewport marker alongside the readout | ✅ done | `inspector_selected_ring_overlays_node`; manual: F2 click → amber ring |
| 4 | No behavior change to picking, flight, cameras, or pipelines | ✅ done | `git diff --stat` touches `main.rs` overlay code + tests only; full workspace gates green |

## Acceptance criteria

- [x] Amber 9 px ring appears on the clicked node in the Game Demo viewport.
- [x] Amber 9 px ring appears on the clicked node in the Cosmic Web tab viewport.
- [x] Rings hide when the node is behind the camera / off-screen; demo ring clears with the target.
- [x] `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-targets` all green.
