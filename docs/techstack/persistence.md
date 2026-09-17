# Save format

- Single versioned binary file per run + JSON sidecar for debugging (`save.bin` + `save.json` via `tools` converter).
- Contains: `universe_version`, galaxy seed, visited systems/planets, colony states, robot states, resources, codex, player position/state, game-time.
- Atomic write (write-temp + rename); corrupt save → backup + clean error, never boot-loop.
- Migration: N → N+1 scripts in `engine::save`; unmigratable = explicit "new game required" message.
- No cloud saves in v1; local only. Slots: 3 + autosave ring.

## Autosave envelope (ADR-004, binding 2026-09-17)

Shipped by `plans/v0.3.0/autosave-persistence`. Codec: `engine::save::format`;
store: `engine::save::store`; triggers: `game::save`. Procedural content is
never saved (ADR-019) — the payload is ship + navigation + metadata only.

Binary layout (all integers little-endian):

| Offset | Field |
|---|---|
| 0 | magic `GSAVE` (5 B) |
| 5 | envelope version (u8) = 1 |
| 6 | master seed (u64) |
| 14 | real-world save timestamp, unix seconds (i64, metadata only) |
| 22 | simulation timestamp (f64, seconds) |
| 30 | session playtime (f64, seconds) |
| 38 | catalog version count n (u8, ≤ 16) |
| 39 | n × (u8 len + UTF-8 bytes) catalog version IDs (≤ 64 B each, ADR-020) |
| … | payload length (u16; 131 or 204) |
| … | `flight::ShipSnapshot` payload (ship 6D state + orientation + mass/fuel + optional fly-to plan + compression) |
| … | FNV-1a 64 checksum over every preceding byte (u64) |

Decode order: magic/length → **checksum → version → parse**. Unknown
versions, truncation, checksum mismatch, and out-of-bounds metadata reject
cleanly — never a panic, never a boot-loop. The checksum is corruption
integrity (FNV-1a 64, hand-rolled), not an adversarial MAC.

Store behavior (`AutosaveRing`):

- `autosave.0.bin` is newest; higher slots are older recovery snapshots
  (default depth 3, ADR-004 floor is 2). New writes land via temp + rename,
  then rotation renames whole files.
- `load_latest` walks newest → oldest; corrupt files are quarantined to
  `*.corrupt` (never deleted) and the next slot is tried. `NoValidSave` is a
  clean empty-state outcome.
- Dev location: workspace `saves/` (gitignored); releases use the platform
  user-data directory. File names are fixed — no caller path components.

Triggers (`game::save::Autosave`, ADR-004 set): confirmed frame transition,
completed SOI handoff, fly-to start, fly-to completion, clean quit, and the
periodic interval (default 120 s, clamped 30–600 s, caller-fed). The
real-world timestamp is metadata only and never enters resume determinism.

The M1-era list above (JSON sidecar, 3 manual slots, colony/robot/codex
payloads) describes the full-game save; it lands with the colony milestones.
The autosave ring supersedes the "autosave ring" clause until then.
