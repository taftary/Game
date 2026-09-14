# Update plan — debug-sphere-viewer / update-2026-09-14-2008 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

Two constructors added (`SphereViewerState::with_values`,
`App::with_viewer` — the parametric core of the existing `new()`), then
the windowed binary constructs the viewer with `WINDOWED_SUBDIV = 4`.
`--headless` keeps `SphereViewerState::new()` (N=6). One inline note in
`docs/techstack/rendering.md`. Verified by screenshot + gates.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260914-001 | done | `SphereViewerState::with_values(subdiv, radius)`; `new()` delegates (headless N=6 path untouched) | In-scope |
| UPD-20260914-002 | done | `App::with_viewer` constructor | In-scope |
| UPD-20260914-003 | done | Windowed binary opens at N=4 (`WINDOWED_SUBDIV`) + rationale comment | In-scope |
| UPD-20260914-004 | done | Verify: N=4 screenshot (readable faces + sites), `--headless` N=6 line, fmt/clippy/test gates | Definition of Done delta |
| UPD-20260914-005 | done | `rendering.md` viewer section notes the N=4 windowed / N=6 headless split | Requirements delta |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Viewer opens at N=4 with readable faces and discernible sites | done | `viewer_default_n4.png`: panel `4` / `→ 2,562 cells`, cells individually readable, olive pentagon sites visible through the veil, gen 50.4 ms |
| 2 | `--headless` N=6 cross-check intact | done | `subdiv=6 radius=1 cells=40962 corners=81920 pentagons=12 tris=245760 hash8=9f087a31 gen_ms=1011.9` |
| 3 | No engine/game change; gates green | done | `git status` touches only `crates/debug` + this folder; 48 lib + 7 bin tests, clippy `-D warnings`, fmt all green |

## Acceptance criteria

- Default window shows a readable sphere (faces + pentagon sites) at
  N=4 with interactive regeneration (~50 ms).
- Slider/field still reach 0–8 including N=6; warning styling above 6
  unchanged.
- CI debug gate (`--headless`) unchanged and green.

## Risks & Next steps

- Risk: at 4k+ windows N=4 cells can still be small → if raised, make
  the default a `--subdiv N` flag instead of a constant (one-line argv
  addition; not needed at 1080p-class windows).
- Next: same "default view must demonstrate the feature" lesson applies
  when the descent slice (M2) picks its default camera framing.
