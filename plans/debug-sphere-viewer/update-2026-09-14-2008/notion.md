# Update notion — debug-sphere-viewer / update-2026-09-14-2008 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`done`

## Reason for update

The notion's interactive default N=6 makes cells subpixel at typical
window sizes (~7 px²/cell at 800×600): faces are unreadable and the 12
pentagon sites are indistinguishable — the viewer fails its inspection
purpose on the default view, and low-N probes kept being needed to see
anything (raised in review after issue-2026-09-14-1749).

## Scope

### In-scope

- Windowed viewer default becomes **N=4** (2,562 cells): clearly
  readable dual-cell faces, discernible pentagon sites, ~50 ms
  regeneration — interactive by a wide margin. Users can still set any
  level 0–8 via slider/field; N=6 remains the cost-curve level
  (warning styling above it already exists).
- The `--headless` path stays **N=6, R=1.0** so the CI cross-check of
  the committed engine mesh hash is unchanged.

### Out-of-scope

- No engine/game changes; no changes to input ranges, toggles, nav, or
  the stats panel.
- No subdivision-dependent wireframe tuning (existing alpha veil reads
  acceptably at N=4).

## Requirements delta

- Amends parent `## Definition of Done` item 1 and the `## Non-functional
  requirements` gate bullet: "opening the viewer" now opens at
  subdivisions **4** instead of the engine default 6. The subdivision
  range (0–8), default toggles, radius default, and stats behavior are
  unchanged.

## Definition of Done delta

- [x] `cargo run -p game_debug` opens at N=4: panel shows `4` /
  `→ 2,562 cells` and the viewport shows readable faces with
  discernible pentagon sites (screenshot evidence).
- [x] `cargo run -p game_debug -- --headless` still prints the N=6
  stats line with `hash8=9f087a31` (engine hash cross-check intact).
- [x] No change to `game_engine`/`game`; all `quality.md` gates green.
