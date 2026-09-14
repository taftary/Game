# Save format

- Single versioned binary file per run + JSON sidecar for debugging (`save.bin` + `save.json` via `tools` converter).
- Contains: `universe_version`, galaxy seed, visited systems/planets, colony states, robot states, resources, codex, player position/state, game-time.
- Atomic write (write-temp + rename); corrupt save → backup + clean error, never boot-loop.
- Migration: N → N+1 scripts in `engine::save`; unmigratable = explicit "new game required" message.
- No cloud saves in v1; local only. Slots: 3 + autosave ring.
