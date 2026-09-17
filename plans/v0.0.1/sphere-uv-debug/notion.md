# Notion — sphere-uv-debug

## Status

`done` (plan: [`plan.md`](plan.md) — all DoD criteria checked)

## Context

`debug-sphere-viewer` is `done`: `game_debug` renders the current `HexSphere`
dual-cell mesh with orbit camera, wireframe + pentagon toggles, validated
subdivisions/radius inputs and read-only stats. There is no way to see how
that sphere maps to texture space — no UVs exist anywhere (`PlanetVertex` is
`position/normal/tint` only, `build_fill` emits no `uv`). Texturing work
cannot judge distortion, seams, or island layout. Locked user decisions
(2026-09-15): true icosahedron-net unwrap for v1, panel-top preview thumb with
click/`U`/button swap, full 6 debug shader modes, extend `PlanetVertex` with
`uv`, standard inputs without export.

## Problem & Needs

- Developers need the UV map of the displayed sphere (icosahedron unwrap) next
  to the 3D view to judge distortion and seams before any texturing lands.
- Developers need a one-click swap: UV preview thumb on the right expands into
  the main viewport while the 3D sphere shrinks into the widget, and back.
- Developers need switchable debug shader views (lit, normals, tint, checker,
  seams, gradients) to read relaxation quality, pentagon sites and UV
  distortion from the same mesh.

## Goals

- Icosahedron-net UV unwrap of the displayed `HexSphere` (20 primal faces
  unfolded to a net, subdivided verts via barycentric interpolation; dual
  centers + corners mapped through their primal triangle).
- UV preview widget at the top of the right inputs panel; click, `U` hotkey
  and explicit `[Swap]` button all toggle `ViewFocus` (`SphereMain | UvMain`):
  focused view fills the main viewport, the other renders in the widget rect.
- Full debug shader suite on both views: Lit, Normal, Tint/Valence, UV
  Checker, Seams+Islands, Lon/Lat gradient; checker density slider.
- Standard UV inputs: view selector, shader selector, checker density, seam
  highlight toggle, wireframe-on-UV toggle. No export in v1.

## Non-goals

- No cube-map / equirect / per-cell schemes in v1 (API reserves them).
- No UV export (PNG/PPM) in v1.
- No custom editable GLSL slot in v1.
- No release `game` binary changes; no mobile/touch UX.
- No elevation/surface layers, LOD, or streaming.

## Users / Stakeholders

- Developers (only). Same rule as `debug-sphere-viewer`: strip or gate before
  any release.

## Functional requirements

- `engine::render::PlanetVertex` gains `uv: [f32; 2]`; `SeededPlanet` and
  debug `build_fill` populate it from the icosa-net unwrap. Buffers stay
  bit-identical apart from the new attribute.
- `UvScheme::IcosaNet` pure builder (`crates/engine/src/render/uv.rs` or
  `hexsphere` extension — plan-time call): 20-face net layout, per-vertex
  `uv` + `seam` flag (fan tris straddling an island edge) + `island` id.
  Deterministic across runs/platforms (integer-driven face assignment, fixed
  net table).
- Sphere Viewer layout: panel-top `UV PREVIEW` block (~244px wide thumb +
  caption `UV — click/U to expand` ↔ `3D — click/U to restore` plus an
  explicit `Swap view (U)` button); thumb, caption and button all flip.
  Main viewport renders focused view (3D orbit or flat ortho UV);
  widget rect renders the other via a second viewport (clipped by the
  viewport transform, no scissor state). The orbit camera persists across
  swaps; the flat view is a fixed aspect-fit (no pan/zoom in v1).
- Panel `UV DEBUG` section: `View: [Sphere|UV]` + `[Swap (U)]`, `Shader:
  [Lit|Normal|Tint|Checker|Seams|LonLat]`, `Checker density slider 2..32`,
  `[x] Seams`, `[x] Wire on UV`. Existing subdiv/radius/regen/stats untouched.
- Fill fragment shader takes `mode + checker_density + seam/island` inputs;
  flat UV view uses an ortho pipeline sharing the same fragment (depth
  test on, write off — coplanar islands must not z-fight).
  Wireframe overlay optionally draws on the UV view.

## Non-functional requirements

- All `docs/techstack/quality.md` gates green (`fmt, clippy -D warnings,
  build, test --workspace --all-targets, test --doc, game_debug --headless`).
- N=6 regen stays interactive (< 1 s); UV build is O(cells) with no per-frame
  allocs beyond existing upload pattern.
- Desktop-first Windows/Linux; debug tool has no mobile target.
- `docs/` updated (rendering/architecture rows touched + techstack version
  bump); plan DoD verification with evidence.

## Definition of Done

- [ ] Icosa-net UVs populate every fill vertex; N=0 net has 20 islands, all
  `uv` in `[0,1]`, seam flags only on island borders (unit-tested).
- [ ] UV preview thumb visible at panel top; click, `U` and `[Swap]` each
  toggle main↔widget in both directions; both cameras preserved.
- [ ] All 6 shader modes switch live on both views; checker density visibly
  changes tiling; seams/islands view matches builder flags.
- [ ] `SeededPlanet::to_indexed_mesh` + debug `build_fill` agree on
  positions/indices/tint (existing test extended for `uv`); engine hash tests
  updated or re-pinned with justification.
- [ ] Both binaries run; `game_debug --headless` prints UV stats
  (islands/seams); all quality gates green with evidence in `plan.md`.
- [ ] Docs updated + links verified; `plans/README.md` lifecycle followed
  (notion → plan → implement → done).

## Constraints & Assumptions

- Locked 2026-09-15: icosa-net v1 (not equirect), panel-top thumb (not
  floating), 6 modes, extend `PlanetVertex` (engine change accepted despite
  the old zero-engine-change rule), no export.
- Extending `PlanetVertex` changes the GPU vertex layout: `game_tools` smoke
  and any layout asserts must be updated together; `HexSphere::mesh_hash`
  stays untouched (topology unchanged) but `IndexedMesh` buffers grow.
- Net table is fixed and documented; island packing never depends on hash
  iteration order (`BTreeMap`/sorted).
- Assumes y-down Vulkan NDC / Richtung conventions from `debug-sphere-viewer`
  issues (front-face CW, radial inflation) still hold for the second viewport.

## Open questions (resolved during implementation)

- Net packing: BFS unfold from face 0 (deterministic order), bbox-normalized
  to `[0,1]²` — done, `render/uv.rs::unfold_net`.
- Flat-view pan/zoom: deferred — v1 is fixed aspect-fit (`flat_mvp`), which
  never lies about distortion. Follow-up material.
- `uv` joins `mesh_hash`? No — hash-excluded, `committed_hash_matches`
  still green.
