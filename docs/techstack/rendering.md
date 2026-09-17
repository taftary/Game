# Rendering (custom vulkano + Vulkan)

## Backend

- `vulkano` owns Instance → Surface (`winit` handle) → Device/Queues → Swapchain; physical device + driver logged at startup.
- All shaders authored GLSL, compiled to SPIR-V by `naga` (`glsl-in` → `spv-out`); no external `shaderc`, no WGSL in the runtime path.
- Dev builds enable Vulkan validation layers; the `tools` renderer smoke must boot on Windows/Linux and report Instance / adapter / driver.
- Swapchain recreation (resize, orientation, backgrounding) is explicit; stale swapchain is never fatal.
- No optional Vulkan extensions in gameplay-critical code; extras are tier-gated and probed at runtime.
- Winding convention: `HexSphere` meshes are CCW-outward in world
  space, and the projection is framebuffer-true (no Y-flip — see the
  conventions section below), so culling pipelines use
  `FrontFace::CounterClockwise` with `CullMode::Back`. Both windowed
  binaries (`game_debug` fill, `game_tools` planet) follow it —
  `plans/v0.0.1/debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored`
  (historical `Clockwise` compensation, superseded by
  `plans/v0.0.1/debug-player-view/issue-2026-09-16-0851-3d-view-y-flipped`).
- Orbit input convention (`OrbitCamera::rotate`, shared): drag up
  pitches the camera toward the sphere's top (FPS-style non-inverted),
  drag right yaws with the drag.

## Camera & screen-space conventions (binding)

These rules are the contract every camera/projection/picking/marker
change must preserve. Each is pinned by named tests; breaking one
fails the suite. History: three bugs (`issue-2026-09-14-2113`,
`issue-2026-09-15-1144`, `issue-2026-09-16-0851`) came from this
convention existing nowhere in writing.

1. **Framebuffer:** NDC `+1` = **top** row, `-1` = bottom (y-down
   pixels). Proven by the upright UI (`ortho_matrix` maps pixel row 0
   to NDC +1, pinned by `ortho_maps_corners`).
2. **Projection:** `OrbitCamera::projection_matrix` (shared by global
   and player cameras, both binaries) is glam
   `directx::perspective` — right-handed, Z ∈ [0, 1], **no Y-flip**.
   Never `vulkan::perspective` (it bakes `yy = −h` and mirrored every
   3D view vertically). Never hand-rolled Y-negations either.
3. **Winding follows the projection:** un-flipped projection ⇒
   `FrontFace::CounterClockwise` + `CullMode::Back`. Flip either the
   projection or the front-face, never both, never neither.
4. **World/tangent frame:** `game::player` maps lon/lat to world as a
   true **ENU** frame (`east × north == up`; `z = −R·cos(lat)·sin(lon)`).
   Headings are compass bearings (0 = north, + toward east, clockwise
   seen from above), and turn input must steer toward the avatar's own
   right (`facing × up` is screen-right in FirstPerson).
5. **Camera defaults:** Follow opens **south of a north-facing player,
   looking north** (thrust walks up-screen, east right-screen).
   FirstPerson looks along the heading with up = surface normal; the
   player marker is hidden there by design (screen-up IS the heading).
6. **Picking (inverse path):** `ray_from_cursor` unprojects with
   `ndc = (2u−1, 1−2v)` on the sphere viewport. Any new unproject must
   use the same signs.
7. **Markers (forward path):** world → pixels goes only through
   `world_to_pixels` (sphere player marker dot + arrow tip) — never a
   re-derived or snapped mapping.

Pinning tests: `projection_uses_vulkan_ndc` (engine),
`tangent_frame_is_east_north_up` + `turn_right_rotates_toward_own_right`
(player frame), `fill_faces_point_outward` (mesh),
`ortho_maps_corners` (UI ortho),
`ray_from_cursor` round-trips (picking),
`follow_marker_arrow_visible_and_oriented` +
`first_person_hides_marker_and_looks_along_heading` (markers),
`defaults_build_high_tier_stats` (mesh stats).

## Passes (v1)

- Planet far-field: `HexSphere` dual mesh (`engine::hexsphere`, ADR-002 in [`../decisions/`](../decisions/)) at a tier-chosen subdivision level, height-displaced.
- Terrain near-field: chunked heightfield mesh, triplanar-ish texturing, no heavy PBR in low tier.
- Atmosphere: analytic scattering shell (cheap approximation on mobile tier).
- Sky: gradient + sun + haze; clouds are billboard/impostor tier in v1, volumetric is out.
- Colony/robots: instanced low-poly meshes, one directional light + hemisphere ambient in low tier.
- Water/lava (if any): flat shaded planes with animated normals in v1; no FFT oceans.
- Post: tone map (shipped v0.2.0: HDR scene target + ACES resolve,
  `exposure-tone-mapping`); bloom/shadows are quality-tier gated.

## Depth bands & log-depth (v0.2.0, `plans/v0.2.0/log-depth-rendering`, ADR-016 binding)

26 decades of scale cannot fit one depth range. Two mechanisms combine;
both change **depth writes only** — the Camera & screen-space
conventions above (NDC +1 = top, un-flipped RH projection,
`CounterClockwise` + `Back`) hold inside every band.

- **Log-depth within a pass** (`engine::render::depth`): Outerra-style
  `log(z)` in the vertex shader, Vulkan Z ∈ [0, 1] form
  `d(w) = log2(1 + w) / log2(1 + far)`. Shader sources are composed
  from the single-authored base (`planet_vert_logdepth`), never
  hand-duplicated. Log passes use `D32_SFLOAT` (mandatory Vulkan 1.1
  format): D16 quanta are coarser than the log slope at decade range
  (pinned by `oracle_log_depth_needs_float_buffer_at_decade_range`).
- **Multi-pass compositing across passes** (`engine::render::bands`):
  bands execute far → near; every band clears depth (no band inherits
  another band's values) and color composites with load/preserve.
  Cross-band occlusion is painter order; within a band, depth resolves.

Bucket rule (the per-pair-of-scales sharing table, executable as
`bucket_for` / `shares_depth_pass`): a layer's home-frame depth vs the
active-frame depth decides its pass — coarser homes share Far, the
active home owns Mid, finer homes share Near. Backdrop (no depth) leads,
UI (no depth) trails. No global depth range exists.

| Active frame | Backdrop | Far (log) | Mid (log) | Near (tight linear) |
|---|---|---|---|---|
| Cosmological | skybox-like | — (nothing coarser) | `(0.1, 3e26)` | `(0.1, finest content extent)` |
| Galactocentric | skybox-like | `(1e21, 3e26)` | `(0.1, 1e21)` | `(0.1, finest)` |
| LocalGroup | skybox-like | `(3e23, 3e26)` | `(0.1, 3e23)` | `(0.1, finest)` |
| StellarNeighborhood | skybox-like | `(1e18, 3e26)` | `(0.1, 1e18)` | `(0.1, finest)` |
| SolarSystem | skybox-like | `(1e15, 3e26)` | `(0.1, 1e15)` | `(0.1, finest)` |
| Planetocentric | skybox-like | `(1e9, 3e26)` | `(0.1, 1e9)` | `(0.1, finest)` |
| LocalEnu | skybox-like | `(1e5, 3e26)` | `(0.1, 1e5)` | — (nothing finer) |

Ranges in meters, Low-tier defaults (`BandConfig::low_defaults`:
`near_plane` 0.1 m, `log_far` 3e26 m); exact boundaries per tier stay
override hooks until ADR-007 device data lands. Worked example: at
active SolarSystem, galaxy content (Galactocentric) and stellar content
share the Far pass, planets own Mid, lander geometry shares Near —
`plan_passes` emits exactly `[Far, Mid, Near]`, all depth-cleared.

Pipeline-kind classification (existing consumers): map `PointList`
stays a no-depth-write Backdrop band; planet fill is Mid/Far content;
line overlays ride their home scale's pass (depth-tested, no write);
UI is the trailing pass. The `game_tools` smoke demonstrates the plan
on GPU behind `--log-depth`: Backdrop clear → Mid log planet (D32F) →
Near linear quad (tight 0.05–10 projection, own depth clear).

Catalog star layer (v0.2.0, `plans/v0.2.0/star-catalog-streaming`,
ADR-017 binding): `engine::render::stars` expands resident tiles plus
deterministic fallback slots to camera-relative point sprites on a
900-unit shell (Backdrop band, drawn ahead of content; GPU buffer
re-uploads only on tile-set change, camera motion rides the MVP push).
First live surface is the `game_debug` Planet View backdrop (reuses the
map `PointList` pipeline; tile state shows in the STATS dock, dev-only).
Sky frame is J2000 equatorial axes; photometric calibration landed
with `exposure-tone-mapping` (`PHOTOMETRIC_ZERO_POINT` anchor; the
twilight star fade-in drives the sky alpha path in the Planet View).

Interplanetary sky glow (v0.2.0, `plans/v0.2.0/zodiacal-light`,
ADR-021 binding): the solar-system frame's real faint dust glow is the
analytic spec §9.4 model in `engine::render::zodiacal` (per-pixel from
the camera ray in ecliptic coordinates, linear radiance out). First
consumer is the exposure path (`calibrate` scales `ZodiacalSource`
output by the photometric zero point); no sky renderer consumes it
directly yet.

Exposure post chain (v0.2.0, `plans/v0.2.0/exposure-tone-mapping`,
ADR-021 binding): the scene renders into an HDR color attachment
(`R16G16B16A16_SFLOAT` preferred, `B10G11R11_UFLOAT_PACK32` fallback,
content-preserving LDR bypass when neither is renderable —
`engine::render::post::select_hdr_format`); a fullscreen resolve pass
tone maps into the swapchain (ACES fit, `resolve_frag_aces` composed
from the fixed variant — never hand-duplicated). Auto-exposure keys to
the dominant source (Sun / albedo / starlight, hysteresis-filtered)
with rate-split adaptation (dark slower than light) and twilight star
fade-in (`engine::render::exposure`; absolute anchor
`PHOTOMETRIC_ZERO_POINT`). Pass order: bands → resolve → UI (UI stays
LDR, never tone mapped); the resolve triangle disables culling and
preserves NDC +1 = top (orientation contract pinned in `post`). The
`game_tools` smoke demonstrates the chain behind `--hdr` (`H` cycles
demo keys); the `game_debug` Planet View applies the kernel to the sky
alpha path (`F5` cycles twilight stages for DoD-2 captures).

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

M1 status (2026-09-14, `plans/v0.0.1/renderer-smoke`): the smoke exists and is
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
  generation + stats print + exposure self-test lines
  (`exposure_handoff`, `twilight_fade`, `tonemap_clip`); runs in CI as
  `cargo run -p game_tools -- --headless --tier low`.
- `--hdr` (windowed): HDR scene target + ACES resolve pass
  (`exposure-tone-mapping`); `H` cycles Sun-day → Albedo →
  Starlight-night demo keys through the live adaptation loop.
- `resolution_scale` is a tier parameter (0.6 / 0.85 / 1.0) but stays
  unapplied until M6 dynamic resolution; shadows are a tier flag only.

## Debug planet view (`game_debug`)

The `game_debug` viewer is the mesh-inspection tool, spread over two
OS windows sharing one Vulkan device (`plans/v0.0.1/debug-ui-reorganize`,
2026-09-16). The viewer window hosts the Galaxy Map (`F1`), the System
Map (`F2`) and the Planet View (`F3`: current `HexSphere` as dual-cell
fans with the shared `OrbitCamera`, a wireframe overlay and a pentagon
highlight, plus an inputs panel with subdivisions 0–8 and live
10·4^N+2 cost hint, validated radius, explicit Regenerate, read-only
stats). The tools window hosts FPS (live frame-health), Console and
Inspector (placeholders), on window-local `1/2/3` tabs; closing it
hides it, `F4` on the viewer window reopens it. Viewer state is
preserved across screen switches.

`plans/v0.0.1/debug-sphere-viewer` status (2026-09-14): implemented against
the M1 `engine::render` APIs (`OrbitCamera`, `PlanetVertex`,
naga compile helper, 1.1-floor boot).

- Windowed: `winit` windows + `vulkano` boot mirroring the tools smoke,
  viewer-owned pipelines (fill with highlight flag and per-cell tint,
  translucent `LineList` wireframe with radial inflation, UI textured
  quads), D16 depth attachment,
  `fontdue` atlas from the vendored `assets/fonts/DejaVuSans.ttf`,
  viewport-clipped 3D + full-window UI in one render pass per window,
  swapchain recreation on resize, adapter + mesh stats logged at
  startup. Shader modules compile once; each window builds its own
  pipelines from them plus its own surface/swapchain/framebuffers and
  atlas descriptor set. The windowed default opens at
   N=4 (readable faces + pentagon sites; N=6 cells are subpixel —
   `plans/v0.0.1/debug-sphere-viewer/update-2026-09-14-2008`). The fill pipeline
   follows the Backend winding convention (`FrontFace::CounterClockwise`)
   and passes the radial outward normal through unflipped
   (`plans/v0.0.1/debug-sphere-viewer/issue-2026-09-14-2113-faces-inverted-orbit-mirrored`,
   projection un-flipped in
   `plans/v0.0.1/debug-player-view/issue-2026-09-16-0851-3d-view-y-flipped`).
- `--headless`: GPU-free viewer-mesh build (N=6, R=1.0) + stats print,
  including `uv_islands`/`uv_seam_verts` from the icosa-net unwrap;
  runs in CI as `cargo run -p game_debug -- --headless`.
- UV inspection (`plans/v0.0.1/sphere-uv-debug`, 2026-09-15): `engine::render::uv`
  lays the 20 base faces out as the classic 5-10-5 triangle strip
  (closed-form absolute slots, one rooted tree walk assigning the forced
  icosa neighbor per slot — pairwise SAT-tested overlap-free); every
  `PlanetVertex` carries `uv`, the debug sidecar adds seam/island flags.
  The Planet View offers six fragment-shader modes (Lit, Normal, Tint,
  Gnomonic Checker with density slider, Seams+Islands, LonLat;
  `1`–`6` select). The 3D view keeps the single-UV honest-stretch look.
  Checker mode uses a cube-domain mapping
  (`plans/v0.0.1/sphere-uv-debug/issue-2026-09-15-0817-3d-checker-gnomonic`):
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
  it flows across the icosa seam overlay.
   `PlanetVertex` grew its `uv` attribute; the engine planet shader ignores
   it, so the `game_tools` smoke is unaffected.
- Panel UX (`plans/v0.0.1/debug-ui-reorganize`): inputs appear only when
  needed. The planet screen uses a left dock (VIEW section with the 2×2
  camera presets, SHADER section with the mode selector, OVERLAYS with
  wireframe/pentagons/seams) and the right data dock (INPUTS
  with subdivisions/radius + Regenerate, SELECTION merging the chunk +
  player readouts, read-only STATS), each group under its own section
  bar. The checker-density slider only
  exists in Checker mode, the walk/cam key hints only exist when the
  player is on, and the preset keys only work on the planet screen.
- Tools window (`plans/v0.0.1/debug-ui-reorganize`, 2026-09-16): a second OS
  window hosts the FPS / Console / Inspector tabs (window-local `1/2/3`
  + click nav). The FPS tab shows the live `FpsOverlay` recorder (one
  sample per event-loop iteration): fps, mean/max frame ms, sample
  count, and a 120-sample sparkline (right = newest, green <20 ms,
  yellow <34 ms, red above). Console and Inspector stay placeholders
  (console direction is still ADR-009's `tracing` layer).
- Flat chunk map (`plans/v0.0.1/chunk-flat-view`, CANCELLED 2026-09-16 by
  `plans/v0.0.1/debug-ui-reorganize`): the viewer flat view (third
  `ViewFocus::ChunkFlat` mode, `CHUNK_FLAT_VERT` pipeline, arrow-key
  orbit, flat picking, flat player marker) is deleted. The engine side
  stays: `engine::render::chunk_flat` still projects one hemisphere at
  a time onto its tangent plane (Lambert azimuthal equal-area,
  fully-inside cells only, island/seam sidecars on the `base_face_ids`
  rule) and player-mode streaming still consumes `visible_hemisphere`.
- `engine::hexsphere` topology and the release `game` binary are untouched; all
  UI code lives in `crates/debug` (first custom swapchain-UI consumer
  per [`stack.md`](stack.md)).
- Universe maps (`plans/v0.0.1/universe-maps`, 3D views in
  `plans/v0.0.1/universe-maps-3d`, 2026-09-16): a `PointList` map pipeline
  (one static buffer, per-vertex color/size/kind, circular
  `gl_PointCoord` mask, alpha blend, no depth write) draws the Galaxy
  Map (L1 backdrop + nebula impostors + 25k star points) and the System
  Map star/planets (orbit rings ride the existing line pipeline with
  the map view-projection); orbit/pan/zoom ride push constants, never
  the buffers. Both maps share one `MapOrbitCamera` (debug lib, pure:
  target/distance/yaw/pitch, per-map clamps; default south-approach
  tilt, `Home` top-down toggle) under the un-flipped
  `directx::perspective` projection. World embedding is
  `(east, up, −north)` — the engine ENU/player frame — so the
  top-down snap reads exactly like the old 2D map (north up, east
  right). Vertices are `vec3` world positions (stars carry the real
  disk thickness from `position_ly[1]`; system orbits add
  presentation-only inclinations ≤ 10° + ascending nodes off a
  stream-free index hash — never in descriptors); world-sized sprites
  scale by `px_scale / clip.w` in the vertex shader. Picking projects
  every candidate through the same view-projection
  (`project_to_screen`, NDC +1 = top, `w ≤ 0` rejected) and takes the
  nearest within 8/10 px, lowest index wins ties. `FillPush` grew an arrival tint (`tint_rgb` right after
  the MVP so `repr(C)` and std430 pack identically, 104 B total — still
  under the 128 B floor): the orbit arrival re-lights the Lit planet
  with the target's atmosphere color and tints the clear color, all
  descriptor-driven, no per-type shader branches.
