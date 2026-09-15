# Rendering (custom vulkano + Vulkan)

## Backend

- `vulkano` owns Instance → Surface (`winit` handle) → Device/Queues → Swapchain; physical device + driver logged at startup.
- All shaders authored GLSL, compiled to SPIR-V by `naga` (`glsl-in` → `spv-out`); no external `shaderc`, no WGSL in the runtime path.
- Dev builds enable Vulkan validation layers; the `tools` renderer smoke must boot on Windows/Linux and report Instance / adapter / driver.
- Swapchain recreation (resize, orientation, backgrounding) is explicit; stale swapchain is never fatal.
- No optional Vulkan extensions in gameplay-critical code; extras are tier-gated and probed at runtime.
- Winding convention: `HexSphere` meshes are CCW-outward in world space,
  but `OrbitCamera::projection_matrix` outputs Y-down NDC (glam
  `vulkan::perspective`) and Vulkan classifies front faces in
  framebuffer space — the projection's Y-flip mirrors winding, so
  culling pipelines must use `FrontFace::Clockwise` with
  `CullMode::Back`. Both windowed binaries (`game_debug` fill,
  `game_tools` planet) follow it —
  `plans/debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored`.
- Orbit input convention (`OrbitCamera::rotate`, shared): drag up
  pitches the camera toward the sphere's top (FPS-style non-inverted),
  drag right yaws with the drag.

## Passes (v1)

- Planet far-field: `HexSphere` dual mesh (`engine::hexsphere`, ADR-002 in [`../decisions/`](../decisions/)) at a tier-chosen subdivision level, height-displaced.
- Terrain near-field: chunked heightfield mesh, triplanar-ish texturing, no heavy PBR in low tier.
- Atmosphere: analytic scattering shell (cheap approximation on mobile tier).
- Sky: gradient + sun + haze; clouds are billboard/impostor tier in v1, volumetric is out.
- Colony/robots: instanced low-poly meshes, one directional light + hemisphere ambient in low tier.
- Water/lava (if any): flat shaded planes with animated normals in v1; no FFT oceans.
- Post: tone map; bloom/shadows are quality-tier gated.

## Quality tiers

| Tier | Target | Resolution | Shadows | Terrain density | Atmosphere |
|---|---|---|---|---|---|
| Low (phones) | 30 fps sustained | ≤1080p, dynamic scale down to 0.6x | off / blob | aggressive LOD, short view distance | cheap approx |
| Medium (tablets / Deck-class / low desktop) | 30–60 fps | 1080p–1440p dynamic | single cascade or off | medium chunks | full approx |
| High (desktop) | 60 fps+ | native + TAA-less | cascaded | dense, long view | full + extras |

Features must run on Low to ship. High-only effects are never load-bearing.
Enforced by the `tools` renderer smoke + device profiles (see [`quality.md`](quality.md)).

## Renderer smoke (`tools`)

The `game_tools` renderer smoke is the renderer test: open a seeded planet, orbit it, force all three tiers.

M1 status (2026-09-14, `plans/renderer-smoke`): the smoke exists and is
CI-gated headlessly.

- Windowed: `winit` window + `vulkano` boot (`Instance` capped at Vulkan
  1.1 + portability enumeration → `Surface` → `Device`/queues →
  `Swapchain`), tier planet (`engine::render::SeededPlanet`) uploaded to
  GPU vertex/index buffers, single-pass render pass, naga-compiled planet
  pipeline (flat-shaded, backface-culled — the convex planet needs no
  depth buffer yet — MVP push constants), drag-orbit / wheel-zoom
  `OrbitCamera`, keys `1/2/3` force Low/Med/High live, swapchain
  recreation on resize, adapter + driver + tier logged at startup.
- Tier planet levels: Low N=3 (642 cells / 3,840 tris), Medium N=4
  (2,562 cells / 15,360 tris), High N=6 (40,962 cells / 245,760 tris).
- `--headless`: GPU-free (never loads the Vulkan loader) planet
  generation + stats print; runs in CI as
  `cargo run -p game_tools -- --headless --tier low`.
- `resolution_scale` is a tier parameter (0.6 / 0.85 / 1.0) but stays
  unapplied until M6 dynamic resolution; shadows are a tier flag only.

## Debug sphere viewer (`game_debug`)

The `game_debug` Sphere Viewer is the mesh-inspection tool: it renders
the current `HexSphere` (dual-cell fans) with the shared `OrbitCamera`,
a wireframe overlay and a pentagon highlight, plus an inputs panel
(subdivisions 0–8 with live 10·4^N+2 cost hint, validated radius,
explicit Regenerate, read-only stats) and FPS/Console/Inspector
placeholder screens (F1–F4 / click nav, viewer state preserved).

`plans/debug-sphere-viewer` status (2026-09-14): implemented against
the M1 `engine::render` APIs (`OrbitCamera`, `PlanetVertex`,
naga compile helper, 1.1-floor boot).

- Windowed: `winit` window + `vulkano` boot mirroring the tools smoke,
  three viewer-owned pipelines (fill with highlight flag and `flat`
  per-cell tint, translucent `LineList` wireframe with depth bias, UI
  textured quads), D16 depth attachment, `fontdue` atlas from the
  vendored `assets/fonts/DejaVuSans.ttf`, viewport-clipped 3D +
  full-window UI in one render pass, swapchain recreation on resize,
  adapter + mesh stats logged at startup. The windowed default opens at
  N=4 (readable faces + pentagon sites; N=6 cells are subpixel —
  `plans/debug-sphere-viewer/update-2026-09-14-2008`). The fill pipeline
  follows the Backend winding convention (`FrontFace::Clockwise`) and
  passes the radial outward normal through unflipped
  (`plans/debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored`).
- `--headless`: GPU-free viewer-mesh build (N=6, R=1.0) + stats print,
  including `uv_islands`/`uv_seam_verts` from the icosa-net unwrap;
  runs in CI as `cargo run -p game_debug -- --headless`.
- UV inspection (`plans/sphere-uv-debug`, 2026-09-15): `engine::render::uv`
  lays the 20 base faces out as the classic 5-10-5 triangle strip
  (closed-form absolute slots, one rooted tree walk assigning the forced
  icosa neighbor per slot — pairwise SAT-tested overlap-free); every
  `PlanetVertex` carries `uv`, the debug sidecar adds seam/island flags. The viewer shows a panel-top UV preview
  thumb — click, `U` or `Swap view` flips it with the main viewport (both
  cameras preserved) — and six fragment-shader modes (Lit, Normal, Tint,
  Gnomonic Checker with density slider, Seams+Islands, LonLat) shared by
  the 3D and flat pipelines (`1`–`6` select, flat view is aspect-fit so the
  checker never lies). The flat view uses proper seam duplication
  (update-2026-09-15-0730): seam cells (dual centers on base edges/vertices)
  emit one fan per incident island from expanded buffers
  (`render::uv::FlatUnwrap`), so every flat triangle stays inside one
  island — no cross-net stretch — and the UV wireframe clips at island
  boundaries; the 3D view keeps the single-UV honest-stretch look.
  Checker mode uses a cube-domain mapping
  (`plans/sphere-uv-debug/issue-2026-09-15-0817-3d-checker-gnomonic`):
  an icosahedral square grid provably cannot be globally consistent
  (60° vertex holonomy vs 90° grid symmetry — it always shows triangles
  at the 12 vertices and mismatched seams), so the checker runs on the
  Bourke cubemap instead: the fragment shader selects the cube face by
  major axis, projects gnomonically, and remaps to equal angles
  (equiangular cubemap) before tiling — only squares, evenly
  distributed. Per-face axes and color flips
  (`engine::render::checker`, drift-guarded against the shader literals)
  are search-assigned so the grid alternates across 8 of the 12 cube
  edges at every density 2–32; the topologically forced same-color
  faults (odd 3-cycles at cube vertices) are confined to a perfect
  matching of 4 edges. The checker is a pure function of direction, so
  it flows across the icosa seam overlay and both views agree.
  `PlanetVertex` grew its `uv` attribute; the engine planet shader ignores
  it, so the `game_tools` smoke is unaffected.
- Flat chunk map (`plans/chunk-flat-view`, 2026-09-15):
  `engine::render::chunk_flat` projects one hemisphere at a time onto
  its tangent plane (Lambert azimuthal equal-area projection of true
  cell corners, so neighbors share edges and every chunk keeps its
  size; only fully-inside cells load — partial rim chunks are dropped;
  island/seam sidecars reuse the `base_face_ids`
  rule so coloring agrees with the UV net). The viewer gains a third
  `ViewFocus::ChunkFlat` mode (focus cycles sphere → UV → chunk flat;
  both flat focuses pair with the 3D sphere thumb) with a dedicated
  `CHUNK_FLAT_VERT` pipeline reusing `FILL_FRAG` + `FillPush`. Arrow
  keys orbit the hemisphere viewpoint 5° per step (Left/Right = yaw,
  Up/Down = pitch) and reload the flat GPU buffers — chunks leaving
  the half unload, entering chunks load. 2D CPU picking
  (`pick_flat_visible` over the loaded set + `flat_point_from_cursor`
  inverse) keeps hover highlight, click-pin and the CHUNK panel working
  in flat. `--headless` prints a `chunk_flat_*` self-test line
  (hemisphere coverage + antipodal reload + flat pick check).
- `engine::hexsphere` topology and the release `game` binary are untouched; all
  UI code lives in `crates/debug` (first custom swapchain-UI consumer
  per [`stack.md`](stack.md)).
