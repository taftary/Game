# Issue specs — cosmic-scale-player / issue-2026-09-19-1958-release-fill-pipeline-panic (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`draft`

## Roles

Reported by (role): DEV (found during update-2026-09-19-1933
screenshot iteration) · Priority set by PO: _(pending)_

## Observed vs Expected

- **Observed**: `game_debug` built with `--release` panics at startup
  during pipeline creation:
  `crates\debug\src\main.rs:3454:10: fill vertex layout must match
  shader: the shader interface contains an input variable named
  "vertex_input_0" (location 0, component 0), but no such attribute
  exists in the vertex definition`.
- **Expected**: release binary boots to the viewer exactly like the
  debug binary.

## Reproduction steps

1. `cargo build -p game_debug --release` on `v0.3.2` HEAD (`128ab27`,
   clean tree — verified with all local changes stashed).
2. Run `target/release/game_debug.exe` (windowed, Intel UHD 620,
   driver 1656914, Vulkan 1.3.215).
3. Panic after boot logs ("cosmic visual build r2" tag prints), before
   window creation. Debug build (`cargo run -p game_debug`) with the
   identical tree runs fine.

## Scope & Impact

- Release builds of the debug shell are unusable (all screens — the
  panic is in `build_fill_pipeline`, before any view draws).
- Debug builds unaffected; `--headless` unaffected (GPU-free path).
- Not caused by update-2026-09-19-1933 (palette knobs): reproduced on
  stashed HEAD; that update's changes never touch FILL shader/vertex
  code.

## Logs / Evidence

```
thread 'main' panicked at crates\debug\src\main.rs:3454:10:
fill vertex layout must match shader: the shader interface contains an
input variable named "vertex_input_0" (location 0, component 0), but
no such attribute exists in the vertex definition
```

Boot log (release, HEAD): adapter "Intel(R) UHD Graphics 620",
api 1.3.215, driver 1656914; last info line before panic:
`cosmic visual build r2: rim-zero falloff + bounded tint + world
halos + light rebalance + resolve clamp`.

## Suspected area

`build_fill_pipeline` (`main.rs:3453-3468`) —
`FillVertex::per_vertex().definition(&vs)` reflects on the naga
SPIR-V of `FILL_VERT`. The `vertex_input_0` fallback naming suggests
naga emits different (unstripped vs. stripped/renamed) interface
variable names under release compilation of the workspace — possibly
profile-dependent `debug-assertions`/`opt-level` behavior in the
naga GLSL frontend or vulkano's `VertexDefinition` name matching.
Check whether `definition()` matches by name when it should match by
location, and whether naga `WriterFlags` differ between profiles.
`compile_shader` in `crates/engine/src/render/shaders.rs` is the
single funnel.
