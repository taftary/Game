# Procedural universe generation

Deterministic, versioned, staged:

1. **Galaxy seed → star systems:** position, spectral class, companion count.
2. **System seed → planets:** count, orbits, type (rocky / desert / ice / volcanic / toxic / oceanic), gravity, atmosphere density/color, resource bias.
3. **Planet seed → surface:** elevation, biomes, water table, POIs, colony site candidates, robot-relevant resources.

Rules:

- Generation version stamped in every save (`universe_version: u32`). Version bump = migration or new game; never silent drift.
- `universe` crate functions are pure: `(seed, version, id) -> descriptor`. No wall-clock, no thread-ID-dependent iteration order in output.
- Heavy surface detail generated lazily per surface chunk around the player, cached, evictable. Surface chunks are groupings (defined in M2) of *cell-chunks* — the hexsphere cell identity (`ChunkId` = cell index, ADR-010 in [`../decisions/`](../decisions/)) — never a renumbering of them.
- Fixed-point or quantized seeds for cross-platform determinism; float noise must be bit-stable or quantized after generation.
- Content IDs (planet/system) are stable strings (`galaxy/seed:…/system:…/planet:…`), safe to store in saves and links.

Planet variety axes (v1): palette, elevation distribution, water/ice coverage, atmosphere color/density, hazard (heat/cold/toxic/radiation), resource table, gravity modifier (narrow range so controls stay consistent).
