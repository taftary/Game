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

### Debug dimensions section

The `game_debug` single window (ADR-022,
[`../../plans/v0.3.1/unified-debug-view/`](../../plans/v0.3.1/unified-debug-view/))
mounts dimension content per waypoint tab: the Milky Way tab mounts
the Galaxy Map point buffer, the Solar System tab the system rings +
points, the Earth tab the planet fill + wireframe; the other seven
waypoint tabs are placeholders (no 3D pass). Selecting a tab owns
only UI state; it does not update `Journey`, regenerate descriptors,
or reload a system.

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

Physical depth cues (v0.2.0, `plans/v0.2.0/depth-cueing`, ADR-021 and
ADR-003): `engine::render::cue` maps the active `FrameId` to the physical
regime required by spec section 9.1. Atmosphere frames use analytic Rayleigh
and Mie source terms; vacuum frames use zodiacal glow, seeded dust
extinction/reddening, peculiar-velocity tinting, or cosmic-web redshift and
density terms. All outputs are linear-radiance source terms consumed before
exposure. Cosmic-web raymarch budgets are tiered at 64x64/16 steps on Low,
128x128/32 on Medium, and 256x256/64 on High; Low retains an analytic
fallback. The active-frame switch harness is GPU-free and does not alter
camera, projection, picking, ENU, winding, or culling conventions.

## Waypoint transitions (v0.2.0)

`engine::waypoints` maps the ten player-facing dimensions in spec §9.3 onto
the seven stable `FrameId` values. Local ENU altitude bands provide the
interior, exterior, aerial, and Earth waypoints without changing frame
coordinates. `TransitionDescriptor` interpolates exposure keys, cue weight,
zodiacal intensity, haze, sky depth, limb glow, and Sun-disk scale. The
descriptor is pure and deterministic, so manual flight and select-to-focus
can share it.

`engine::render::atmosphere` supplies the atmospheric shell inputs: Rayleigh /
Mie-compatible haze transmission, a blue-to-indigo-to-black altitude ramp,
limb glow, and both the 100 km FAI and approximately 80 km reanalysis
references. The shell shader is descriptor-driven and tiered; Low uses a
bounded linear approximation. It preserves the un-flipped DirectX projection,
NDC `+1` top-row convention, CCW front face, and back-face culling.

The bounded `TransitionEvents` queue is a consumer boundary for the future
ADR-004 autosave feature. The developer-only `game_debug` transition pill
shows the in-flight descriptor bottom-center and the widget Console tab carries the event
log; neither is linked into the release `game` binary.

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

The `game_debug` viewer is the mesh-inspection tool in one OS window
sharing one Vulkan device (ADR-022,
[`../../plans/v0.3.1/unified-debug-view/`](../../plans/v0.3.1/unified-debug-view/),
reversing the `plans/v0.0.1/debug-ui-reorganize` two-window model).
The top bar hosts the Game Demo tab (`F1`), the Dimensions dropdown
(`F2`: ten waypoints on `1`–`0`) and Settings (`F3`). UI draw order is
push order with all solids emitted before any text
(`ui_items_to_vertices`), so overlay menus must be appended **last**:
the Dimensions dropdown composes into its own `UiItems`
(`compose_overlay_ui`), uploaded to a separate vertex buffer and drawn
after every other UI surface with the same pipeline — topmost by GPU
command order, not just push order. It is a
solid-black panel with drop shadow, border, hover lift,
row separators and right-aligned status, hit-tested first for the same
reason (topmost surface wins). The Milky Way /
Solar System / Earth dimension tabs mount the absorbed Galaxy Map,
System Map and Planet View (current `HexSphere` as dual-cell fans
with the shared `OrbitCamera`, a wireframe overlay and a pentagon
highlight, plus an inputs panel with subdivisions 0–8 and live
10·4^N+2 cost hint, validated radius, explicit Regenerate, read-only
stats); the Cosmic Web tab (v0.3.2, ADR-023; cinematic refresh in
update-2026-09-18-2328, smoke in update-2026-09-20-0645 replacing the
update-2026-09-19-1245 ribbons) mounts the
absorbed cosmic-web inspector (instanced smoke-billboard filaments
through the additive smoke `TriangleList` pipeline, grain + gas veil +
3-layer node impostors through the additive glow `PointList`
pipeline, own orbit/pan/zoom camera, live player point). The Game Demo tab renders the same web as the
player-immersive scene (one glow draw + one smoke draw per surface,
buffers relative to an upload origin, camera recentered on the same
origin with a 50 Mpc rebase). Both cosmic views render through an
HDR scene target with a real bloom chain (bright extract + 2-scale
separable blur + ACES resolve composite) on capable devices, LDR
bypass otherwise. The dev widget
(`` ` `` toggle, `F6`–`F8` sub-tabs) hosts FPS
(live frame-health), the Console (fly-to event feed) and the
Inspector (journey summary); a fly-to pill floats bottom-center
while a fly-to leg is in flight (live easing progress, hidden
otherwise), and three toggle buttons
live inside the always-visible top bar (left dock, right dock, dev
widget with FPS value). Viewer state is preserved across screen switches.

Camera & screen-space contract, cosmic extension (v0.3.2, ADR-023):
the space camera (`debug::cosmic_camera`: Chase/Orbit/FirstPerson)
uses the un-flipped `directx::perspective` with MapOrbitCamera-style
distance-derived near/far; the cosmic player marker goes through
`world_to_pixels` + the shared dot/arrow/`YOU` path pin-for-pin with
the sphere marker (FirstPerson hides it by construction — eye-plane
`w ≤ 0`); the inspector player point is map content (one-vertex
point draw), never the UI marker. Points/lines carry no faces, so no
front-face/cull state is involved on either cosmic surface.

Cinematic refresh (update-2026-09-18-2328): the cosmic draws use two
new additive (`One`, `One`, premultiplied in-shader) pipelines —
glow points (rim-zero quadratic-falloff sprite mask, per-sprite
alpha, emissive colors above 1.0 on cluster cores; halos are
world-sized `kind`-1 sprites, cores/grain/glow fixed-pixel `kind` 0)
and braid lines (per-vertex rgba) — both carrying a bounded
exaggerated Hubble redshift tint from view depth (`clip.w` clamped
non-negative and capped, spec §9.1; unbounded tint decorrelated
channels into rainbow squares on screen). The shared alpha `map`/`line`
pipelines and every other surface are untouched. In HDR mode the
same draws record into an offscreen HDR scene pass (indigo clear),
then five dedicated bloom targets (A–E) at half-res: bright extract
→ A, H blur A → B, V blur B → C, wide-H C → D, wide-V D → E (final).
Every target is written exactly once, then only read — never ping-
ponged (Intel UHD 620 corruption workaround, see
`docs/reports/2026-09-19-intel-hdr-bloom-corruption.md`). The
bloom-composite ACES resolve reads scene + E into the swapchain
image; the inspector marker and all UI draw after the resolve. Fullscreen
passes reuse `RESOLVE_VERT` empty-vertex-input triangles with
the `post.rs` NDC-top-row UV contract (`v_uv = vec2(pos.x, 1.0 -
pos.y)`); the bloom GLSL (`BLOOM_BRIGHT_FRAG`, `BLOOM_BLUR_FRAG`,
`resolve_frag_bloom`) and `BloomParams` live in
`engine::render::post`, the GPU half follows the `game_tools`
`ResolvePass` precedent, and transients rebuild with the swapchain.

Smoke filaments (update-2026-09-20-0645, replaces update-2026-09-19-1245
P1 ribbons): `cosmic_web.rs` emits one compact `SmokePuff` per puff
(pos, world diameter, rgba, noise seed — 48 B GPU, ~2.5 MB nominal
for ~52k puffs vs ~1.2M ribbon tris). `main.rs` uploads them as
per-instance vertices; `SMOKE_VERT` expands `gl_VertexIndex` into
camera-facing billboard quads (billboard frame = 1 cross +
normalize, no trig in the vertex shader; centers reuse the same
`braid_point` derivation the grain pass uses, so grain still
textures the smoke), world diameters 2–6.5 Mpc, hub warming baked on
CPU, bounded redshift tint, and a 1→6 Mpc near-eye fade (the player
spawns inside a filament — unfaded puffs fill the screen as white
slabs). `SMOKE_FRAG` applies rim-zero radial falloff × a cheap 4x4
hash dust term (no sin/exp per pixel — mobile fill-rate friendly);
push block `SmokePush` (MVP, eye, px_scale, exposure, redshift —
92 B) carries the per-surface grade. A 2-px minimum-world-size clamp
keeps distant puffs visible in the zoomed-out inspector (without it
they shrink subpixel and vanish). ~104k tris, inside the 500k Low
budget with margin. Known costs/risks: inspector zoom-out stacks
 dozens of puffs/px — mitigated by small alpha with the
 MAP smoke exposure at 0.85 (~5x the retired ribbon MAP grade, paying
 for the billboard area spread) vs 0.65 demo; follow-ups:
 distance/frustum cull, Low grain-budget cut (grain still 800k
 points).

Illustris-look pass (`cosmic-web-illustris-look`, v0.3.2): render-only
enrichment toward the Illustris projection target — no descriptor,
hash, pipeline, or budget change. Strands fray `3–7` per link across
one or two seeded arms (long links `>20 Mpc` split arms, so
sub-threads diverge); smoke puffs become tangent-aligned stretched
sheaths (`3–8 Mpc` long × `0.3–1.0 Mpc` thin, `4×` stretch in-shader,
tighter `0.35 Mpc` jitter, core + faint-halo tiers, junction warming
at degree-`≥3` bifurcations, anisotropic falloff + `8×8` hash dust);
gold beads (`≤60k`, `cosmic_web/bead` stream, spine sub-segments,
mass-graded emissive) string dwarf glitter along threads and ride the
glow `PointList` (pick-ignored); faint-thread alpha floor `0.05 →
0.03` so weak threads sink into the backdrop. Nominal headless:
`smoke51953 grain800000 beads59958 impostors18000`. Bloom write-once,
redshift/near-eye/picking/marker pins all preserved (pinned by
`cosmic_shader_safety_pins`, which keeps the `rl > 1e-10` side
guard and the `1.0 - r2` rim-zero literal plus the `vec3(luma)`
white hot-center pin).

White-smoke pass (2026-09-20, same v0.3.2): visibility + white
sheaths, still render-only (no count, pipeline, or budget change —
nominal headless stays `smoke51953 grain800000 beads59958
impostors18000`). CPU: steep density contrast — base alpha
`0.02+0.13d` and rgb floor dimmed (`0.10/0.12/0.35` at d=0, dense
ceiling unchanged), so faint mist lands darker than the original
grade while dense threads carry ~2x; white subset density-gated
(`mix 0.15+0.55d`, every third core puff — faint smoke stays
blue-dark). GPU: `SMOKE_FRAG` desaturates toward the puff's own
luminance at the quad core (`core=(1-r2)³ × 0.45`,
arithmetic-only); exposures `DEMO 0.65 / MAP 0.85`. Lesson
recorded: falloff peak must stay at 1.0 — a distant puff's pixels
only sample the core, so renormalizing peak brightness brightens
the whole far field (an interim ×1.8 was reverted for exactly this
reason).

Close-up fix (2026-09-20, same v0.3.2): the stretched quads read as
hard paper blades when magnified — three causes, all shader-side.
`SMOKE_FRAG` falloff is now fully dissolving on both axes (`ax²`
tips hit exactly zero instead of cutting at 65%, `radial²` core has
zero slope instead of a tent ridge) with the peak kept at 1.0 — the
far-field grade lives in the CPU alpha + exposure knobs, never in a
peak renormalization (an interim ×1.8 was reverted: far pixels
sample only the core, so it brightened the whole far field); the
`8×8` hash dust is bilinear-smoothed value noise (still fract-only)
so near puffs read as gas texture, not block edges. `SMOKE_VERT`
near fade is size-relative (`0.35×len → 1.25×len`, floor `1→6 Mpc`)
so a puff dissolves before its quad edges resolve on screen; beads,
grain, and impostors keep nearby structure legible.

Palette quick pass (update-2026-09-19-1933): grading-only retune on
the same geometry — no pipeline, topology, or image changes, bloom
write-once rule trivially preserved. Filament braid ramps regraded to
dim-indigo → blue-violet with the alpha floor at 0.05 (faint links
sink to the deepened backdrop `[0.008, 0.005, 0.024]`, voids read
dark) and dense strands premultiplying to ~1.95 in blue, past the
bloom threshold (1.0): the half-res 4-pass blur chain keeps only ~1/4
of a 1-px line's over-threshold energy and dilutes point sources
~1/(2πσ²), so braid emissive and node-core size/brightness must sit
well above threshold to bloom visibly. Strands also warm toward amber
near hub endpoints (convex mix by `1 − √taper`) — the target's
golden-red infusion around clusters. The descriptor glow points
became a **gas veil**: world-sized soft sprites (2.8 Mpc, α 0.045)
hugging the links so filaments sit in faint blue mist. Node impostors
mass-stratify harder (shared `mass_level` ramp re-centered 1e12–3e14
M☉ so ordinary cluster hubs read golden, cores 3–12 px at up to 5.0
emissive on a deep-gold ramp, halos cyan→amber, 2.5–6 Mpc — narrowed
so near-camera halos hit the 256 px point-size clamp as a smaller,
dimmer smudge; the real fix is quad impostors (still open after the
smoke update).
The redshift depth cue softened (`COSMIC_REDSHIFT_PER_MPC` 0.004 →
0.002, saturation 125 → 250 Mpc; red boost 0.75z over blue kill
0.7z, dim 0.45z) so gold survives and distant structures warm like
the target's pink-tinged far filaments; safety clamps unchanged.
Grade knobs are per-surface bin-local consts in
`main.rs` (`COSMIC_DEMO_*` / `COSMIC_MAP_*`): smoke/sprite alpha
exposure (`SmokePush.exposure` scales puff alpha in-shader;
`GlowPush.exposure` already existed) plus resolve
exposure/bloom intensity — the zoomed-out inspector stacks dozens
of puffs per pixel where the immersive demo stacks a few, so one
grade cannot serve both. Engine `BloomParams::spec_defaults()`
(threshold, blur σ) stays untouched.

**v0.3.3 direction (ADR-025, planned 2026-09-20 — nothing below is
implemented yet; each paragraph above is rewritten by the feature
that retires it).** The v0.3.2 cosmic renderer decorates the link
graph (straight `a↔b` segments → smoke quads, grain, beads, 3-per-node
impostors) and cannot produce the reference's curved/branching gas
bodies, walls, dark voids, or hub hierarchy
(`docs/reports/2026-09-19-cosmic-web-visual-architecture.md` § 11).
v0.3.3 renders the **field** instead: `engine::universe::web` exports
a non-hashed `WebField` sidecar (≈ 1M Zel'dovich tracers with a
smoothed overdensity + the 128³ T-web class/density grid;
`web-field-export`); the debug cosmic surfaces draw adaptive-kernel
additive tracer splats coloured by one density ramp
(`cosmic-tracer-splat`, retires grain + beads); a shared window term
— visibility fog `1/(1+(d/L)²)` on every cosmic draw + an inspector
slab mode with a 20° near-orthographic FOV (`cosmic-depth-window`;
the camera contract gains a per-instance `fov_y` on `MapOrbitCamera`
/ `CosmicCamera`, picking and drawing sharing one matrix); mass-rank
hub tiers A/B/C with in-sprite radial core ramps and a member-galaxy
scatter (`cosmic-hub-hierarchy`, retires the 3-per-node impostors); a
write-once mip bloom pyramid whose pass list is asserted by a test —
every image written once, never read by its writer
(`bloom-mip-chain`, retires the 5-target blur); grid-driven gas bodies
and walls as cell sprites on Low and a quarter-res emission-only
raymarch of a 128³ 3D texture on Medium/High (`cosmic-gas-veil-v2`,
retires smoke + the descriptor-glow veil + braid/spine/strand
helpers); and a vista intro that boots the demo on the reference
composition and dives to Chase (`cosmic-vista-intro`). Reproducible
evidence comes first: `game_debug --capture` with four presets
(`cosmic-capture-harness`). Invariants carried unchanged: un-flipped
`directx::perspective`, `ndc = (2u−1, 1−2v)` node-only picking,
`world_to_pixels` marker, bloom/raymarch write-once, hashed engine
paths integer + `sqrt`-only, per-pixel shaders arithmetic-only.
Plans: `plans/v0.3.3/`.

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
- Panel UX (`plans/v0.0.1/debug-ui-reorganize`; shell unified by
  [`../../plans/v0.3.1/unified-debug-view/`](../../plans/v0.3.1/unified-debug-view/)):
  inputs appear only when needed, and hidden docks draw nothing. The
  Earth tab uses a left dock (VIEW section with the 2×2 camera
  presets, SHADER section with the mode selector, OVERLAYS with
  wireframe/pentagons/seams) and the right data dock (INPUTS
  with subdivisions/radius + Regenerate, SELECTION merging the chunk +
  player readouts, read-only STATS), each group under its own section
  bar. The checker-density slider only
  exists in Checker mode, the walk/cam key hints only exist when the
  player is on, and the preset keys only work with planet content.
- Dev widget (ADR-022, replacing the 2026-09-16 tools window): one
  fixed overlay with FPS / Console / Inspector sub-tabs (`` ` ``
  toggle, `F6`–`F8` + click nav). The FPS body shows the live
  `FpsOverlay` recorder (one sample per event-loop iteration): fps,
  mean/max frame ms, sample count, and a 120-sample sparkline (right
  = newest, green <20 ms, yellow <34 ms, red above). Console carries
  the transition-event log feed; Inspector shows the journey summary
  (console direction stays ADR-009's `tracing` layer).
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
