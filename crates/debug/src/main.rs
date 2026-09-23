//! `game_debug` planet viewer binary.
//!
//! `--headless` runs the GPU-free path (CI-safe — never loads the Vulkan
//! loader): builds the default N=6/R=1.0 viewer mesh through the
//! `game_debug` lib and prints stats.
//!
//! Windowed (default) opens one `winit` window (ADR-022): the top bar
//! hosts GAME DEMO (`F1`), DIMENSIONS (`F2`, dropdown of the ten
//! waypoints on `1`–`0`) and SETTINGS (`F3`). The demo tab renders the
//! current journey layer presentation-accurately with the shipping
//! HUD; the Milky Way / Solar System / Earth dimension tabs mount the
//! absorbed Galaxy Map / System Map / Planet View (orbit camera,
//! filled dual-cell mesh, wireframe overlay, pentagon highlight,
//! cell-chunk hover highlight + click-to-pin, inputs panel,
//! read-only stats). A transition strip shows in-flight fly-to
//! easing progress; the dev widget (`` ` ``, `F6`–`F8` sub-tabs
//! FPS/Console/Inspector) and the corner strip float over every tab.
//! `F9`–`F11` toggle tab bar / left dock / right dock; `Esc` unwinds
//! UI focus (closing the window exits).
//!
//! All screen logic lives in the `game_debug` lib (window- and GPU-free);
//! this binary owns the winit event loop, the graphics pipelines (fill,
//! wireframe lines, UI quads), the depth buffers and the
//! font-atlas texture. `game_engine` and `game` are untouched.
//!
//! Usage: `game_debug [--headless] [--seed N]`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use game::camera::CameraMode;
use game::hud::HudInputs;
use game::journey::{Journey, JourneyEvent, Layer};
use game::transit::{SIM_DT_SECS, Transit, plan_cost};
use game_debug::actions::{Action, ActionGroup, DROPDOWN_ORDER, digit_for_dimension_index};
use game_debug::app::{App as DebugApp, ChromeState, Screen, ViewContent, WidgetTab};
use game_debug::cosmic_camera::cosmic_tip_world;
use game_debug::cosmic_player::CRUISE_SPEED_NOTCH;
use game_debug::fps::{FPS_SPARKLINE, FpsOverlay};
use game_debug::galaxy_map::{DEFAULT_GALAXY_SEED, GalaxyMapView, spectral_color, star_world};
use game_debug::loader::{LoadPlan, LoadSource, LoadStep};
use game_debug::params::{cell_count_hint, parse_radius, parse_subdivisions, subdiv_warning};
use game_debug::picking::{Ray, intersect_sphere, pick_cell, project_to_screen, ray_from_cursor};
use game_debug::planet_viewer::{DebugMode, PlanetViewerState};
use game_debug::player_view::{MoveKeys, PlayerViewState};
use game_debug::sky::{CatalogSky, SKY_ORDER, SkySummary};
use game_debug::system_map::{SystemMapView, arrival_for, orbit_ring_points, planet_slot};
use game_debug::text::GlyphAtlas;
use game_debug::ui::{self, Layout, Rect};
use game_engine::catalog::scheduler::SkyView;
use game_engine::flight::{ShipMode, mode_of};
use game_engine::frames::recenter;
use game_engine::render::{
    BLOOM_DOWN_FRAG, BLOOM_PREFILTER_FRAG, BLOOM_UP_FRAG, BloomDownPush, BloomMarchResolvePush,
    BloomPrefilterPush, BloomUpPush, ExposureParams, FOV_Y, HdrSelection, MAX_PITCH,
    MipBloomParams, OrbitCamera, QualityTier, RESOLVE_VERT, ShaderKind, WORLD_TO_EQUATORIAL,
    compile_glsl_to_spirv, create_instance, device_score, log_physical_device,
    required_device_extensions, resolve_frag_bloom_march, select_hdr_format, star_visibility,
    visible_hemisphere,
};
use game_engine::universe::{WebDescriptor, WebField};
use game_engine::waypoints::WaypointId;
use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferToImageInfo, CopyImageInfo,
    CopyImageToBufferInfo, PrimaryAutoCommandBuffer, RenderPassBeginInfo, SubpassBeginInfo,
    SubpassContents,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags,
};
use vulkano::format::{ClearValue, Format, FormatFeatures};
use vulkano::image::sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo};
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::instance::{Instance, InstanceExtensions};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::color_blend::{
    AttachmentBlend, BlendFactor, BlendOp, ColorBlendAttachmentState, ColorBlendState,
};
use vulkano::pipeline::graphics::depth_stencil::{CompareOp, DepthState, DepthStencilState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::{CullMode, FrontFace, RasterizationState};
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition, VertexInputState};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{
    DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout,
    PipelineShaderStageCreateInfo,
};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader::{EntryPoint, ShaderModule, ShaderModuleCreateInfo};
use vulkano::swapchain::{
    Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo, acquire_next_image,
};
use vulkano::sync::{self, GpuFuture};
use vulkano::{Validated, VulkanError, VulkanLibrary};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// Note: the global allocator switch lives in `game_engine` (feature
// `game_engine/mimalloc`) — exactly one `#[global_allocator]` may exist
// per binary, so this crate never defines its own.

// ---------------------------------------------------------------------------
// Viewer GLSL (naga-compiled at runtime through `engine::render`).
// ---------------------------------------------------------------------------

const FILL_VERT: &str = r##"#version 450
layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in float tint;
layout(location = 3) in vec2 uv;
layout(location = 4) in vec2 dbg;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    vec3 tint_rgb;
    float use_tint;
    float highlight;
    float mode;
    float density;
    float seams_on;
    float hover_cell;
    float pin_cell;
} pc;
layout(location = 0) out vec3 v_normal;
// `flat`: tint/seam/island are per-vertex flags. Fan triangles are
// (center, corner_i, corner_i+1), and Vulkan's default provoking vertex
// is the first one — so every triangle takes its cell center's flags.
layout(location = 1) flat out float v_tint;
layout(location = 2) out vec2 v_uv;
layout(location = 3) flat out float v_seam;
layout(location = 4) flat out float v_island;
layout(location = 5) flat out float v_hover;
layout(location = 6) flat out float v_pin;
layout(location = 7) flat out vec3 v_tint_rgb;
layout(location = 8) flat out float v_use_tint;
void main() {
    gl_Position = pc.mvp * vec4(position, 1.0);
    // Hover/pin ride the provoking vertex like tint/seam/island: fan
    // centers upload first in cell order, so gl_VertexIndex IS the chunk
    // id there. Compared in float (vertex counts stay exactly
    // representable in f32) — uint(negative) is UB in GLSL, and -1.0
    // means "none", which can never match a real index.
    float vid = float(gl_VertexIndex);
    v_hover = (abs(vid - pc.hover_cell) < 0.5) ? 1.0 : 0.0;
    v_pin = (abs(vid - pc.pin_cell) < 0.5) ? 1.0 : 0.0;
    // Orbit arrival tint (uniform push — one value for the whole mesh).
    v_tint_rgb = pc.tint_rgb;
    v_use_tint = pc.use_tint;
    // The normal attribute is radial outward (position / radius) —
    // pass it through. An earlier `-normal` hack lit the far side's
    // inner faces, which were wrongly visible until the fill
    // pipeline's front face matched the Y-down projection
    // (issue-2026-09-14-2113).
    v_normal = normal;
    v_tint = tint * pc.highlight;
    v_uv = uv;
    v_seam = dbg.x;
    v_island = dbg.y;
}"##;

const FILL_FRAG: &str = r"#version 450
layout(location = 0) in vec3 v_normal;
layout(location = 1) flat in float v_tint;
layout(location = 2) in vec2 v_uv;
layout(location = 3) flat in float v_seam;
layout(location = 4) flat in float v_island;
layout(location = 5) flat in float v_hover;
layout(location = 6) flat in float v_pin;
layout(location = 7) flat in vec3 v_tint_rgb;
layout(location = 8) flat in float v_use_tint;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    vec3 tint_rgb;
    float use_tint;
    float highlight;
    float mode;
    float density;
    float seams_on;
    float hover_cell;
    float pin_cell;
} pc;
layout(location = 0) out vec4 f_color;
float hash1(float n) {
    return fract(sin(n * 12.9898) * 43758.5453);
}
vec3 island_color(float i) {
    float h = hash1(i + 1.0);
    return 0.5 + 0.5 * cos(6.28318 * (h + vec3(0.0, 0.33, 0.67)));
}
// Cube-domain checker face table — generated by
// `engine::render::checker::glsl_const_block` (Bourke cubemap +
// equiangular remap). Cube faces are the one domain where a square grid
// is globally consistent (90° vertex holonomy matches the grid
// symmetry; the icosahedron's 60° vertices make it impossible there).
// Per-face axes + color flips are search-assigned so the grid alternates
// across 8 edges and the forced same-color faults (odd 3-cycles at the
// 8 cube vertices) form a perfect matching of 4 edges — no triangles
// anywhere. The debug test `gnomonic_glsl_matches_engine` asserts this
// block verbatim — change the generator, never these literals.
const vec3 GNO_N[6] = vec3[6](vec3(1.000000000e0, 0.000000000e0, 0.000000000e0), vec3(-1.000000000e0, 0.000000000e0, 0.000000000e0), vec3(0.000000000e0, 1.000000000e0, 0.000000000e0), vec3(0.000000000e0, -1.000000000e0, 0.000000000e0), vec3(0.000000000e0, 0.000000000e0, 1.000000000e0), vec3(0.000000000e0, 0.000000000e0, -1.000000000e0));
const vec3 GNO_U[6] = vec3[6](vec3(0.000000000e0, 1.000000000e0, 0.000000000e0), vec3(0.000000000e0, 1.000000000e0, 0.000000000e0), vec3(0.000000000e0, 0.000000000e0, -1.000000000e0), vec3(0.000000000e0, 0.000000000e0, 1.000000000e0), vec3(1.000000000e0, 0.000000000e0, 0.000000000e0), vec3(-1.000000000e0, 0.000000000e0, 0.000000000e0));
const vec3 GNO_V[6] = vec3[6](vec3(-0.000000000e0, -0.000000000e0, 1.000000000e0), vec3(-0.000000000e0, -0.000000000e0, -1.000000000e0), vec3(-1.000000000e0, -0.000000000e0, -0.000000000e0), vec3(-1.000000000e0, -0.000000000e0, -0.000000000e0), vec3(0.000000000e0, 1.000000000e0, 0.000000000e0), vec3(0.000000000e0, 1.000000000e0, 0.000000000e0));
const float GNO_FLIP[6] = float[6](1.000000000e0, 1.000000000e0, 0.000000000e0, 0.000000000e0, 0.000000000e0, 0.000000000e0);
const float GNO_PI4 = 7.853981853e-1;
const float GNO_TWO_OVER_PI = 6.366197467e-1;
void main() {
    int m = int(pc.mode + 0.5);
    vec3 n = normalize(v_normal);
    vec3 sun = normalize(vec3(0.5, 0.8, 0.6));
    float diffuse = max(dot(n, sun), 0.0);
    // Debug visualizations (sphere-uv-debug): 0 Lit (legacy look),
    // 1 Normal, 2 Tint, 3 Gnomonic Checker, 4 Seams+Islands, 5 LonLat.
    vec3 col;
    if (m == 1) {
        col = n * 0.5 + 0.5;
    } else if (m == 2) {
        col = mix(vec3(0.25, 0.45, 0.75), vec3(1.0, 0.8, 0.2), v_tint);
    } else if (m == 3) {
        // Cube-domain checker (Bourke cubemap + equiangular remap):
        // the sphere direction selects its cube face (max dot = major
        // axis), projects gnomonically onto the face plane, and the
        // face-centered coords are remapped to equal angles before
        // tiling — only squares, evenly distributed, alternating across
        // edges everywhere except the 4 matched fault edges.
        int best = 0;
        float best_dot = -2.0;
        for (int f = 0; f < 6; f++) {
            float fd = dot(n, GNO_N[f]);
            if (fd > best_dot) {
                best_dot = fd;
                best = f;
            }
        }
        vec3 rel = n / best_dot - GNO_N[best];
        vec2 local = vec2(dot(rel, GNO_U[best]), dot(rel, GNO_V[best]));
        vec2 g = (atan(local) + GNO_PI4) * (pc.density * GNO_TWO_OVER_PI);
        vec2 cell = floor(g);
        float c = mod(cell.x + cell.y + GNO_FLIP[best], 2.0);
        col = mix(vec3(0.12, 0.13, 0.16), vec3(0.85, 0.87, 0.92), c);
    } else if (m == 4) {
        col = island_color(v_island);
    } else if (m == 5) {
        col = vec3(v_uv.x, v_uv.y, 0.5);
    } else {
        vec3 hex_color = vec3(0.25, 0.45, 0.75);
        vec3 pent_color = vec3(1.0, 0.8, 0.2);
        vec3 base = mix(hex_color, pent_color, v_tint);
        // Orbit arrival tint (universe-maps): the target's atmosphere
        // color re-lights the bare mesh — one uniform value read off the
        // descriptor, no per-type branches anywhere.
        base = mix(base, v_tint_rgb, v_use_tint);
        col = base * (0.25 + 0.75 * diffuse);
    }
    if (pc.seams_on > 0.5 && v_seam > 0.5) {
        col = mix(col, vec3(1.0, 0.15, 0.15), 0.85);
    }
    // Chunk hover/pin (cell-chunks): applied last so the highlight reads
    // in every debug mode; the pin is the stronger, sticky selection.
    col = mix(col, vec3(0.6, 0.95, 1.0), 0.45 * v_hover);
    col = mix(col, vec3(0.6, 0.95, 1.0), 0.75 * v_pin);
    f_color = vec4(col, 1.0);
}";

const LINE_VERT: &str = r##"#version 450
layout(location = 0) in vec3 position;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float inflate;
} pc;
void main() {
    // Radial inflation: wireframe segments are exactly coplanar with the
    // fill fans' corner-to-corner edges, so depth bias alone z-fights on
    // real drivers -- the overlay vanished at grazing angles. A tiny
    // world-space radial push keeps segments visibly above the fill at
    // every subdivision (issue-2026-09-14-2020).
    vec3 world = position * (1.0 + pc.inflate);
    gl_Position = pc.mvp * vec4(world, 1.0);
}"##;

const LINE_FRAG: &str = r"#version 450
layout(location = 0) out vec4 f_color;
void main() {
    // Translucent: at high subdivisions the cells are subpixel and an
    // opaque overlay would bury the faces (every pixel would read as
    // line color). Alpha lets the face shading show through the veil.
    f_color = vec4(0.75, 0.87, 1.0, 0.45);
}";

const UI_VERT: &str = r"#version 450
layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 uv;
layout(location = 2) in vec4 color;
layout(push_constant) uniform PushConstants {
    mat4 ortho;
    float use_tex;
} pc;
layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out float v_use_tex;
void main() {
    gl_Position = pc.ortho * vec4(pos, 0.0, 1.0);
    v_uv = uv;
    v_color = color;
    v_use_tex = pc.use_tex;
}";

const UI_FRAG: &str = r"#version 450
// Note: naga 30 has no combined `sampler2D` globals — separate texture +
// sampler with explicit `set`/`binding` (its own test-suite pattern).
layout(set = 0, binding = 0) uniform texture2D tex;
layout(set = 0, binding = 1) uniform sampler tex_sampler;
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in float v_use_tex;
layout(location = 0) out vec4 f_color;
void main() {
    float ink = texture(sampler2D(tex, tex_sampler), v_uv).r;
    float a = mix(1.0, ink, step(0.5, v_use_tex));
    f_color = vec4(v_color.rgb, v_color.a * a);
}";

// Galaxy/system-map point sprites (universe-maps, universe-maps-3d):
// one static buffer, one draw. Stars use pixel sizes (`kind` 0);
// nebula impostors use world sizes (`kind` 1) scaled per-vertex by the
// perspective divide (`px_scale / clip.w`), so a single upload serves
// every zoom level and tilt. Circular mask via gl_PointCoord; sizes
// clamp to 256 px (documented debug-viewer cap).
const MAP_VERT: &str = r"#version 450
layout(location = 0) in vec3 map_pos;
layout(location = 1) in vec3 color;
layout(location = 2) in vec3 misc;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float px_scale;
    float exposure;
} pc;
layout(location = 0) out vec3 v_color;
layout(location = 1) out float v_alpha;
void main() {
    vec4 clip = pc.mvp * vec4(map_pos, 1.0);
    gl_Position = clip;
    float px = (misc.z < 0.5) ? misc.x : misc.x * pc.px_scale / max(clip.w, 1e-6);
    gl_PointSize = clamp(px, 1.0, 256.0);
    v_color = color;
    // Exposure/visibility multiplier (ETM-006/009): 1.0 everywhere
    // except the Planet-View sky draw, where the twilight stage fades
    // catalog stars per the exposure kernel. Alpha-side so sprite
    // sizes (which encode magnitude) stay untouched.
    v_alpha = misc.y * pc.exposure;
}";

const MAP_FRAG: &str = r"#version 450
layout(location = 0) in vec3 v_color;
layout(location = 1) in float v_alpha;
layout(location = 0) out vec4 f_color;
void main() {
    vec2 d = gl_PointCoord - vec2(0.5);
    if (dot(d, d) > 0.25) {
        discard;
    }
    f_color = vec4(v_color, v_alpha);
}";

// Cosmic glow point sprites (update-2026-09-18-2328): soft Gaussian
// mask (no hard edge — halo impostors fade out), premultiplied
// additive output for the `One, One` blend, and an exaggerated
// Hubble redshift tint from view depth (`clip.w`, spec §9.1: distant
// filaments redden and dim). Cosmic views only — the shared map
// shaders above stay byte-identical for galaxy/system/planet/sky.
/// Shared depth-window GLSL (`cosmic-depth-window`): the authority is
/// `game_debug::cosmic_window::COSMIC_WINDOW_GLSL` — pasted verbatim
/// into `GLOW_VERT`, `SPLAT_VERT`, and `MARCH_FRAG` below (pinned
/// byte-identical by `cosmic_window_snippet_shared` +
/// `march_loop_stays_narrow`; `concat!` cannot
/// take consts, so paste + pin instead of composition).
/// Visibility fog `1/(1+(d/L)²)` × slab window, arithmetic-only
/// (`smoothstep`, `/`, `*`) — no `exp`, no depth-buffer read (points
/// write none). `kind >= 1` (hub impostor halos + cores) gets the
/// 0.25 fog floor so the navigation goal stays visible at any
/// distance.
const GLOW_VERT: &str = r"#version 450
layout(location = 0) in vec3 map_pos;
layout(location = 1) in vec3 color;
layout(location = 2) in vec3 misc;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float px_scale;
    float exposure;
    float redshift;
    float fog_l;
    float slab_center;
    float slab_half;
} pc;

float cosmic_window_vis(float clip_w, float fog_l, float slab_center, float slab_half, int kind) {
    float fog = (fog_l <= 0.0) ? 1.0 : 1.0 / (1.0 + (clip_w / fog_l) * (clip_w / fog_l));
    float slab = (slab_half <= 0.0) ? 1.0 : 1.0 - smoothstep(slab_half - 5.0, slab_half + 5.0, abs(clip_w - slab_center));
    float vis = fog * slab;
    vis = (kind >= 1) ? max(vis, 0.25) : vis;
    return vis;
}

layout(location = 0) out vec3 v_color;
layout(location = 1) out float v_alpha;
layout(location = 2) out float v_kind;
void main() {
    vec4 clip = pc.mvp * vec4(map_pos, 1.0);
    gl_Position = clip;
    // `kind` 1 = world-unit size (halo impostors), scaled by
    // `px_scale` over the perspective divide — the shared map-shader
    // convention; `kind` 0 = fixed pixel size (cores, grain, glow);
    // `kind` 2 = hub core with an in-sprite radial ramp
    // (`cosmic-hub-hierarchy`).
    float px = (misc.z < 0.5) ? misc.x : misc.x * pc.px_scale / max(clip.w, 1e-6);
    gl_PointSize = clamp(px, 1.0, 256.0);
    // Bounded redshift depth: negative view depth clamps to 0 and the
    // exaggerated term caps at 0.5, so both denominators stay >= 1.35
    // — the tint can never divide by zero, flip a channel's sign, or
    // feed Inf/NaN into the additive chain (the visual-issue fix).
    // Coefficients softened in update-2026-09-19-1933: gentle redden,
    // mild dim, so golden hubs survive at depth (red boost kept
    // stronger than the blue kill — distant structures warm like the
    // target's pink-tinged far filaments).
    float z = min(pc.redshift * max(clip.w, 0.0), 0.5);
    vec3 tint = vec3(1.0 + 0.75 * z, 1.0, 1.0 / (1.0 + 0.7 * z));
    float dim = 1.0 / (1.0 + 0.45 * z);
    v_color = color * tint * dim;
    // Depth window (`cosmic-depth-window`): fog × slab, hub floor on
    // `kind >= 1` so the navigation goal never fades out.
    float vis = cosmic_window_vis(clip.w, pc.fog_l, pc.slab_center, pc.slab_half, int(misc.z + 0.5));
    v_alpha = misc.y * pc.exposure * vis;
    v_kind = misc.z;
}";

const GLOW_FRAG: &str = r"#version 450
layout(location = 0) in vec3 v_color;
layout(location = 1) in float v_alpha;
layout(location = 2) in float v_kind;
layout(location = 0) out vec4 f_color;
void main() {
    vec2 d = gl_PointCoord - vec2(0.5);
    // Soft round sprite: exactly 0 at the rim (an exp() floor left
    // faint square corners on screen — the visual-issue fix).
    float t = max(0.0, 1.0 - 4.0 * dot(d, d));
    float fall = t * t;
    vec3 col = v_color;
    // Hub core (`cosmic-hub-hierarchy`, `kind` 2): white-hot center →
    // pale yellow → orange rim inside the sprite (arithmetic-only).
    float r = length(d) * 2.0;
    vec3 core = mix(vec3(1.0, 0.97, 0.85), vec3(1.0, 0.55, 0.25), smoothstep(0.2, 0.5, r));
    col = (v_kind > 1.5) ? core : col;
    f_color = vec4(col * v_alpha * fall, 1.0);
}";

// Cosmic tracer splats (`cosmic-tracer-splat`, ADR-025): one additive
// point sprite per Zel'dovich tracer, kernel size + color from local
// density (the Illustris particle-splat look). The vertex stage may use
// `exp2`/`log2` (per-vertex, not per-pixel); the fragment stays
// arithmetic-only (mobile fill-rate rule). Density ramp stops match
// `cosmic_splat::DENSITY_RAMP_STOPS` (pinned by
// `cosmic_shader_safety_pins`); the vertex unpack mirrors
// `cosmic_splat::splat_pack` bit-for-bit.
const SPLAT_VERT: &str = r"#version 450
layout(location = 0) in vec3 pos;
layout(location = 1) in uint packed;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    vec4 eye;
    float px_scale;
    float exposure;
    float redshift;
    float h0;
    float alpha_k;
    float fog_l;
    float slab_center;
    float slab_half;
} pc;
layout(location = 0) out vec3 v_color;
layout(location = 1) out float v_alpha;
layout(location = 2) out float v_b;
// Shared density ramp (`cosmic-gas-veil-v2` CGV-001): the authority is
// `game_debug::cosmic_veil::COSMIC_DENSITY_RAMP_GLSL` — pasted verbatim
// (pinned byte-identical by `cosmic_density_ramp_shared`; `concat!`
// cannot take consts, so paste + pin instead of composition).
vec3 cosmic_density_ramp(float log2od) {
    vec3 c0 = vec3(0.10, 0.08, 0.35);
    vec3 c1 = vec3(0.35, 0.32, 0.80);
    vec3 c2 = vec3(0.85, 0.85, 1.00);
    vec3 c3 = vec3(1.00, 0.92, 0.60);
    vec3 c4 = vec3(1.00, 0.45, 0.40);
    vec3 col = c0;
    col = mix(col, c1, clamp((log2od - (-2.0)) / (0.0 - (-2.0)), 0.0, 1.0));
    col = mix(col, c2, clamp((log2od - 0.0) / (1.5 - 0.0), 0.0, 1.0));
    col = mix(col, c3, clamp((log2od - 1.5) / (3.0 - 1.5), 0.0, 1.0));
    col = mix(col, c4, clamp((log2od - 3.0) / (4.5 - 3.0), 0.0, 1.0));
    return col;
}

float cosmic_window_vis(float clip_w, float fog_l, float slab_center, float slab_half, int kind) {
    float fog = (fog_l <= 0.0) ? 1.0 : 1.0 / (1.0 + (clip_w / fog_l) * (clip_w / fog_l));
    float slab = (slab_half <= 0.0) ? 1.0 : 1.0 - smoothstep(slab_half - 5.0, slab_half + 5.0, abs(clip_w - slab_center));
    float vis = fog * slab;
    vis = (kind >= 1) ? max(vis, 0.25) : vis;
    return vis;
}

void main() {
    uint q = packed & 65535u;
    float log2od = float(q) / 65535.0 * 16.0 - 8.0;
    uint tint = (packed >> 16u) & 3u;
    uint b = (packed >> 18u) & 1u;
    float od = exp2(log2od);
    // Adaptive kernel h = h0 * (1+d)^(-1/3), clamped [0.5, 4] Mpc.
    float h = clamp(pc.h0 * exp2(-log2(max(od, 1e-3)) / 3.0), 0.5, 4.0);
    vec4 clip = pc.mvp * vec4(pos, 1.0);
    gl_Position = clip;
    float px = clamp(h * pc.px_scale / max(clip.w, 1e-6), 1.5, 64.0);
    gl_PointSize = px;
    // Constant energy per splat: dense clumps are bright because they
    // hold many particles, not because each is bigger.
    float alpha = pc.alpha_k / max(px * px, 1.0) * pc.exposure;
    // Depth window (`cosmic-depth-window`): splats never take the hub
    // floor (kind 0) — fully fogged or out-of-slab vertices add zero.
    alpha *= cosmic_window_vis(clip.w, pc.fog_l, pc.slab_center, pc.slab_half, 0);
    // Near-eye fade (the v0.3.2 white-flash lesson): kernels closer
    // than 2h dissolve instead of filling the screen.
    float dist = length(pos - pc.eye.xyz);
    alpha *= smoothstep(h, 2.0 * h, dist);
    // Bounded redshift depth (the glow-shader treatment, same clamps).
    float z = min(pc.redshift * max(clip.w, 0.0), 0.5);
    vec3 hubble = vec3(1.0 + 0.75 * z, 1.0, 1.0 / (1.0 + 0.7 * z));
    float dim = 1.0 / (1.0 + 0.45 * z);
    // Density ramp + emissive (dense cores cross the bloom threshold)
    // + class-C hub tint toward pink-red.
    vec3 ramp = cosmic_density_ramp(log2od);
    float emissive = 1.0 + 0.5 * max(0.0, log2od - 1.5);
    vec3 tinted = mix(ramp, vec3(1.0, 0.5, 0.55), float(tint) / 3.0 * 0.6);
    v_color = tinted * emissive * hubble * dim;
    v_alpha = alpha;
    v_b = float(b);
}";

const SPLAT_FRAG: &str = r"#version 450
layout(location = 0) in vec3 v_color;
layout(location = 1) in float v_alpha;
layout(location = 2) in float v_b;
layout(location = 0) out vec4 f_color;
void main() {
    vec2 d = gl_PointCoord - vec2(0.5);
    // Rim-zero kernel (the glow-shader falloff, same literal).
    float t = max(0.0, 1.0 - 4.0 * dot(d, d));
    float fall = t * t;
    vec3 col = v_color;
    // Class-B galaxy core: warm 2 px lobe on top of the kernel.
    float core = max(0.0, 1.0 - dot(d, d) * 16.0);
    col = mix(col, vec3(1.0, 0.85, 0.55) * 2.0, core * v_b);
    f_color = vec4(col * v_alpha * fall, 1.0);
}";

// Procedural GPU tracers (`cosmic-gpu-tracers`, ADR-026 §2): no vertex
// input — `gl_VertexIndex` drives the draw (`vertex_count = cells × k`).
// Per invocation: Lagrangian cell from the cell list + sub-sample
// offset → trilinear displacement fetch → Eulerian position, density
// from the R8 veil volume at the Eulerian cell. The fragment is the
// shared `SPLAT_FRAG` (arithmetic-only); this stage may use
// `exp2`/`log2` (per-vertex, not per-pixel — the mobile fill-rate rule
// governs the fragment; vertex-stage fetch is the recorded device
// requirement in `quality.md`, CGT-011).
const SPLAT_PROC_VERT: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture3D disp_tex;
layout(set = 0, binding = 1) uniform sampler disp_sampler;
layout(set = 0, binding = 2) uniform texture3D density_tex;
layout(set = 0, binding = 3) uniform sampler density_sampler;
layout(set = 0, binding = 4) readonly buffer CellList {
    uint cell_indices[];
};
layout(set = 0, binding = 5) uniform ProcParams {
    vec4 origin_cell;
    vec4 radius_k;
} pp;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    vec4 eye;
    float px_scale;
    float exposure;
    float redshift;
    float h0;
    float alpha_k;
    float fog_l;
    float slab_center;
    float slab_half;
} pc;
layout(location = 0) out vec3 v_color;
layout(location = 1) out float v_alpha;
layout(location = 2) out float v_b;
// Shared density ramp (`cosmic-gas-veil-v2` CGV-001): the authority is
// `game_debug::cosmic_veil::COSMIC_DENSITY_RAMP_GLSL` — pasted verbatim
// (pinned byte-identical by `cosmic_density_ramp_shared`; `concat!`
// cannot take consts, so paste + pin instead of composition).
vec3 cosmic_density_ramp(float log2od) {
    vec3 c0 = vec3(0.10, 0.08, 0.35);
    vec3 c1 = vec3(0.35, 0.32, 0.80);
    vec3 c2 = vec3(0.85, 0.85, 1.00);
    vec3 c3 = vec3(1.00, 0.92, 0.60);
    vec3 c4 = vec3(1.00, 0.45, 0.40);
    vec3 col = c0;
    col = mix(col, c1, clamp((log2od - (-2.0)) / (0.0 - (-2.0)), 0.0, 1.0));
    col = mix(col, c2, clamp((log2od - 0.0) / (1.5 - 0.0), 0.0, 1.0));
    col = mix(col, c3, clamp((log2od - 1.5) / (3.0 - 1.5), 0.0, 1.0));
    col = mix(col, c4, clamp((log2od - 3.0) / (4.5 - 3.0), 0.0, 1.0));
    return col;
}

float cosmic_window_vis(float clip_w, float fog_l, float slab_center, float slab_half, int kind) {
    float fog = (fog_l <= 0.0) ? 1.0 : 1.0 / (1.0 + (clip_w / fog_l) * (clip_w / fog_l));
    float slab = (slab_half <= 0.0) ? 1.0 : 1.0 - smoothstep(slab_half - 5.0, slab_half + 5.0, abs(clip_w - slab_center));
    float vis = fog * slab;
    vis = (kind >= 1) ? max(vis, 0.25) : vis;
    return vis;
}

// SNORM displacement range in cells (mirrors
// `engine::universe::web::field_export::DISP_QUANT_RANGE_CELLS`).
const float DISP_SNORM_CELLS = 8.0;
// Fixed sub-cell offsets (CGT-005): k = 1 is the cell centre, else the
// refine() pattern 0.25 + 0.5 * corner-bit (x fastest) — the CPU
// `cosmic_splat::splat_sub_offsets` order, pinned by
// `splat_proc_offset_table` below.
vec3 splat_sub_offset(uint sub, uint k) {
    if (k <= 1u) { return vec3(0.5); }
    return vec3(
        0.25 + 0.5 * float(sub & 1u),
        0.25 + 0.5 * float((sub >> 1u) & 1u),
        0.25 + 0.5 * float((sub >> 2u) & 1u));
}

void main() {
    float n = pp.radius_k.z;
    uint k = clamp(uint(pp.radius_k.y + 0.5), 1u, 8u);
    uint slot = uint(gl_VertexIndex) / k;
    uint sub = uint(gl_VertexIndex) - slot * k;
    uint flat_idx = cell_indices[slot];
    uint ni = uint(n + 0.5);
    vec3 cellvec = vec3(float(flat_idx % ni), float((flat_idx / ni) % ni), float(flat_idx / (ni * ni)));
    // At most 1/4 sub-cell of fract-hash dither (R-2 lattice-imprint
    // mitigation): deterministic per (slot, sub); 0.25 cells at k = 1,
    // 0.125 above.
    float amp = (k <= 1u) ? 0.25 : 0.125;
    float h1 = fract(float(slot) * 0.6180339887 + float(sub) * 0.3819660113);
    float h2 = fract(h1 * 7.980664 + 0.215389);
    float h3 = fract(h2 * 7.980664 + 0.715389);
    vec3 q = mod(cellvec + splat_sub_offset(sub, k) + (vec3(h1, h2, h3) - 0.5) * amp, n);
    // Explicit LOD 0: vertex-stage fetches have no derivatives, so the
    // implicit-`texture()` form is invalid here (naga pin) — and the
    // volumes hold a single mip level anyway.
    vec3 disp = textureLod(sampler3D(disp_tex, disp_sampler), q / n, 0.0).xyz * DISP_SNORM_CELLS;
    vec3 e = mod(q - disp, n);
    float cell = pp.origin_cell.w;
    float half = n * cell * 0.5;
    vec3 box_pos = e * cell - half;
    float radius = pp.radius_k.x;
    // Sphere cull (edge-cell subs can land outside): clipped,
    // zero-size, zero-alpha — the degenerate-draw contract.
    if (dot(box_pos, box_pos) > radius * radius) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        gl_PointSize = 0.0;
        v_color = vec3(0.0);
        v_alpha = 0.0;
        v_b = 0.0;
        return;
    }
    vec3 pos = box_pos - pp.origin_cell.xyz;
    // Density from the R8 veil volume at the Eulerian cell (same
    // packing the march inverts with `q * 10 - 4`).
    float density_q = textureLod(sampler3D(density_tex, density_sampler), e / n, 0.0).r;
    float log2od = density_q * 10.0 - 4.0;
    float od = exp2(log2od);
    // Adaptive kernel h = h0 * (1+d)^(-1/3), clamped [0.5, 4] Mpc.
    float h = clamp(pc.h0 * exp2(-log2(max(od, 1e-3)) / 3.0), 0.5, 4.0);
    vec4 clip = pc.mvp * vec4(pos, 1.0);
    gl_Position = clip;
    float px = clamp(h * pc.px_scale / max(clip.w, 1e-6), 1.5, 64.0);
    gl_PointSize = px;
    // Constant energy per splat: dense clumps are bright because they
    // hold many particles, not because each is bigger.
    float alpha = pc.alpha_k / max(px * px, 1.0) * pc.exposure;
    // Depth window (`cosmic-depth-window`): splats never take the hub
    // floor (kind 0) — fully fogged or out-of-slab vertices add zero.
    alpha *= cosmic_window_vis(clip.w, pc.fog_l, pc.slab_center, pc.slab_half, 0);
    // Near-eye fade (the v0.3.2 white-flash lesson): kernels closer
    // than 2h dissolve instead of filling the screen.
    float dist = length(pos - pc.eye.xyz);
    alpha *= smoothstep(h, 2.0 * h, dist);
    // Bounded redshift depth (the glow-shader treatment, same clamps).
    float z = min(pc.redshift * max(clip.w, 0.0), 0.5);
    vec3 hubble = vec3(1.0 + 0.75 * z, 1.0, 1.0 / (1.0 + 0.7 * z));
    float dim = 1.0 / (1.0 + 0.45 * z);
    // Density ramp + emissive (dense cores cross the bloom threshold).
    // No per-tracer class tint: the cell list carries no hub-proximity
    // word (hubs read through the glow members + impostors instead —
    // grading input for CGT-008).
    vec3 ramp = cosmic_density_ramp(log2od);
    float emissive = 1.0 + 0.5 * max(0.0, log2od - 1.5);
    v_color = ramp * emissive * hubble * dim;
    v_alpha = alpha;
    v_b = 0.0;
}";

/// World-space kernel scale: `h = h0 * (1+d)^(-1/3)` Mpc.
const SPLAT_H0: f32 = 2.0;
/// Constant-energy numerator: `alpha = k / max(px^2, 1)`. Per-surface
/// grade (round 1, 2026-09-20): the zoomed-out inspector stacks the
/// full 500 Mpc depth column (~7× the demo's per-px energy), so the
/// map runs 0.2 while the immersive demo keeps 1.0. `cosmic-depth-
/// window` fog replaces this knob with a real depth term.
const SPLAT_ALPHA_K_DEMO: f32 = 1.0;
/// See [`SPLAT_ALPHA_K_DEMO`].
const SPLAT_ALPHA_K_MAP: f32 = 0.2;

// Cosmic gas-veil raymarch (`cosmic-gas-veil-v2`, Medium/High):
// quarter-res fullscreen emission-only march of the 128³ density
// grid (uploaded once as R8). One `exp2` per step converts the packed
// log-density back to overdensity — the single scoped exception to
// the arithmetic-only fragment rule (recorded in CGV-005: the march
// never runs on Low; sprite fragments stay arithmetic-only). Ray–sphere
// clip, ordered-dither offset, fog + slab in-march, near-eye ramp.
// Reconstructed from the SAME view-projection as the draws (inverse
// of the pushed matrix, NDC top row — pinned by CGV-007).
const MARCH_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture3D grid_tex;
layout(set = 0, binding = 1) uniform sampler grid_sampler;
layout(push_constant) uniform PushConstants {
    mat4 inv_vp;
    vec4 eye_radius;
    vec4 center_cell;
    vec4 origin_steps;
    vec4 window_gain;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
// Shared density ramp (`cosmic-gas-veil-v2` CGV-001): the authority is
// `game_debug::cosmic_veil::COSMIC_DENSITY_RAMP_GLSL` — pasted verbatim
// (pinned byte-identical by `cosmic_density_ramp_shared`).
vec3 cosmic_density_ramp(float log2od) {
    vec3 c0 = vec3(0.10, 0.08, 0.35);
    vec3 c1 = vec3(0.35, 0.32, 0.80);
    vec3 c2 = vec3(0.85, 0.85, 1.00);
    vec3 c3 = vec3(1.00, 0.92, 0.60);
    vec3 c4 = vec3(1.00, 0.45, 0.40);
    vec3 col = c0;
    col = mix(col, c1, clamp((log2od - (-2.0)) / (0.0 - (-2.0)), 0.0, 1.0));
    col = mix(col, c2, clamp((log2od - 0.0) / (1.5 - 0.0), 0.0, 1.0));
    col = mix(col, c3, clamp((log2od - 1.5) / (3.0 - 1.5), 0.0, 1.0));
    col = mix(col, c4, clamp((log2od - 3.0) / (4.5 - 3.0), 0.0, 1.0));
    return col;
}
float cosmic_window_vis(float clip_w, float fog_l, float slab_center, float slab_half, int kind) {
    float fog = (fog_l <= 0.0) ? 1.0 : 1.0 / (1.0 + (clip_w / fog_l) * (clip_w / fog_l));
    float slab = (slab_half <= 0.0) ? 1.0 : 1.0 - smoothstep(slab_half - 5.0, slab_half + 5.0, abs(clip_w - slab_center));
    float vis = fog * slab;
    vis = (kind >= 1) ? max(vis, 0.25) : vis;
    return vis;
}
void main() {
    // NDC from the resolve UV contract (v_uv = (pos.x, 1 - pos.y)).
    vec2 ndc = vec2(v_uv.x * 2.0 - 1.0, (1.0 - v_uv.y) * 2.0 - 1.0);
    vec4 near4 = pc.inv_vp * vec4(ndc, 0.0, 1.0);
    vec4 far4 = pc.inv_vp * vec4(ndc, 1.0, 1.0);
    vec3 near = near4.xyz / max(near4.w, 1e-6);
    vec3 far = far4.xyz / max(far4.w, 1e-6);
    vec3 dir = far - near;
    float dl = length(dir);
    dir = (dl > 1e-6) ? dir / dl : vec3(0.0, 0.0, 1.0);
    vec3 eye = pc.eye_radius.xyz;
    float radius = pc.eye_radius.w;
    vec3 center = pc.center_cell.xyz;
    float cell = max(pc.center_cell.w, 1e-6);
    // Ray–sphere intersect (unit-dir quadratic, CPU-mirrored).
    vec3 oc = eye - center;
    float b = dot(oc, dir);
    float c = dot(oc, oc) - radius * radius;
    float h = b * b - c;
    if (h <= 0.0) { f_color = vec4(0.0); return; }
    float sq = sqrt(h);
    float t0 = max(-b - sq, 0.0);
    float t1 = -b + sq;
    if (t1 <= t0) { f_color = vec4(0.0); return; }
    float steps = max(pc.origin_steps.w, 1.0);
    float dt = (t1 - t0) / steps;
    // Ordered dither hides banding at 32–48 steps (fract only).
    float dither = fract(dot(gl_FragCoord.xy, vec2(0.7548776662, 0.5698402909)));
    float fog_l = pc.window_gain.x;
    float slab_center = pc.window_gain.y;
    float slab_half = pc.window_gain.z;
    float gain = pc.window_gain.w;
    // Grid box: cubic, centered — size from the (negative) origin.
    vec3 origin = pc.origin_steps.xyz;
    vec3 span = max(-2.0 * origin, vec3(1e-6));
    vec3 acc = vec3(0.0);
    // Grade round 2 (2026-09-22): the march is a vis-weighted MEAN, not
    // a column. A column grows with chord length, so the emission `k`
    // graded at the 30 Mpc slab framing washed full-depth views
    // (demo/inspector chords are 100–500 Mpc). Dividing by the
    // integrated weight keeps one grade correct on every view — and
    // softens the sphere-limb edge (short limb chords divide by less).
    float wsum = 0.0;
    for (float i = 0.0; i < 64.0; i += 1.0) {
        if (i >= steps) { break; }
        float t = t0 + (i + dither) * dt;
        vec3 p = eye + dir * t;
        // Outside the grid box: no contribution (clamp edge would smear).
        vec3 guv = (p - origin) / span;
        if (any(lessThan(guv, vec3(0.0))) || any(greaterThan(guv, vec3(1.0)))) {
            continue;
        }
        float q = texture(sampler3D(grid_tex, grid_sampler), guv).r;
        float log2od = q * 10.0 - 4.0;
        float od = exp2(log2od);
        float a = 0.002 * max(0.0, od - 0.5);
        float vis = cosmic_window_vis(t, fog_l, slab_center, slab_half, 0);
        // Near-eye ramp (same rule as splats): never an opaque wash.
        float near_w = smoothstep(0.0, 2.0 * cell, t);
        float w = vis * near_w * dt;
        acc += cosmic_density_ramp(log2od) * (a * w);
        wsum += w;
    }
    vec3 mean = acc / max(wsum, 1e-6);
    f_color = vec4(mean * gain, 1.0);
}";

/// March emission constant: `a = k·max(0, od − 0.5)` (grade round 1,
/// 2026-09-21 — matches the Low sprite body at the slab framing).
/// Baked into `MARCH_FRAG` as the `0.002` literal (GLSL has no Rust
/// consts; the value is pinned by `march_emission_matches_grade`).
/// Test-only anchor in non-test builds.
#[allow(dead_code)]
const VEIL_MARCH_K: f32 = 0.002;
/// March target gain in the march push (`window_gain.w`): passthrough.
/// The march target holds the vis-weighted MEAN density response; the
/// grade lives at the resolve (`VEIL_MARCH_RESOLVE_GAIN`), so the two
/// knobs never compound — grade-round-2 lesson (2026-09-22): one const
/// fed both sides and blew the composite out 900×.
const VEIL_MARCH_GAIN: f32 = 1.0;
/// March composite gain at the resolve (`scene + bloom·i + march·e`,
/// FR4): `30.0` carries the 30 Mpc reference window back in — the mean
/// over a slab sightline (`wsum ≈ 30`) lands exactly where grade
/// round 1 put the column, while full-depth views divide by their own
/// weight instead of washing.
const VEIL_MARCH_RESOLVE_GAIN: f32 = 30.0;

// ---------------------------------------------------------------------------
// GPU types (bin-local, mirroring the `game_tools` `MvpData` pattern).
// ---------------------------------------------------------------------------

/// Position-only wireframe vertex.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct LineVertex {
    #[format(R32G32B32_SFLOAT)]
    position: [f32; 3],
}

/// Combined fill vertex: engine position/normal/tint/uv plus the
/// debug-only seam/island sidecar (`dbg.x` = seam flag, `dbg.y` =
/// island id). Built at upload from `PlanetViewerState`.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct FillVertex {
    #[format(R32G32B32_SFLOAT)]
    position: [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    normal: [f32; 3],
    #[format(R32_SFLOAT)]
    tint: f32,
    #[format(R32G32_SFLOAT)]
    uv: [f32; 2],
    #[format(R32G32_SFLOAT)]
    dbg: [f32; 2],
}

/// UI quad vertex: screen pixels + atlas UV + tint.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct UiVertex {
    #[format(R32G32_SFLOAT)]
    pos: [f32; 2],
    #[format(R32G32_SFLOAT)]
    uv: [f32; 2],
    #[format(R32G32B32A32_SFLOAT)]
    color: [f32; 4],
}

/// Fill push constants: MVP + arrival tint (RGB first: `mat4` ends at a
/// 16 B boundary so the `vec3` packs identically in Rust `repr(C)` and
/// GLSL std430) + pentagon-highlight flag + debug-mode id + checker
/// density + seam overlay flag + hovered/pinned chunk ids (−1.0 = none;
/// 104 B < 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct FillPush {
    mvp: [[f32; 4]; 4],
    tint_rgb: [f32; 3],
    use_tint: f32,
    highlight: f32,
    mode: f32,
    density: f32,
    seams_on: f32,
    hover_cell: f32,
    pin_cell: f32,
}

/// Line push constants: MVP + radial wireframe inflation (68 B < 128 B).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct LinePush {
    mvp: [[f32; 4]; 4],
    inflate: f32,
}

/// Wireframe radial inflation factor (2e-4 of radius ≈ 0.2 mm at R=1).
const LINE_INFLATE: f32 = 2e-4;

/// UI push constants: pixel→NDC ortho + texture-enable flag.
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct UiPush {
    ortho: [[f32; 4]; 4],
    use_tex: f32,
}

/// Galaxy/system-map point vertex: world-space `(east, up, −north)`
/// in map units (compressed ly / AU) + tint + (size, alpha, kind).
/// `kind` 0 = pixel size (stars, backdrop, planets); `kind` 1 =
/// world-unit size scaled by `MapPush::px_scale` over the perspective
/// divide (nebulae).
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct MapVertex {
    #[format(R32G32B32_SFLOAT)]
    map_pos: [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    color: [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    misc: [f32; 3],
}

/// Map push constants: perspective view-projection + pixels-per-unit
/// at the target depth for world-sized sprites + exposure/visibility
/// multiplier (72 B < 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct MapPush {
    mvp: [[f32; 4]; 4],
    px_scale: f32,
    exposure: f32,
}

/// Cosmic glow push constants: MVP + pixel size scale + exposure +
/// exaggerated Hubble redshift strength per Mpc of view depth
/// (update-2026-09-18-2328) + depth-window terms (`cosmic-depth-
/// window`, 88 B < 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct GlowPush {
    mvp: [[f32; 4]; 4],
    px_scale: f32,
    exposure: f32,
    redshift: f32,
    fog_l: f32,
    slab_center: f32,
    slab_half: f32,
}

/// Cosmic tracer-splat vertex (`cosmic-tracer-splat`): origin-relative
/// Mpc position + packed density/class word (see
/// `cosmic_splat::splat_pack`). 16 B — Low holds 300k in 4.8 MB.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct SplatVertex {
    #[format(R32G32B32_SFLOAT)]
    pos: [f32; 3],
    #[format(R32_UINT)]
    packed: u32,
}

/// Cosmic splat push constants: MVP + buffer-frame eye (near-eye
/// fade) + pixel scale + exposure + redshift + kernel scale +
/// constant-energy numerator + depth-window terms
/// (`cosmic-depth-window`, 112 B < 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct SplatPush {
    mvp: [[f32; 4]; 4],
    eye: [f32; 4],
    px_scale: f32,
    exposure: f32,
    redshift: f32,
    h0: f32,
    alpha_k: f32,
    fog_l: f32,
    slab_center: f32,
    slab_half: f32,
}

/// Procedural-splat params UBO (`cosmic-gpu-tracers`, CGT-005): the
/// `SPLAT_PROC_VERT` `ProcParams` block (set 0, binding 5). Two `vec4`s
/// so the Rust `repr(C)` layout matches GLSL `std140` with no padding
/// traps (32 B, pinned by `splat_proc_params_layout`).
#[derive(BufferContents, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SplatProcParams {
    /// xyz: buffer origin in box Mpc; w: lattice cell size Mpc.
    origin_cell: [f32; 4],
    /// x: sphere radius Mpc; y: sub-samples per cell `k`; z: lattice
    /// cells per axis `n`; w: unused.
    radius_k: [f32; 4],
}

/// Cosmic veil-march push constants (`cosmic-gas-veil-v2`): inverse
/// view-projection (ray reconstruction from the same matrix as the
/// draws) + eye/sphere + grid frame + window terms. Exactly 128 B —
/// the Vulkan 1.1 floor (pinned by `march_push_fits_vulkan_floor`).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct MarchPush {
    inv_vp: [[f32; 4]; 4],
    eye_radius: [f32; 4],
    center_cell: [f32; 4],
    origin_steps: [f32; 4],
    window_gain: [f32; 4],
}

/// Per-frame procedural-tracer draw resources (`cosmic-gpu-tracers`,
/// CGT-005): views + cell buffer (per seed, shared) with the per-frame
/// params + invocation count (`cells × k`). The recording functions
/// build the descriptor set against their own pipeline layout from
/// this (layouts are per-window; resources per-seed) — never shared
/// across pipelines.
#[derive(Clone)]
struct ProcFrame {
    disp_view: Arc<ImageView>,
    density_view: Arc<ImageView>,
    cells: Subbuffer<[u32]>,
    params: SplatProcParams,
    count: u32,
}

/// Precomputed per-frame cosmic draw state (update-2026-09-18-2328) —
/// see `ViewerApp::cosmic_frame`. `eye` is the camera position in the
/// surface's buffer frame. Depth-window terms (`cosmic-depth-window`):
/// `fog_l` (Mpc, ≤ 0 = off), `slab_center`/`slab_half` (Mpc view
/// depth, `slab_half` ≤ 0 = off). `march` carries the veil raymarch
/// push (`cosmic-gas-veil-v2`; stale in sprites mode — the pass is
/// skipped there).
struct CosmicFrame {
    mvp: [[f32; 4]; 4],
    px_scale: f32,
    is_demo: bool,
    eye: [f32; 3],
    glow: Subbuffer<[MapVertex]>,
    splats: Subbuffer<[SplatVertex]>,
    /// Procedural-tracer draw (`cosmic-gpu-tracers` CGT-005): `None`
    /// → the recording falls back to the legacy `splats` path (R-3
    /// fallback / empty cell list).
    proc_draw: Option<ProcFrame>,
    fog_l: f32,
    slab_center: f32,
    slab_half: f32,
    march: MarchPush,
    viewport: Viewport,
}

/// Assemble the veil-march push for a frame (CGV-005): inverse of the
/// pushed view-projection (ray reconstruction from the SAME matrix as
/// the draws), buffer-frame eye/sphere/grid, window terms, steps from
/// the veil mode.
#[allow(clippy::too_many_arguments)] // one push per frame; explicit fields are the pin
fn march_push_for(
    mvp: [[f32; 4]; 4],
    eye: [f32; 3],
    origin: glam::DVec3,
    field: &WebField,
    radius_mpc: f32,
    fog_l: f32,
    slab_center: f32,
    slab_half: f32,
    steps: u32,
) -> MarchPush {
    let inv_vp = Mat4::from_cols_array_2d(&mvp).inverse().to_cols_array_2d();
    MarchPush {
        inv_vp,
        eye_radius: [eye[0], eye[1], eye[2], radius_mpc],
        center_cell: [
            -origin.x as f32,
            -origin.y as f32,
            -origin.z as f32,
            field.cell_size_mpc as f32,
        ],
        origin_steps: [
            (field.origin_mpc[0] - origin.x) as f32,
            (field.origin_mpc[1] - origin.y) as f32,
            (field.origin_mpc[2] - origin.z) as f32,
            steps as f32,
        ],
        window_gain: [fog_l, slab_center, slab_half, VEIL_MARCH_GAIN],
    }
}

/// Additive blend for the cosmic glow paths: source added at full
/// strength (premultiplied in-shader), so overlapping strands and
/// halos accumulate light. The shared alpha-blend pipelines are
/// untouched.
fn additive_blend() -> AttachmentBlend {
    AttachmentBlend {
        color_blend_op: BlendOp::Add,
        src_color_blend_factor: BlendFactor::One,
        dst_color_blend_factor: BlendFactor::One,
        alpha_blend_op: BlendOp::Add,
        src_alpha_blend_factor: BlendFactor::One,
        dst_alpha_blend_factor: BlendFactor::One,
    }
}

/// Deep-indigo clear color for the cosmic views (near-black violet —
/// voids read as negative space against additive filaments). Deepened
/// in update-2026-09-19-1933 so faint links can sink below it.
const COSMIC_BACKDROP: [f32; 4] = [0.008, 0.005, 0.024, 1.0];

/// Cosmic grade knobs (update-2026-09-19-1933), split per surface:
/// the immersive Game Demo stacks a few sprites per pixel while the
/// zoomed-out inspector stacks dozens, so one grade cannot serve both.
/// Engine `BloomParams::spec_defaults()` (threshold 1.0, blur σ)
/// stays the shared spec; only these bin-local values tune the look.
/// Sprite alpha exposure (hub members + impostors + veil sprites).
const COSMIC_DEMO_GLOW_EXPOSURE: f32 = 1.0;
const COSMIC_MAP_GLOW_EXPOSURE: f32 = 0.3;
/// Scene exposure at the ACES resolve.
const COSMIC_DEMO_EXPOSURE: f32 = 1.15;
const COSMIC_MAP_EXPOSURE: f32 = 0.85;
/// Bloom intensities at the resolve, per surface (FR6 — back toward
/// spec now that the chain delivers the halo: the wide pyramid keeps
/// ~all of a hub core's over-threshold energy, so the 2.2/1.2
/// compensations would blow the halos out).
const COSMIC_DEMO_BLOOM_INTENSITY: f32 = 1.0;
const COSMIC_MAP_BLOOM_INTENSITY: f32 = 0.85;

/// Mip-bloom pyramid depth (`bloom-mip-chain`): 5 levels (High —
/// the windowed viewer and the capture path have no tier switch, so
/// both run the full pyramid and stay pixel-identical). Matches
/// `MipBloomParams::for_tier(High)`.
const BLOOM_LEVELS: u8 = 5;

/// Twilight demo stages (F5 cycles): sky-luminance keys at day + the
/// mid of each twilight band, so Planet-View captures step through the
/// star fade-in (exposure-tone-mapping DoD-2). Keys sit inside bands
/// (never on a threshold) so each stage shows unmistakable partial
/// visibility except the endpoints.
fn twilight_key(stage: u8) -> (f64, &'static str) {
    match stage % 4 {
        0 => (1.0, "Day"),
        1 => (10f64.powf(-4.5), "Civil"),
        2 => (10f64.powf(-6.25), "Nautical"),
        _ => (10f64.powf(-7.5), "Astronomical"),
    }
}

/// Depth format shared by the render pass and the depth image.
const DEPTH_FORMAT: Format = Format::D16_UNORM;
/// UI dynamic vertex buffer capacity (solids + text quads per frame).
const MAX_UI_VERTS: u64 = 16384;
/// UI raster size, px.
const UI_PX: f32 = 16.0;
/// Press-to-release travel (px) below which a left-button viewport gesture
/// counts as a click (chunk pin) rather than an orbit drag.
const CLICK_MAX_DRAG_PX: f32 = 4.0;
/// Windowed default subdivisions (headless stays at the engine N=6 pin).
const WINDOWED_SUBDIV: u32 = 4;
/// Windowed default radius.
const WINDOWED_RADIUS: f32 = 1.0;

// ---------------------------------------------------------------------------
// UI theme.
// ---------------------------------------------------------------------------

type Color = [f32; 4];
const C_TEXT: Color = [0.92, 0.93, 0.96, 1.0];
const C_DIM: Color = [0.60, 0.63, 0.70, 1.0];
const C_WARN: Color = [1.00, 0.70, 0.20, 1.0];
const C_ERR: Color = [1.00, 0.40, 0.35, 1.0];
const C_PANEL_BG: Color = [0.08, 0.09, 0.13, 1.0];
const C_NAV_BG: Color = [0.06, 0.07, 0.11, 1.0];
const C_TAB_ACTIVE: Color = [0.16, 0.22, 0.38, 1.0];
const C_FIELD_BG: Color = [0.04, 0.05, 0.08, 1.0];
const C_BTN: Color = [0.15, 0.25, 0.45, 1.0];
const C_BTN_OFF: Color = [0.10, 0.10, 0.12, 1.0];
const C_TRACK: Color = [0.13, 0.15, 0.20, 1.0];
const C_KNOB: Color = [0.55, 0.65, 0.90, 1.0];
const C_CHECK: Color = [0.45, 0.75, 0.45, 1.0];
/// Section header bar behind VIEW / INPUTS / SELECTION / STATS titles.
const C_SECTION_BG: Color = [0.11, 0.13, 0.19, 1.0];
/// Dimensions dropdown panel: solid black so nothing behind the menu
/// can ever show through.
const C_DROPDOWN_BG: Color = [0.00, 0.00, 0.00, 1.0];
/// Dropdown border outline.
const C_DROPDOWN_BORDER: Color = [0.35, 0.45, 0.65, 1.0];
/// Dropdown drop shadow (soft offset slab behind the panel).
const C_DROPDOWN_SHADOW: Color = [0.00, 0.00, 0.00, 0.55];
/// Hovered dropdown row lift (under the cursor, below selection).
const C_DROPDOWN_HOVER: Color = [0.22, 0.30, 0.52, 0.45];
/// Dropdown row separator hairline.
const C_DROPDOWN_SEP: Color = [0.35, 0.45, 0.65, 0.18];
/// Transition pill background (semi-transparent dark).
const C_PILL_BG: Color = [0.05, 0.06, 0.10, 0.92];
/// FPS sparkline bars: <20 ms ok, <34 ms warm, above = hitch.
const C_FPS_OK: Color = [0.30, 0.85, 0.45, 1.0];
const C_FPS_WARN: Color = [1.00, 0.75, 0.25, 1.0];
const C_FPS_HITCH: Color = [1.00, 0.35, 0.30, 1.0];
/// Screen-aware layout: the unified single-window shell reclaims
/// hidden docks (left/right toggle independently — ADR-022), so the
/// viewport grows when chrome hides. The top bar is always on.
fn app_layout(chrome: ChromeState, win_w: f32, win_h: f32) -> Layout {
    ui::layout_unified(win_w, win_h, chrome.left_dock, chrome.right_dock)
}
/// Player marker dot (planet view).
const C_PLAYER: Color = [0.30, 1.00, 0.45, 1.0];
/// Player marker size, pixels.
const PLAYER_DOT: f32 = 6.0;
/// Streaming desired-set refresh throttle while player mode is active.
const STREAM_SYNC_MS: u64 = 100;

// ---------------------------------------------------------------------------
// Args + headless.
// ---------------------------------------------------------------------------

fn usage() -> &'static str {
    "usage: game_debug [--headless] [--seed N] [--capture OUT.png --view inspector|slab|demo|vista --size WxH]"
}

/// Offscreen capture request (`--capture`, `cosmic-capture-harness`):
/// renders one frame of a cosmic surface without a window and writes
/// an 8-bit sRGB PNG. GPU-gated (never in CI).
#[derive(Debug)]
struct CaptureRequest {
    /// Output PNG path.
    path: String,
    /// Camera preset (surface + pose).
    view: game_debug::cosmic_capture::CaptureView,
    /// Output size, px.
    width: u32,
    /// Output size, px.
    height: u32,
}

/// Parsed CLI: `--headless` runs the GPU-free checks; `--seed N`
/// opens the viewer (and seeds the headless map checks) on universe N;
/// `--capture` renders one offscreen frame (GPU required).
#[derive(Debug)]
struct CliArgs {
    headless: bool,
    seed: Option<u64>,
    capture: Option<CaptureRequest>,
}

fn parse_args(argv: &[String]) -> Result<CliArgs, String> {
    use game_debug::cosmic_capture::{CAPTURE_DEFAULT_SIZE, parse_size, parse_view};
    let mut headless = false;
    let mut seed = None;
    let mut capture_path: Option<String> = None;
    let mut capture_view = None;
    let mut capture_size = CAPTURE_DEFAULT_SIZE;
    let mut rest = argv.iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--help" | "-h" => return Err(usage().to_owned()),
            "--seed" => match rest.next() {
                Some(value) => match value.parse::<u64>() {
                    Ok(n) => seed = Some(n),
                    Err(_) => {
                        return Err(format!(
                            "--seed needs a u64 seed, got {value:?}\n{usage}",
                            usage = usage()
                        ));
                    }
                },
                None => {
                    return Err(format!("--seed needs a value\n{usage}", usage = usage()));
                }
            },
            "--capture" => match rest.next() {
                Some(value) => capture_path = Some(value.clone()),
                None => {
                    return Err(format!("--capture needs a path\n{usage}", usage = usage()));
                }
            },
            "--view" => match rest.next() {
                Some(value) => match parse_view(value) {
                    Ok(view) => capture_view = Some(view),
                    Err(error) => {
                        return Err(format!("{error}\n{usage}", usage = usage()));
                    }
                },
                None => {
                    return Err(format!("--view needs a preset\n{usage}", usage = usage()));
                }
            },
            "--size" => match rest.next() {
                Some(value) => match parse_size(value) {
                    Ok(size) => capture_size = size,
                    Err(error) => {
                        return Err(format!("{error}\n{usage}", usage = usage()));
                    }
                },
                None => {
                    return Err(format!(
                        "--size needs a WxH value\n{usage}",
                        usage = usage()
                    ));
                }
            },
            other => {
                return Err(format!(
                    "unknown argument {other:?}\n{usage}",
                    usage = usage()
                ));
            }
        }
    }
    let capture = capture_path.map(|path| CaptureRequest {
        path,
        view: capture_view.unwrap_or(game_debug::cosmic_capture::CaptureView::Inspector),
        width: capture_size.0,
        height: capture_size.1,
    });
    if capture.is_some() && headless {
        return Err(format!(
            "--capture needs a GPU; --headless is GPU-free\n{usage}",
            usage = usage()
        ));
    }
    Ok(CliArgs {
        headless,
        seed,
        capture,
    })
}

/// Pose the demo camera for a `vista` capture (`cosmic-vista-intro`
/// CVI-008 test seam): external pose + 25° FOV from `vista_pose` at
/// t = 0 exactly — the same pose the live boot holds.
fn pose_demo_camera_for_vista_capture(debug: &mut DebugApp) {
    let chase = debug.cosmic.chase_pose();
    let pose = game_debug::cosmic_vista::vista_pose(
        &debug.cosmic.web,
        debug.cosmic.params.descriptor_radius_mpc,
        &chase,
    );
    debug
        .cosmic
        .camera
        .set_external_pose(Some((pose.eye, pose.target)));
    debug.cosmic.camera.set_fov_y(pose.fov_y_deg.to_radians());
}

/// GPU-free viewer check: build the default mesh through the lib, print
/// stats, run the pick self-test, exit 0. Never touches
/// `VulkanLibrary` or `EventLoop`. `seed` overrides the universe the
/// map checks run on (`--seed N`).
fn run_headless(seed: Option<u64>) -> i32 {
    let viewer = PlanetViewerState::new();
    let mut debug_app = DebugApp::new();
    // Demo boot: the Game Demo tab mounts the cosmic player scene —
    // generated web, spawned player, chase camera tracking it.
    assert_eq!(debug_app.screen, Screen::GameDemo);
    assert_eq!(debug_app.screen_content(), Some(ViewContent::CosmicWeb));
    assert!(!debug_app.cosmic.web.nodes.is_empty());
    assert!(!debug_app.cosmic.web.links.is_empty());
    assert_eq!(
        debug_app.cosmic.camera.render_origin(),
        debug_app.cosmic.player.position_mpc()
    );
    // Vista continuity pin (`cosmic-vista-intro` FR7): 10 s of ticks
    // (the shared `tick` path, no input) walk Hold → Dive → Done and
    // land exactly on the live Chase pose — the dive is one continuous
    // motion with no cut. Runs first: later self-tests tick with
    // thrust held (which skips), so they need Done behind them.
    {
        use game_debug::cosmic_vista::VistaPhase;
        assert_eq!(debug_app.cosmic.vista.phase, VistaPhase::Hold);
        for _ in 0..600 {
            debug_app.cosmic.tick(1.0 / 60.0);
        }
        assert_eq!(debug_app.cosmic.vista.phase, VistaPhase::Done);
        assert!(!debug_app.cosmic.camera.has_external_pose());
        let got = debug_app.cosmic.vista.pose();
        let want = debug_app.cosmic.chase_pose();
        assert!(
            (got.eye - want.eye).length() < 1e-6,
            "vista must end at the Chase eye"
        );
        assert!(
            (got.target - want.target).length() < 1e-6,
            "vista must end at the Chase target"
        );
        assert!((got.fov_y_deg - want.fov_y_deg).abs() < 1e-6);
        let events = std::mem::take(&mut debug_app.cosmic.vista_events);
        assert_eq!(events, vec!["hold", "dive", "done"]);
        println!("vista=done events=hold,dive,done ok");
    }
    // Field-render layout (`cosmic-gas-veil-v2` CGV-009: smoke retired
    // with the link-graph decoration; splats + hubs + veil sprites
    // derive non-empty from the boot web).
    // GPU-free — upload happens only in the windowed shell.
    let layout_seed = debug_app.cosmic.seed;
    let layout_origin = debug_app.cosmic.upload_origin;
    // Hub tiers + members (CHH-005): sprite totals + the goal-tier
    // check (FR6 — the spawn's first fly-to goal reads Tier A/B, or
    // the highlight ring carries it per the recorded UX-1 fallback).
    let (hub_impostors, hub_members, goal_tier) = {
        use game_debug::cosmic_hubs::{HubTier, hub_impostors, hub_members};
        let web = &debug_app.cosmic.web;
        let impostors = hub_impostors(web, layout_origin);
        let members = hub_members(web, layout_seed, layout_origin);
        assert!(!impostors.is_empty(), "hubs must emit impostors");
        assert!(!members.is_empty(), "hubs must emit members");
        let goal = web
            .strongest_link_from(web.home_node)
            .map(|link| {
                let far = if link.a == web.home_node {
                    link.b
                } else {
                    link.a
                };
                far as usize
            })
            .unwrap_or(web.home_node as usize);
        let tier = HubTier::of(goal, web.nodes.len());
        (impostors.len(), members.len(), tier)
    };
    println!(
        "cosmic_hubs=impostors{} members{} goal_tier{:?} ok",
        hub_impostors, hub_members, goal_tier
    );
    // Splat counts per tier + Low overdraw estimate (CTS-006/008):
    // stride subsets of the same field, so Low ⊂ Medium ⊂ High.
    // Veil mode line (CGV-006/008): sprites count or march steps.
    let (splats_low, splats_med, splats_high, overdraw_low, veil_info) = {
        use game_debug::cosmic_splat::{SplatTier, overdraw_estimate, splat_records};
        use game_debug::cosmic_veil::{VeilMode, veil_sprites};
        let web = &debug_app.cosmic.web;
        let field = &debug_app.cosmic.field;
        let low = splat_records(field, web, layout_origin, SplatTier::Low);
        let med = splat_records(field, web, layout_origin, SplatTier::Medium);
        let high = splat_records(field, web, layout_origin, SplatTier::High);
        assert!(!high.is_empty(), "splats must emit points");
        assert!(low.len() < med.len() && med.len() <= high.len());
        // Inspector framing at 1080p: px_scale / 430 Mpc depth.
        let px_per_mpc = debug_app.cosmic_inspector.camera.px_scale(1080.0) / 430.0;
        let overdraw = overdraw_estimate(&low, px_per_mpc, (1920, 1080));
        let veil = match veil_mode() {
            VeilMode::Sprites => {
                let sprites = veil_sprites(field, layout_origin);
                assert!(!sprites.is_empty(), "veil must emit sprites");
                format!("sprites{}", sprites.len())
            }
            VeilMode::March { steps } => {
                let bytes = game_debug::cosmic_veil::veil_volume_bytes(field);
                assert_eq!(bytes.len(), 128 * 128 * 128, "volume must be 128³");
                format!("march{steps}")
            }
        };
        (low.len(), med.len(), high.len(), overdraw, veil)
    };
    println!(
        "cosmic_layout=splatsL{} splatsM{} splatsH{} hubs{} members{} veil{} overdrawL{:.1} ok",
        splats_low, splats_med, splats_high, hub_impostors, hub_members, veil_info, overdraw_low
    );
    // Procedural-tracer layout (`cosmic-gpu-tracers` FR5): the cell
    // list behind the `gl_VertexIndex` draw + the tier draw counts.
    // Separate line so the CTS layout pin above stays byte-stable.
    {
        use game_debug::cosmic_splat::{SplatK, SplatTier};
        use game_engine::universe::web::cell_list;
        let cells = cell_list(&debug_app.cosmic.field);
        let (kl, km, kh) = (
            SplatK::for_tier(SplatTier::Low).0,
            SplatK::for_tier(SplatTier::Medium).0,
            SplatK::for_tier(SplatTier::High).0,
        );
        assert!(!cells.is_empty(), "cell list must emit cells");
        println!(
            "cosmic_proc=cells{} kL{} kM{} kH{} vertsH{} ok",
            cells.len(),
            kl,
            km,
            kh,
            cells.len() * usize::from(kh),
        );
    }
    // Slab relief (`cosmic-depth-window` NFR5): fraction of Low splats
    // inside the nominal slab window (30 Mpc at the home depth under
    // the 20° slab camera) — the draws that survive; the rest add ~0
    // through the window term. Separate line so the CTS layout pin
    // above stays byte-stable.
    {
        use game_debug::cosmic_splat::{SplatTier, splat_records};
        use game_debug::cosmic_window::SlabState;
        let web = &debug_app.cosmic.web;
        let field = &debug_app.cosmic.field;
        let low = splat_records(field, web, glam::DVec3::ZERO, SplatTier::Low);
        let mut cam = debug_app.cosmic_inspector.camera.clone();
        cam.set_fov_keep_framing(20.0);
        let eye = cam.eye();
        let fwd = (cam.target() - eye).normalize_or_zero();
        let home = web.home().position_mpc;
        let home_depth = ((home[0] as f32 - eye.x) * fwd.x
            + (home[1] as f32 - eye.y) * fwd.y
            + (home[2] as f32 - eye.z) * fwd.z)
            .max(0.0);
        let slab = SlabState::default_on();
        let half = slab.thickness_mpc * 0.5 + 5.0;
        let mut keep = 0usize;
        for r in &low {
            let depth = ((r.pos[0] - eye.x) * fwd.x
                + (r.pos[1] - eye.y) * fwd.y
                + (r.pos[2] - eye.z) * fwd.z)
                .max(0.0);
            if (depth - home_depth).abs() <= half {
                keep += 1;
            }
        }
        println!(
            "slab_relief=keep{} total{} frac{:.2} ok",
            keep,
            low.len(),
            keep as f64 / low.len().max(1) as f64
        );
    }
    // Field sidecar self-check (`web-field-export`, ADR-025): the export
    // entry point returns the identical descriptor, the tracer band
    // holds, and the sidecar fits its memory budget. Timings print for
    // the plan.md record (the +120 ms NFR3 budget is a release-desktop
    // number; dev-profile absolutes are recorded, not gated).
    {
        use game_engine::universe::{
            CosmicWebParams, WebFieldBudget, generate_cosmic_web, generate_cosmic_web_with_field,
        };
        let params = CosmicWebParams::nominal();
        let t0 = Instant::now();
        let plain = generate_cosmic_web(1337, &params);
        let plain_ms = t0.elapsed();
        let t1 = Instant::now();
        let (via_field, field) =
            generate_cosmic_web_with_field(1337, &params, WebFieldBudget::Full);
        let field_ms = t1.elapsed();
        assert_eq!(plain, via_field, "export must not move the descriptor");
        assert!(
            (900_000..=1_200_000).contains(&field.tracers.len()),
            "nominal tracer band broken: {}",
            field.tracers.len()
        );
        let bytes =
            field.tracers.len() * size_of::<game_engine::universe::WebTracer>() + field.grid.len();
        assert!(bytes <= 20 * 1024 * 1024, "sidecar over budget: {bytes} B");
        println!(
            "web_field=tracers{} plain_ms{} field_ms{} delta_ms{} mb{} ok",
            field.tracers.len(),
            plain_ms.as_millis(),
            field_ms.as_millis(),
            field_ms.as_millis().saturating_sub(plain_ms.as_millis()),
            bytes / (1024 * 1024)
        );
    }
    // Cruise smoke (update 2026-09-18-2027): five seconds of W must move
    // the ship Mpc-scale — the old thrust law could not move it at all.
    let cruise_p0 = debug_app.cosmic.player.position_mpc();
    debug_app.cosmic.held.fwd = true;
    for _ in 0..300 {
        debug_app.cosmic.tick(1.0 / 60.0);
    }
    debug_app.cosmic.held.clear();
    let cruise_moved = (debug_app.cosmic.player.position_mpc() - cruise_p0).length();
    assert!(
        cruise_moved > 1.0,
        "headless cruise must move Mpc-scale, moved {cruise_moved}"
    );
    println!("cruise_selftest=moved{cruise_moved:.2}Mpc ok");
    // Rebase traverse (`cosmic-rebase-async` CRA-007/FR6): scripted
    // max-pace thrust across ≥ 500 Mpc with ≥ 10 rebases (50 Mpc
    // rebase distance). Two clocks: per-tick time is the FRAME-cost
    // analog (tick + poll-equivalent bookkeeping — the only work a
    // windowed frame ever does for a rebase), while the build time is
    // the WORKER cost (off-frame by design, informational here). The
    // windowed main thread additionally uploads + swaps the two
    // buffers on the swap frame only (CRA-008, reference hardware).
    {
        use game_debug::cosmic_player::CRUISE_T_CROSS_MIN_S;
        use game_debug::cosmic_rebase::{build_demo_glow, build_demo_splats};
        debug_app.cosmic.player.cruise.t_cross_s = CRUISE_T_CROSS_MIN_S;
        debug_app.cosmic.held.fwd = true;
        let start = debug_app.cosmic.player.position_mpc();
        let mut rebases = 0u32;
        let mut max_tick_ms = 0.0f64;
        let mut max_build_ms = 0.0f64;
        let mut ticks = 0u32;
        let mut travelled = 0.0;
        while (travelled < 500.0 || rebases < 10) && ticks < 120_000 {
            ticks += 1;
            let frame = Instant::now();
            let crossed = debug_app.cosmic.tick(1.0 / 60.0);
            max_tick_ms = max_tick_ms.max(frame.elapsed().as_secs_f64() * 1000.0);
            if crossed {
                let origin = debug_app.cosmic.player.position_mpc();
                let built = Instant::now();
                let glow = build_demo_glow(
                    &debug_app.cosmic.web,
                    &debug_app.cosmic.field,
                    debug_app.cosmic.seed,
                    origin,
                    veil_mode(),
                );
                let splats =
                    build_demo_splats(&debug_app.cosmic.field, &debug_app.cosmic.web, origin);
                max_build_ms = max_build_ms.max(built.elapsed().as_secs_f64() * 1000.0);
                rebases += 1;
                assert!(
                    !glow.is_empty() && !splats.is_empty(),
                    "rebase build must emit points"
                );
                debug_app.cosmic.rebased();
            }
            travelled = (debug_app.cosmic.player.position_mpc() - start).length();
        }
        debug_app.cosmic.held.clear();
        println!(
            "rebase_traverse=travelled{travelled:.1}Mpc ticks{ticks} rebases{rebases} max_tick_ms{max_tick_ms:.2} max_build_ms{max_build_ms:.1} ok"
        );
        assert!(
            travelled >= 500.0,
            "traverse must cover 500 Mpc, covered {travelled:.1}"
        );
        assert!(
            rebases >= 10,
            "traverse must rebase ≥ 10×, rebased {rebases}×"
        );
        assert!(
            max_tick_ms <= 33.0,
            "frame work must stay in budget, max tick {max_tick_ms:.2} ms"
        );
    }
    // Unified shell smoke: F2 opens Dimensions on the active
    // waypoint, digits pick dropdown entries, F-keys jump the
    // widget to a sub-tab.
    assert!(debug_app.select_top_by_fkey(2));
    assert_eq!(debug_app.screen, Screen::Dimensions(WaypointId::MilkyWay));
    assert!(debug_app.dropdown_open);
    assert!(debug_app.select_dimension_by_digit(1));
    assert_eq!(debug_app.screen, Screen::Dimensions(WaypointId::CosmicWeb));
    // Cosmic Web tab mounts the shared web (fourth absorbed view).
    assert_eq!(debug_app.screen_content(), Some(ViewContent::CosmicWeb));
    // Placeholder dimension: flat UI, no 3D content.
    assert!(debug_app.select_dimension_by_digit(8));
    assert_eq!(debug_app.screen, Screen::Dimensions(WaypointId::Aerial));
    assert_eq!(debug_app.screen_content(), None);
    assert!(debug_app.select_widget_tab_by_fkey(7));
    assert_eq!(debug_app.widget_tab, WidgetTab::Console);
    // Pick self-test (cell-chunks): aiming at cell 0's center from 5
    // radii out must resolve chunk 0 — fails loudly on picking or
    // mesh-ordering regressions.
    let target =
        glam::Vec3::from_array(viewer.mesh.cell_center(0)).normalize() * viewer.radius * 5.0;
    let ray = Ray {
        origin: target,
        dir: -target.normalize(),
    };
    let hit = intersect_sphere(ray, viewer.radius).expect("pick self-test ray must hit");
    let picked = pick_cell(&viewer.mesh, hit, None);
    assert_eq!(
        picked.index(),
        0,
        "pick self-test resolved chunk {}",
        picked.index()
    );
    let stats = &viewer.stats;
    let seams = viewer.fill_debug.seam.iter().filter(|&&s| s == 1.0).count();
    let mut islands = viewer.fill_debug.island.clone();
    islands.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    islands.dedup();
    println!(
        "subdiv={} radius={} cells={} corners={} pentagons={} tris={} hash8={} gen_ms={:.1}",
        viewer.subdiv,
        viewer.radius,
        stats.cells,
        stats.corners,
        stats.pentagons,
        6 * (stats.cells - stats.pentagons) + 5 * stats.pentagons,
        stats.hash8,
        stats.gen_ms,
    );
    println!(
        "uv_islands={} uv_seam_verts={} mode={:?}",
        islands.len(),
        seams,
        viewer.debug_mode,
    );
    println!("pick_selftest=chunk{} ok", picked.index());
    // Player self-test (debug-player-view): face east, then thrust
    // along the heading on the default mesh; longitude must rise,
    // latitude hold, streaming settle on the walker's hemisphere.
    let mut walk = PlayerViewState::new(viewer.radius);
    walk.toggle();
    walk.set_keys(MoveKeys {
        east: true,
        ..MoveKeys::default()
    });
    for _ in 0..10 {
        let desired = visible_hemisphere(&viewer.mesh, walk.position().to_array());
        walk.update(0.05, &desired);
    }
    walk.set_keys(MoveKeys {
        north: true,
        ..MoveKeys::default()
    });
    for _ in 0..20 {
        let desired = visible_hemisphere(&viewer.mesh, walk.position().to_array());
        walk.update(0.05, &desired);
    }
    // One zero-dt sync: the last move can rotate fresh rim cells into
    // the hemisphere that no tick has streamed yet (the viewer closes
    // the same one-frame lag on the next frame).
    let desired = visible_hemisphere(&viewer.mesh, walk.position().to_array());
    walk.update(0.0, &desired);
    let (lon, lat) = walk.lon_lat_deg();
    assert!(lon > 0.0, "east walk must raise longitude, got {lon}");
    assert!(lat.abs() < 1.0, "east walk must hold latitude, got {lat}");
    // The current hemisphere is fully loaded (the grace cache may hold
    // the trail on top, so this is a subset check, not equality).
    let desired = visible_hemisphere(&viewer.mesh, walk.position().to_array());
    let loaded = walk.loaded();
    assert!(
        desired.iter().all(|cell| loaded.contains(cell)),
        "hemisphere must be loaded: {} desired, {} have",
        desired.len(),
        loaded.len(),
    );
    println!(
        "player_selftest=lon{lon:.2} lat{lat:.2} loaded{} ok",
        walk.loaded_count(),
    );
    // Galaxy-map self-test (universe-maps-3d): default seed builds the
    // v1 star count; projecting star 0 and picking it resolves star 0;
    // zoom clamps hold. GPU-free like the rest of this path.
    let mut map = GalaxyMapView::new(seed.unwrap_or(DEFAULT_GALAXY_SEED));
    assert_eq!(
        map.galaxy.stars.len(),
        game_engine::universe::DEFAULT_STAR_COUNT as usize,
        "galaxy must build the v1 default star count"
    );
    let vp = Rect {
        x: 0.0,
        y: 0.0,
        w: 800.0,
        h: 600.0,
    };
    let star = &map.galaxy.stars[0];
    let (sx, sy) = project_to_screen(star_world(star), map.camera.view_proj(vp.w / vp.h), vp)
        .expect("star 0 must project in the default framing");
    assert_eq!(
        map.select_at((sx, sy), vp),
        Some(0),
        "project-then-pick must resolve star 0"
    );
    map.camera.zoom_by(1e-9);
    assert_eq!(map.camera.distance(), map.camera.min_distance());
    map.camera.zoom_by(1e9);
    assert_eq!(map.camera.distance(), map.camera.max_distance());
    println!(
        "galaxy_seed={} galaxy_stars={} galaxy_hash={:016x} pick_selftest=star0 ok",
        map.seed,
        map.galaxy.stars.len(),
        game_engine::universe::galaxy_hash(&map.galaxy),
    );
    // System-map self-test (universe-maps-3d): load star 0's system,
    // project planet 0 and pick it, toggle the L4 focus, drive the
    // journey drill-down GalaxyMap → SystemMap. GPU-free.
    let star0 = map.galaxy.stars[0].clone();
    let mut system = SystemMapView::new(map.seed, &star0);
    let (px, py, pz) =
        game_debug::system_map::planet_slot(system.system.planets[0].orbit_radius_au, 0);
    let (sx, sy) = project_to_screen(
        Vec3::new(px as f32, py as f32, pz as f32),
        system.camera.view_proj(vp.w / vp.h),
        vp,
    )
    .expect("planet 0 must project in the default framing");
    assert_eq!(system.select_at((sx, sy), vp), Some(0));
    system.toggle_focus();
    assert_eq!(system.focus, Some(0));
    system.toggle_focus();
    assert_eq!(system.focus, None);
    let mut journey = Journey::new(map.seed);
    journey.update(JourneyEvent::SelectStar(0));
    let fx = journey.update(JourneyEvent::EnterSystem);
    assert_eq!(journey.active_layer(), Layer::System);
    assert_eq!(fx.len(), 3);
    println!(
        "system_star=0 planets={} system_hash={:016x} focus_toggle=ok journey=System ok",
        system.system.planets.len(),
        game_engine::universe::system_hash(&system.system),
    );
    // Transit self-test (universe-maps): fixed-step countdown reaches
    // ready exactly at duration; cancel-before-commit drops it.
    let mut transit = game::transit::Transit::begin(0, 0);
    for _ in 0..game::transit::TRANSIT_TICKS {
        assert!(!transit.ready());
        transit.tick();
    }
    assert!(transit.ready());
    transit.cancel();
    let cost = game::transit::plan_cost(system.system.planets[0].orbit_radius_au);
    println!(
        "transit_ticks={} fuel={:.1} energy={:.1} ok",
        game::transit::TRANSIT_TICKS,
        cost.fuel,
        cost.energy,
    );
    // Catalog-sky self-test (star-catalog-streaming): model-only sky,
    // synthetic orbiting views; asserts the plan → fallback → expand
    // loop never blanks. GPU-free like the rest of this path.
    println!("{} ok", CatalogSky::headless_check());
    0
}

/// Offscreen capture format (R-1): no surface exists, so the windowed
/// swapchain format is unavailable — `B8G8R8A8_SRGB` is the near-
/// universal swapchain pick, and the capture builds its own render
/// passes + pipelines against it (never reusing windowed pipelines
/// against a foreign format).
const CAPTURE_FORMAT: Format = Format::B8G8R8A8_SRGB;

/// Default capture seed (the DoD gate examples use `--seed 1337`).
const CAPTURE_DEFAULT_SEED: u64 = 1337;

/// Offscreen cosmic capture (`cosmic-capture-harness` FR1): headless
/// Vulkan boot (no surface, no swapchain, no window), same HDR chain /
/// pipelines / buffers as the windowed viewer, one frame recorded
/// through the shared CAP-001 seam (`record_cosmic_hdr_prepass` +
/// `record_cosmic_view_arm` — the only recording path), read back to
/// host, BGRA→RGBA swizzled, PNG-encoded top-row-first, written to
/// `request.path`. Deterministic per (build, seed, preset, size) on a
/// given GPU: fixed sim time `t = 0` (no ticks), no animation, all
/// randomness from the seed. Exit codes (FR5): 2 = no Vulkan device,
/// 3 = output unwritable.
///
/// This function intentionally duplicates the small CPU-side frame
/// assembly (`cosmic_frame` stays windowed-only): the GPU command
/// recording is shared, the CPU pose math is not.
fn run_capture(request: CaptureRequest, seed: Option<u64>) -> i32 {
    use game_debug::cosmic_capture::{CaptureView, encode_png_rgba8, preset_for};
    let seed = seed.unwrap_or(CAPTURE_DEFAULT_SEED);
    let preset = preset_for(request.view);
    let (w, h) = (request.width, request.height);
    // FR5: unwritable path → exit 3 before touching Vulkan.
    if let Some(parent) = std::path::Path::new(&request.path).parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "capture: cannot create output dir {}: {error}",
            parent.display()
        );
        return 3;
    }
    // Headless Vulkan boot: instance without surface extensions, a
    // graphics queue without presentation support, no swapchain
    // extension on the device.
    let library = match VulkanLibrary::new() {
        Ok(library) => library,
        Err(error) => {
            eprintln!("capture: no Vulkan loader: {error}");
            return 2;
        }
    };
    let instance = match create_instance(library, InstanceExtensions::empty(), false) {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("capture: instance failed: {error:?}");
            return 2;
        }
    };
    let (physical_device, queue_family_index) = match instance
        .enumerate_physical_devices()
        .map_err(|error| format!("{error:?}"))
        .and_then(|devices| {
            devices
                .filter_map(|p| {
                    p.queue_family_properties()
                        .iter()
                        .enumerate()
                        .position(|(_, q)| q.queue_flags.intersects(QueueFlags::GRAPHICS))
                        .map(|i| (p, i as u32))
                })
                .min_by_key(|(p, _)| device_score(p.properties().device_type))
                .ok_or_else(|| "no graphics queue found".to_owned())
        }) {
        Ok(found) => found,
        Err(error) => {
            eprintln!("capture: no suitable Vulkan device: {error}");
            return 2;
        }
    };
    log_physical_device(&physical_device);
    let (device, mut queues) = match Device::new(
        physical_device.clone(),
        DeviceCreateInfo {
            enabled_extensions: DeviceExtensions::empty(),
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            ..Default::default()
        },
    ) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("capture: logical device failed: {error:?}");
            return 2;
        }
    };
    let queue = queues.next().expect("one queue requested");
    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
    let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
        device.clone(),
        Default::default(),
    ));
    let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
        device.clone(),
        Default::default(),
    ));
    let post_sampler = Sampler::new(
        device.clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            address_mode: [SamplerAddressMode::ClampToEdge; 3],
            ..Default::default()
        },
    )
    .expect("capture post sampler must create");
    let shaders = ShaderSet::compile(&device);
    // Offscreen render passes: main (capture format + depth, the
    // windowed main-pass shape) plus the shared scene/post builders.
    let render_pass = vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                format: CAPTURE_FORMAT,
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
            depth: {
                format: DEPTH_FORMAT,
                samples: 1,
                load_op: Clear,
                store_op: DontCare,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {depth},
        },
    )
    .expect("capture render pass must create");
    // HDR select over the same support predicate as windowed (FR5:
    // missing HDR → LDR bypass + log, still exit 0).
    let hdr_format = match select_hdr_format(|format| hdr_support(&physical_device, format)) {
        HdrSelection::Hdr(format) => Some(format),
        HdrSelection::LdrBypass => None,
    };
    if hdr_format.is_none() {
        eprintln!("capture: no HDR format — LDR bypass");
    }
    let scene_format = hdr_format.unwrap_or(CAPTURE_FORMAT);
    let scene_pass = build_scene_pass(&device, scene_format);
    let post_pass = build_post_pass(&device, scene_format);
    // Cosmic-only pipeline subset (the capture records no other view).
    let pipes = Pipelines {
        fill: build_fill_pipeline(&device, &shaders, &render_pass),
        line: build_line_pipeline(&device, &shaders, &render_pass),
        ui: build_ui_pipeline(&device, &shaders, &render_pass),
        map: build_map_pipeline(&device, &shaders, &render_pass),
        map_glow: build_glow_pipeline(&device, &shaders, &render_pass),
        splat: build_splat_pipeline(&device, &shaders, &render_pass),
        splat_proc: build_splat_proc_pipeline(&device, &shaders, &render_pass),
        glow_scene: build_glow_pipeline(&device, &shaders, &scene_pass),
        splat_scene: build_splat_pipeline(&device, &shaders, &scene_pass),
        splat_proc_scene: build_splat_proc_pipeline(&device, &shaders, &scene_pass),
        prefilter: build_post_pipeline(
            &device,
            &shaders.prefilter_frag,
            &shaders.post_vert,
            &post_pass,
            None,
            "bloom prefilter",
        ),
        down: build_post_pipeline(
            &device,
            &shaders.down_frag,
            &shaders.post_vert,
            &post_pass,
            None,
            "bloom down",
        ),
        up: build_post_pipeline(
            &device,
            &shaders.up_frag,
            &shaders.post_vert,
            &post_pass,
            None,
            "bloom up",
        ),
        march: build_post_pipeline(
            &device,
            &shaders.march_frag,
            &shaders.post_vert,
            &post_pass,
            None,
            "veil march",
        ),
        resolve: build_post_pipeline(
            &device,
            &shaders.resolve_frag,
            &shaders.post_vert,
            &render_pass,
            Some(DepthStencilState::default()),
            "bloom resolve",
        ),
    };
    // Deterministic app state at t = 0 (no ticks, no animation).
    let mut debug = DebugApp::new();
    if seed != DEFAULT_GALAXY_SEED {
        debug.cosmic.reseed(seed);
    }
    // Gas-veil density volume (per seed; the march samples it).
    let veil_volume = upload_veil_volume(
        &memory_allocator,
        &command_buffer_allocator,
        &queue,
        &device,
        &debug.cosmic.field,
    );
    let is_demo = preset.surface_is_demo;
    let aspect = w as f32 / h as f32;
    // Depth window (`cosmic-depth-window` CDW-007): the `slab` preset
    // narrows to 20° (framing kept by the distance rescale) and windows
    // a 30 Mpc slice at the home node's view depth — the target's
    // composition. `inspector` stays full-depth 60°.
    if request.view == CaptureView::Slab {
        debug.cosmic_inspector.camera.set_fov_keep_framing(20.0);
        debug.cosmic_inspector.slab = game_debug::cosmic_window::SlabState::default_on();
    }
    // Vista intro (`cosmic-vista-intro` CVI-008): the `vista` preset IS
    // the t = 0 vista pose — external pose + 25° FOV on the demo
    // camera, driven per seed by `vista_pose` (the same pose the live
    // boot holds). Optional `GAME_DEBUG_VISTA_T` pre-roll (seconds of
    // 60 Hz ticks, deterministic per build+seed) serves the timed
    // marker/hint DoD shots.
    let vista_pose = if request.view == CaptureView::Vista {
        pose_demo_camera_for_vista_capture(&mut debug);
        if let Ok(t) = std::env::var("GAME_DEBUG_VISTA_T")
            && let Ok(secs) = t.parse::<f64>()
            && secs > 0.0
        {
            for _ in 0..(secs * 60.0) as usize {
                debug.cosmic.tick(1.0 / 60.0);
            }
        }
        Some({
            let chase = debug.cosmic.chase_pose();
            game_debug::cosmic_vista::vista_pose(
                &debug.cosmic.web,
                debug.cosmic.params.descriptor_radius_mpc,
                &chase,
            )
        })
    } else {
        None
    };
    let (mvp, px_scale, eye, origin, fog_l, slab_center, slab_half) = if is_demo {
        let camera = &debug.cosmic.camera;
        let eye_w = camera.eye_world();
        let origin = debug.cosmic.upload_origin;
        // Vista preset (CVI-008): fog/slab ride the t = 0 pose (fog
        // off, 40 Mpc slab at the hub depth); the plain demo keeps the
        // slider fog with the slab off.
        let (fog_l, slab_center, slab_half) = match vista_pose {
            Some(pose) => (pose.fog_l_mpc(), pose.slab_center_mpc, pose.slab_half_mpc),
            None => (debug.cosmic.fog_l_mpc, 0.0, 0.0),
        };
        (
            camera.view_proj(aspect).to_cols_array_2d(),
            camera.px_scale(h as f32),
            [
                (eye_w.x - origin.x) as f32,
                (eye_w.y - origin.y) as f32,
                (eye_w.z - origin.z) as f32,
            ],
            origin,
            fog_l,
            slab_center,
            slab_half,
        )
    } else {
        let inspector = &debug.cosmic_inspector;
        let e = inspector.camera.eye();
        // Slab center: home-node view depth under the preset camera.
        let (slab_center, slab_half) = if inspector.slab.on {
            let home = debug.cosmic.web.home().position_mpc;
            let fwd = (inspector.camera.target() - e).normalize_or_zero();
            let depth = ((home[0] as f32 - e.x) * fwd.x
                + (home[1] as f32 - e.y) * fwd.y
                + (home[2] as f32 - e.z) * fwd.z)
                .max(0.0);
            (depth, inspector.slab.thickness_mpc * 0.5)
        } else {
            (0.0, 0.0)
        };
        (
            inspector.view_proj(aspect).to_cols_array_2d(),
            inspector.camera.px_scale(h as f32),
            [e.x, e.y, e.z],
            DVec3::ZERO,
            0.0,
            slab_center,
            slab_half,
        )
    };
    let extent = [w, h];
    // Veil mode first (the glow upload branches on it).
    let veil_mode = veil_mode();
    let glow = upload_cosmic_glow(
        &memory_allocator,
        &debug.cosmic.web,
        &debug.cosmic.field,
        seed,
        origin,
        veil_mode,
    );
    let splats = upload_cosmic_splats(
        &memory_allocator,
        &debug.cosmic.field,
        &debug.cosmic.web,
        origin,
    );
    // Procedural-tracer resources (CGT-004): displacement volume +
    // cell list on the capture device (same seed path as windowed).
    let disp_volume = upload_displacement_volume(
        &memory_allocator,
        &command_buffer_allocator,
        &queue,
        &device,
        &debug.cosmic.field,
    );
    let capture_cells = game_engine::universe::web::cell_list(&debug.cosmic.field);
    let capture_cell_list = upload_cell_list(&memory_allocator, &capture_cells);
    let capture_k = splat_k_default();
    let proc_draw = match (
        disp_volume.as_ref().map(|(_, view)| view.clone()),
        veil_volume.as_ref().map(|(_, view)| view.clone()),
        capture_cell_list.clone(),
    ) {
        (Some(disp_view), Some(density_view), Some(cells)) => Some(ProcFrame {
            disp_view,
            density_view,
            cells,
            params: SplatProcParams {
                origin_cell: [
                    origin.x as f32,
                    origin.y as f32,
                    origin.z as f32,
                    debug.cosmic.field.cell_size_mpc as f32,
                ],
                radius_k: [
                    debug.cosmic.field.sphere_radius_mpc as f32,
                    capture_k as f32,
                    debug.cosmic.field.grid_cells as f32,
                    0.0,
                ],
            },
            count: (capture_cells.len() as u32).saturating_mul(capture_k as u32),
        }),
        _ => None,
    }
    .filter(|proc| proc.count > 0);
    let ship = debug.cosmic.player.position_mpc();
    let player_point = upload_cosmic_player_point(
        &memory_allocator,
        [ship.x as f32, ship.y as f32, ship.z as f32],
    );
    // Veil march push (CGV-005): steps from the mode above; skipped
    // at record time in sprites mode.
    let march_steps = match veil_mode {
        game_debug::cosmic_veil::VeilMode::Sprites => 0,
        game_debug::cosmic_veil::VeilMode::March { steps } => steps,
    };
    let march = march_push_for(
        mvp,
        eye,
        origin,
        &debug.cosmic.field,
        debug.cosmic.params.descriptor_radius_mpc as f32,
        fog_l,
        slab_center,
        slab_half,
        march_steps,
    );
    let frame = CosmicFrame {
        mvp,
        px_scale,
        is_demo,
        eye,
        glow,
        splats,
        proc_draw,
        fog_l,
        slab_center,
        slab_half,
        march,
        viewport: Viewport {
            offset: [0.0, 0.0],
            extent: [w as f32, h as f32],
            depth_range: 0.0..=1.0,
        },
    };
    let extent_arr = [w, h];
    let veil_view = veil_volume.as_ref().map(|(_, view)| view.clone());
    let hdr = hdr_format.map(|format| {
        ViewerApp::build_hdr_chain(
            &memory_allocator,
            &descriptor_set_allocator,
            &post_sampler,
            format,
            extent_arr,
            &scene_pass,
            &post_pass,
            &pipes,
            veil_view.as_ref().expect("veil volume must upload"),
        )
    });
    // Offscreen target: capture-format color (+TRANSFER_SRC for the
    // readback) with depth, under the offscreen main pass.
    let target_image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: CAPTURE_FORMAT,
            extent: [w, h, 1],
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| {
        eprintln!("capture: target image failed: {error}");
        std::process::exit(2);
    });
    let target_view = ImageView::new_default(target_image.clone()).unwrap_or_else(|error| {
        eprintln!("capture: target view failed: {error}");
        std::process::exit(2);
    });
    let target_depth = create_depth_view(&memory_allocator, extent);
    let target_fb = Framebuffer::new(
        render_pass.clone(),
        FramebufferCreateInfo {
            attachments: vec![target_view, target_depth],
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| {
        eprintln!("capture: target framebuffer failed: {error:?}");
        std::process::exit(2);
    });
    let redshift = game_debug::cosmic_web::COSMIC_REDSHIFT_PER_MPC;
    let (glow_exposure, resolve_exposure, bloom_intensity) = if is_demo {
        (
            COSMIC_DEMO_GLOW_EXPOSURE,
            COSMIC_DEMO_EXPOSURE,
            COSMIC_DEMO_BLOOM_INTENSITY,
        )
    } else {
        (
            COSMIC_MAP_GLOW_EXPOSURE,
            COSMIC_MAP_EXPOSURE,
            COSMIC_MAP_BLOOM_INTENSITY,
        )
    };
    let splat_alpha_k = if is_demo {
        SPLAT_ALPHA_K_DEMO
    } else {
        SPLAT_ALPHA_K_MAP
    };
    let bloom_enabled = !std::env::var("GAME_DEBUG_COSMIC_BLOOM").is_ok_and(|value| value == "0");
    let readback = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE
                | MemoryTypeFilter::PREFER_HOST,
            ..Default::default()
        },
        (0..w as usize * h as usize * 4).map(|_| 0u8),
    )
    .expect("capture readback buffer must create");
    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("capture command buffer builder must create");
    if let Some(chain) = hdr.as_ref() {
        // High-tier pyramid on both paths (windowed + capture stay
        // pixel-identical — the debug binary has no tier switch).
        let bloom_params = MipBloomParams::for_tier(QualityTier::High);
        record_cosmic_hdr_prepass(
            &mut builder,
            &pipes,
            chain,
            &frame,
            glow_exposure,
            redshift,
            &bloom_params,
            bloom_enabled,
            splat_alpha_k,
            &descriptor_set_allocator,
            &memory_allocator,
            &post_sampler,
        );
        // Gas-veil march (CGV-005/006): pyramid → march → resolve.
        // Skipped in sprites mode (cleared target adds ~0).
        if matches!(veil_mode, game_debug::cosmic_veil::VeilMode::March { .. }) {
            record_veil_march(&mut builder, &pipes, chain, &frame.march);
        }
    }
    builder
        .begin_render_pass(
            RenderPassBeginInfo {
                clear_values: vec![Some(COSMIC_BACKDROP.into()), Some(ClearValue::Depth(1.0))],
                ..RenderPassBeginInfo::framebuffer(target_fb)
            },
            SubpassBeginInfo {
                contents: SubpassContents::Inline,
                ..Default::default()
            },
        )
        .expect("capture pass must begin");
    record_cosmic_view_arm(
        &mut builder,
        &pipes,
        hdr.as_ref(),
        bloom_enabled,
        &frame,
        frame.viewport.clone(),
        [w as f32, h as f32],
        glow_exposure,
        resolve_exposure,
        bloom_intensity,
        redshift,
        splat_alpha_k,
        VEIL_MARCH_RESOLVE_GAIN,
        player_point,
        &descriptor_set_allocator,
        &memory_allocator,
        &post_sampler,
    );
    builder
        .end_render_pass(Default::default())
        .expect("capture pass must end");
    builder
        .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(
            target_image,
            readback.clone(),
        ))
        .expect("capture readback copy must record");
    let command_buffer = builder.build().expect("capture command buffer must build");
    if let Err(error) = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("capture submit must succeed")
        .then_signal_fence_and_flush()
        .expect("capture fence must flush")
        .wait(None)
    {
        eprintln!("capture: GPU execution failed: {error}");
        return 2;
    }
    // BGRA (swapchain order) → RGBA swizzle, row 0 = top (NDC +1 =
    // top invariant, NFR5) straight into the PNG encoder.
    let pixels = {
        let guard = readback.read().expect("capture readback must map");
        let mut rgba = guard.to_vec();
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        rgba
    };
    debug_assert_eq!(pixels.len(), w as usize * h as usize * 4);
    let png = match encode_png_rgba8(w, h, &pixels) {
        Ok(png) => png,
        Err(error) => {
            eprintln!("capture: PNG encode failed: {error}");
            return 3;
        }
    };
    if let Err(error) = std::fs::write(&request.path, &png) {
        eprintln!("capture: cannot write {}: {error}", request.path);
        return 3;
    }
    let view_name = match request.view {
        CaptureView::Inspector => "inspector",
        CaptureView::Slab => "slab",
        CaptureView::Demo => "demo",
        CaptureView::Vista => "vista",
    };
    println!(
        "capture saved: {} (view {view_name}, seed {seed}, {w}x{h})",
        request.path
    );
    0
}

// ---------------------------------------------------------------------------
// Pure UI builders (unit-tested below; the frame loop only converts).
// ---------------------------------------------------------------------------

/// Windowed-capture timestamp (`captures/<surface>-<seed>-<ts>.png`):
/// UTC `yyyymmdd-hhmmss` from UNIX seconds, dependency-free (Hinnant's
/// days-from-civil algorithm over u64). Exploration filenames only —
/// DoD evidence always comes from `--capture` presets.
fn capture_timestamp(secs: u64) -> String {
    let days = secs / 86_400;
    let tod = secs % 86_400;
    // Days → civil date (Hinnant): all exact u64 math, valid for any
    // non-negative day count.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += u64::from(m <= 2);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        y,
        m,
        d,
        tod / 3_600,
        (tod % 3_600) / 60,
        tod % 60
    )
}

/// Current UTC timestamp for windowed captures; `unknown-time` when
/// the clock is unavailable (never a panic on a dev tool path).
fn capture_now_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| capture_timestamp(d.as_secs()))
        .unwrap_or_else(|_| "unknown-time".to_owned())
}

/// Thousands separator for stats (`40962` → `"40,962"`).
fn fmt_int(mut n: usize) -> String {
    if n == 0 {
        return "0".to_owned();
    }
    let mut groups = Vec::new();
    while n > 0 {
        groups.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    let mut out = groups
        .pop()
        .expect("n > 0")
        .trim_start_matches('0')
        .to_owned();
    if out.is_empty() {
        out.push('0');
    }
    for group in groups.iter().rev() {
        out.push(',');
        out.push_str(group);
    }
    out
}

/// Pixel→NDC ortho (y-down pixels): `(0,0)` → `(-1,1)`, `(w,h)` → `(1,-1)`.
fn ortho_matrix(w: f32, h: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, -2.0 / h, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}

/// Global orbit-camera presets behind the panel VIEW buttons
/// (and the `G`/`T`/`B`/`R` keys). They retarget the free orbit camera;
/// the player cameras are untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalPreset {
    Top,
    Bottom,
    Right,
    Perspective,
}

impl GlobalPreset {
    /// Panel buttons in draw/hit-test order.
    const ALL: [(GlobalPreset, &'static str); 4] = [
        (GlobalPreset::Top, "Top"),
        (GlobalPreset::Bottom, "Bot"),
        (GlobalPreset::Right, "Right"),
        (GlobalPreset::Perspective, "Persp"),
    ];

    fn from_index(index: usize) -> Option<GlobalPreset> {
        Self::ALL.get(index).map(|(preset, _)| *preset)
    }
}

/// Snap the global orbit camera to `preset` at framing distance
/// (3.2 R inside the `[1.6 R, 8 R]` smoke range).
fn snap_global_camera(camera: &mut OrbitCamera, preset: GlobalPreset, radius: f32) {
    let (yaw, pitch) = match preset {
        GlobalPreset::Perspective => (0.0, 0.35),
        GlobalPreset::Top => (0.0, MAX_PITCH),
        GlobalPreset::Bottom => (0.0, -MAX_PITCH),
        GlobalPreset::Right => (0.0, 0.0),
    };
    *camera = OrbitCamera::new(
        Vec3::ZERO,
        3.2 * radius,
        yaw,
        pitch,
        1.6 * radius,
        8.0 * radius,
    );
}

/// Short panel label for a player camera mode.
fn short_mode(mode: CameraMode) -> &'static str {
    match mode {
        CameraMode::Follow => "follow",
        CameraMode::FirstPerson => "first",
        CameraMode::ThirdPerson => "third",
        CameraMode::Global => "global",
    }
}

/// Project a world point through `view_proj` to y-down viewport pixels.
/// `None` when behind the camera (`w <= 0`) or outside the rect — the
/// player marker then hides instead of smearing across the screen.
fn world_to_pixels(view_proj: Mat4, world: Vec3, rect: Rect) -> Option<(f32, f32)> {
    if rect.w < 1.0 || rect.h < 1.0 {
        return None;
    }
    let clip = view_proj * world.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
        return None;
    }
    Some((
        rect.x + (ndc.x + 1.0) / 2.0 * rect.w,
        rect.y + (1.0 - ndc.y) / 2.0 * rect.h,
    ))
}

/// One screen-space triangle (y-down pixels): the `UiItems::tri`
/// payload and the player-arrow geometry unit.
type UiTri = ((f32, f32), (f32, f32), (f32, f32));

/// Screen-space arrow from `origin` along unit `dir` (y-down pixels):
/// shaft quad + triangular head. Returns 3 tris (2 shaft, 1 head).
/// Pure geometry — unit-tested below.
fn arrow_tris(origin: (f32, f32), dir: (f32, f32), len: f32, width: f32, head: f32) -> [UiTri; 3] {
    let (ox, oy) = origin;
    let (dx, dy) = dir;
    let (nx, ny) = (-dy, dx);
    let shaft = (len - head).max(0.0);
    let (bx, by) = (ox + dx * shaft, oy + dy * shaft);
    let (tx, ty) = (ox + dx * len, oy + dy * len);
    let hw = width / 2.0;
    [
        (
            (ox + nx * hw, oy + ny * hw),
            (bx + nx * hw, by + ny * hw),
            (ox - nx * hw, oy - ny * hw),
        ),
        (
            (ox - nx * hw, oy - ny * hw),
            (bx + nx * hw, by + ny * hw),
            (bx - nx * hw, by - ny * hw),
        ),
        (
            (bx + nx * width, by + ny * width),
            (tx, ty),
            (bx - nx * width, by - ny * width),
        ),
    ]
}

/// World-space arrow tip for the planet marker: along the facing at
/// an eye-distance-proportional length (readable once the pixels are
/// clamped in [`draw_player_marker`]). Shared by the frame draw and
/// the marker regression tests below.
fn sphere_tip_world(player: &PlayerViewState, pos: Vec3) -> Vec3 {
    let eye_dist = (player.eye() - pos).length().max(1e-6);
    pos + player.facing() * eye_dist * 0.08
}

/// Player marker: center dot at the exact position plus an arrow along
/// the projected facing (`tip`, skipped when it doesn't project or is
/// too short to read — FirstPerson hides it by design: the camera
/// looks along the heading, so screen-up IS the player direction).
fn draw_player_marker(items: &mut UiItems, origin: (f32, f32), tip: Option<(f32, f32)>) {
    let (mx, my) = origin;
    items.solid(
        Rect {
            x: mx - PLAYER_DOT / 2.0,
            y: my - PLAYER_DOT / 2.0,
            w: PLAYER_DOT,
            h: PLAYER_DOT,
        },
        C_PLAYER,
    );
    if let Some((tx, ty)) = tip {
        let (dx, dy) = (tx - mx, ty - my);
        let len = dx.hypot(dy);
        if len > 4.0 {
            let clamped = len.clamp(16.0, 48.0);
            let dir = (dx / len, dy / len);
            for (a, b, c) in arrow_tris(origin, dir, clamped, 4.0, 10.0) {
                items.tri(a, b, c, C_PLAYER);
            }
        }
    }
    items.text("YOU".to_owned(), mx + PLAYER_DOT, my - 6.0, C_PLAYER);
}

/// Target highlight ring radius, pixels.
const TARGET_RING_R: f32 = 9.0;

/// Target/selection ring: square outline around a projected viewport
/// point as four thin UI-pass rects (the galaxy/system map
/// selection-ring geometry). Pure geometry — unit-tested below.
fn draw_target_ring(items: &mut UiItems, center: (f32, f32), r: f32, color: Color) {
    let (x0, y0) = center;
    for rect in [
        Rect {
            x: x0 - r,
            y: y0 - r,
            w: 2.0 * r,
            h: 1.5,
        },
        Rect {
            x: x0 - r,
            y: y0 + r,
            w: 2.0 * r,
            h: 1.5,
        },
        Rect {
            x: x0 - r,
            y: y0 - r,
            w: 1.5,
            h: 2.0 * r,
        },
        Rect {
            x: x0 + r,
            y: y0 - r,
            w: 1.5,
            h: 2.0 * r,
        },
    ] {
        items.solid(rect, color);
    }
}

/// Checkbox square inside a label row.
fn check_box(row: Rect, lh: f32) -> Rect {
    Rect {
        x: row.x,
        y: row.y + (lh - 16.0) / 2.0,
        w: 16.0,
        h: 16.0,
    }
}

/// Widget rects inside the planet screen's left dock (presets + shader
/// + planet overlays). Only built for the Planet View screen.
struct PlanetLeftRects {
    shader_button: Rect,
    density_track: Option<Rect>,
    wire_box: Rect,
    pent_box: Rect,
    seam_box: Rect,
    /// Global camera presets as a 2×2 grid: [Top, Bottom] / [Right, Persp].
    preset_grid: [[Rect; 2]; 2],
}

/// Planet left-dock row plan: widget rects + label rows in draw order.
/// Built with a single cursor so hit-testing and drawing always agree.
/// The density rows only exist in Checker mode (inputs appear only when
/// needed — the cursor flows up when they don't).
struct PlanetLeftPlan {
    rects: PlanetLeftRects,
    view_header: Rect,
    preset_hint: Rect,
    shader_header: Rect,
    shader_hint: Rect,
    density_label: Option<Rect>,
    overlay_header: Rect,
    wire_label: Rect,
    pent_label: Rect,
    seam_label: Rect,
}

fn planet_left_plan(left: Rect, lh: f32, checker: bool) -> PlanetLeftPlan {
    let mut rows = ui::PanelRows::new(left, ui::DOCK_PAD);
    let view_header = rows.next(lh + 6.0, 4.0);
    let preset_hint = rows.next(lh, 4.0);
    let preset_row_top = rows.next(28.0, 6.0);
    let preset_row_bot = rows.next(28.0, 6.0);
    let shader_header = rows.next(lh + 6.0, 4.0);
    let shader_button = rows.next(28.0, 4.0);
    let shader_hint = rows.next(lh, 4.0);
    let (density_label, density_track) = if checker {
        (Some(rows.next(lh, 4.0)), Some(rows.next(20.0, 6.0)))
    } else {
        (None, None)
    };
    let overlay_header = rows.next(lh + 6.0, 4.0);
    let wire_label = rows.next(lh, 4.0);
    let pent_label = rows.next(lh, 4.0);
    let seam_label = rows.next(lh, 4.0);
    let top = ui::split_row_2(preset_row_top, 6.0);
    let bot = ui::split_row_2(preset_row_bot, 6.0);
    PlanetLeftPlan {
        rects: PlanetLeftRects {
            shader_button,
            density_track,
            wire_box: check_box(wire_label, lh),
            pent_box: check_box(pent_label, lh),
            seam_box: check_box(seam_label, lh),
            preset_grid: [[top[0], top[1]], [bot[0], bot[1]]],
        },
        view_header,
        preset_hint,
        shader_header,
        shader_hint,
        density_label,
        overlay_header,
        wire_label,
        pent_label,
        seam_label,
    }
}

/// Widget rects inside the right data dock (params + selection + stats).
/// Only built for the Planet View screen.
struct RightRects {
    subdiv_field: Rect,
    subdiv_track: Rect,
    radius_field: Rect,
    regen_button: Rect,
}

/// Right dock row plan: widget rects + label/text rows in draw order.
/// `warn` reserves the extra above-N=6 warning row; `player_active`
/// reserves the 3 player readout rows (a single "off" line otherwise —
/// walk/cam hints only exist when the player is on).
struct RightPlan {
    rects: RightRects,
    inputs_header: Rect,
    subdiv_label: Rect,
    subdiv_hint: Rect,
    warn_line: Option<Rect>,
    radius_label: Rect,
    radius_hint: Rect,
    selection_header: Rect,
    chunk_lines: [Rect; 4],
    player_lines: Vec<Rect>,
    stats_header: Rect,
    stat_lines: [Rect; 9],
}

fn right_panel_plan(panel: Rect, lh: f32, warn: bool, player_active: bool) -> RightPlan {
    let mut rows = ui::PanelRows::new(panel, ui::DOCK_PAD);
    let inputs_header = rows.next(lh + 6.0, 4.0);
    let subdiv_label = rows.next(lh, 4.0);
    let subdiv_field = rows.next(24.0, 4.0);
    let subdiv_track = rows.next(20.0, 4.0);
    let subdiv_hint = rows.next(lh, 4.0);
    let warn_line = warn.then(|| rows.next(lh, 4.0));
    let radius_label = rows.next(lh, 4.0);
    let radius_field = rows.next(24.0, 4.0);
    let radius_hint = rows.next(lh, 4.0);
    let regen_button = rows.next(28.0, 6.0);
    let selection_header = rows.next(lh + 6.0, 4.0);
    let chunk_lines = [
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
    ];
    let player_lines = if player_active {
        vec![
            rows.next(lh, 4.0),
            rows.next(lh, 4.0),
            rows.next(lh, 4.0),
            rows.next(lh, 4.0),
        ]
    } else {
        vec![rows.next(lh, 4.0)]
    };
    let stats_header = rows.next(lh + 6.0, 4.0);
    let stat_lines = [
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
    ];
    RightPlan {
        rects: RightRects {
            subdiv_field,
            subdiv_track,
            radius_field,
            regen_button,
        },
        inputs_header,
        subdiv_label,
        subdiv_hint,
        warn_line,
        radius_label,
        radius_hint,
        selection_header,
        chunk_lines,
        player_lines,
        stats_header,
        stat_lines,
    }
}

/// One text run for the UI builders below.
struct UiText {
    text: String,
    x: f32,
    baseline: f32,
    color: Color,
}

/// Frame UI: solid rects + triangles + text runs (converted to
/// vertices later). Triangles cover rotated shapes (the player heading
/// arrow) that axis-aligned rects cannot express.
#[derive(Default)]
struct UiItems {
    solids: Vec<(Rect, Color)>,
    tris: Vec<(UiTri, Color)>,
    texts: Vec<UiText>,
}

impl UiItems {
    fn solid(&mut self, rect: Rect, color: Color) {
        self.solids.push((rect, color));
    }

    fn tri(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: Color) {
        self.tris.push(((a, b, c), color));
    }

    fn text(&mut self, text: String, x: f32, baseline: f32, color: Color) {
        self.texts.push(UiText {
            text,
            x,
            baseline,
            color,
        });
    }
}

/// Game Demo tab body: the shipping HUD fed by the live cosmic player
/// (frame/time/target lines from `game::hud`; SOI stays `—` — no
/// handoffs at this scale) plus the camera mode and the flight hint.
/// Zero debug data by construction (the chrome builders run separately
/// and only when their toggles are on). Takes `&mut` for the HUD's SOI
/// hysteresis state.
fn build_demo_ui(items: &mut UiItems, lh: f32, app: &mut DebugApp, area: Rect) {
    let cosmic = &mut app.cosmic;
    let target_label = cosmic.target_label();
    let frame = cosmic.hud.update(&HudInputs {
        ship: &cosmic.player.ship,
        clock: &cosmic.player.clock,
        executor: cosmic.player.exec.as_ref(),
        target_label: target_label.as_deref(),
        soi_weight: 0.0,
        soi_events: &[],
        body_label: None,
    });
    let mode_line = format!("cam:   {} · marker YOU", cosmic.camera.mode().label());
    let speed_line = format!(
        "speed: {:.3} Mpc/s · cruise {:.1}s",
        cosmic.player.real_speed_mpc_s(),
        cosmic.player.cruise.t_cross_s
    );
    let mut rows = ui::PanelRows::new(area, 12.0);
    section_bar(items, rows.next(lh + 6.0, 4.0), "GAME DEMO");
    for (line, color) in [
        (frame.frame_line, C_TEXT),
        (frame.time_line, C_TEXT),
        (
            frame
                .soi
                .map(|soi| soi.message)
                .unwrap_or_else(|| "soi:    —".to_owned()),
            C_DIM,
        ),
        (
            frame
                .target
                .map(|target| target.line)
                .unwrap_or_else(|| "target: —".to_owned()),
            C_DIM,
        ),
        (mode_line, C_TEXT),
        (speed_line, C_TEXT),
    ] {
        text_row(items, lh, rows.next(lh, 6.0), line, color);
    }
    text_row(
        items,
        lh,
        rows.next(lh, 6.0),
        "mouse steer · WASD cruise · click target · E fly-to · Shift+wheel pace · P camera · V vista (any input skips)"
            .to_owned(),
        C_DIM,
    );
}

/// Cosmic Web dimension tab (WS5 — fourth absorbed view): web stats +
/// camera hints on the left, node selection readout on the right. The
/// viewport renders the shared web through the inspector camera (3D
/// pass); this builder only draws chrome + docks. Read-only: no field
/// here mutates journey, transit, or viewer state.
fn build_cosmic_web_ui(atlas: &mut GlyphAtlas, app: &DebugApp, layout: Layout) -> UiItems {
    let cosmic = &app.cosmic;
    let inspector = &app.cosmic_inspector;
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_topbar(&mut items, lh, app, layout);

    // ---- Left dock: COSMIC WEB ----
    if layout.left.w >= 1.0 {
        items.solid(layout.left, C_PANEL_BG);
        let mut rows = ui::PanelRows::new(layout.left, 12.0);
        section_bar(&mut items, rows.next(lh + 6.0, 4.0), "COSMIC WEB");
        text_row(
            &mut items,
            lh,
            rows.next(lh, 6.0),
            format!("seed {}", cosmic.seed),
            C_TEXT,
        );
        text_row(
            &mut items,
            lh,
            rows.next(lh, 6.0),
            format!(
                "{} nodes · {} links · {} glow",
                cosmic.web.nodes.len(),
                cosmic.web.links.len(),
                cosmic.web.glow_mpc.len()
            ),
            C_DIM,
        );
        text_row(
            &mut items,
            lh,
            rows.next(lh, 6.0),
            format!(
                "voids {:.1}% · home node {}",
                cosmic.web.void_fraction * 100.0,
                cosmic.web.home_node
            ),
            C_DIM,
        );
        section_bar(&mut items, rows.next(lh + 6.0, 4.0), "CAMERA");
        text_row(
            &mut items,
            lh,
            rows.next(lh, 6.0),
            format!(
                "target {:+.0},{:+.0},{:+.0} D {:.0} Mpc",
                inspector.camera.target().x,
                inspector.camera.target().y,
                inspector.camera.target().z,
                inspector.camera.distance()
            ),
            C_DIM,
        );
        for hint in [
            "wheel: zoom (log)",
            "left-drag: orbit",
            "right-drag: pan",
            "click: select node",
            "Home: top-down",
            "S: slab mode",
            "Shift+wheel: slab depth",
            "[/]: slab thickness",
        ] {
            text_row(&mut items, lh, rows.next(lh, 6.0), hint.to_owned(), C_DIM);
        }
        // Depth-window readout (`cosmic-depth-window` FR3).
        text_row(
            &mut items,
            lh,
            rows.next(lh, 6.0),
            if inspector.slab.on {
                format!(
                    "slab {} Mpc @ {:.0} Mpc · 20°",
                    inspector.slab.thickness_mpc, inspector.slab.center_mpc
                )
            } else {
                "slab off".to_owned()
            },
            C_DIM,
        );
    }

    // ---- Right dock: SELECTION ----
    if layout.panel.w >= 1.0 {
        items.solid(layout.panel, C_PANEL_BG);
        let mut rows = ui::PanelRows::new(layout.panel, 12.0);
        section_bar(&mut items, rows.next(lh + 6.0, 4.0), "SELECTION");
        match inspector
            .selected
            .and_then(|i| cosmic.web.nodes.get(i as usize))
        {
            Some(node) => {
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    format!(
                        "node {} · tier {}",
                        node.node_index,
                        match game_debug::cosmic_hubs::HubTier::of(
                            node.node_index as usize,
                            cosmic.web.nodes.len()
                        ) {
                            game_debug::cosmic_hubs::HubTier::A => "A",
                            game_debug::cosmic_hubs::HubTier::B => "B",
                            game_debug::cosmic_hubs::HubTier::C => "C",
                        }
                    ),
                    C_TEXT,
                );
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    format!("mass {:.3e} M☉", node.mass_msun),
                    C_DIM,
                );
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    format!("r_vir {:.2} Mpc", node.virial_radius_mpc),
                    C_DIM,
                );
                let links = cosmic
                    .web
                    .links
                    .iter()
                    .filter(|l| l.a == node.node_index || l.b == node.node_index)
                    .count();
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    format!("{links} links"),
                    C_DIM,
                );
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    cosmic.web.node_content_id(node.node_index),
                    C_DIM,
                );
            }
            None => {
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    "click a node".to_owned(),
                    C_DIM,
                );
                let ship = cosmic.player.position_mpc();
                text_row(
                    &mut items,
                    lh,
                    rows.next(lh, 6.0),
                    format!("player {:+.1},{:+.1},{:+.1} Mpc", ship.x, ship.y, ship.z),
                    C_DIM,
                );
            }
        }
    }
    // Target highlight: amber ring around the inspected node,
    // projected through the inspector camera at the fixed web-center
    // origin the tab buffers share (the galaxy-tab ring precedent —
    // clamped into the viewport, hidden behind the camera).
    if let Some(node) = inspector
        .selected
        .and_then(|i| cosmic.web.nodes.get(i as usize))
    {
        let vp = layout.viewport;
        let view_proj = inspector.view_proj(vp.w / vp.h);
        let world = Vec3::new(
            node.position_mpc[0] as f32,
            node.position_mpc[1] as f32,
            node.position_mpc[2] as f32,
        );
        if let Some((sx, sy)) = project_to_screen(world, view_proj, vp) {
            draw_target_ring(
                &mut items,
                (sx.clamp(vp.x, vp.x + vp.w), sy.clamp(vp.y, vp.y + vp.h)),
                TARGET_RING_R,
                C_WARN,
            );
        }
    }
    items
}

/// Placeholder dimension tab (S7): waypoint identity + live status,
/// never a blank page. Periodically re-rendered, so the
/// `INACTIVE` badge always reflects the journey layer.
fn build_placeholder_ui(
    items: &mut UiItems,
    lh: f32,
    app: &DebugApp,
    waypoint: WaypointId,
    area: Rect,
) {
    let mut rows = ui::PanelRows::new(area, 12.0);
    section_bar(
        items,
        rows.next(lh + 6.0, 4.0),
        &format!("DIMENSION: {} (L{})", waypoint.name(), waypoint.number()),
    );
    let is_active = app.active_waypoint() == waypoint;
    for (line, color) in [
        (
            if is_active {
                "INACTIVE badge withheld: this is the active layer".to_owned()
            } else {
                "INACTIVE · showing last state".to_owned()
            },
            if is_active { C_CHECK } else { C_WARN },
        ),
        (
            "no 3D content at this scale yet — placeholder".to_owned(),
            C_DIM,
        ),
        (
            "per-dimension content lands with its feature".to_owned(),
            C_DIM,
        ),
    ] {
        text_row(items, lh, rows.next(lh, 6.0), line, color);
    }
}

/// One clickable Controls row: screen rect + the action it fires.
pub struct ControlsRow {
    pub rect: Rect,
    pub action: Action,
}

/// Settings → Controls section plan (S5): every registry action as
/// a clickable row in two columns (Chrome+Navigate left,
/// Camera+Travel right). Pure function of `(area, lh)` — the click
/// router rebuilds the identical plan, so hit rects match by
/// construction.
pub struct ControlsPlan {
    pub rows: Vec<ControlsRow>,
}

fn controls_plan(area: Rect, lh: f32) -> ControlsPlan {
    use game_debug::actions::{ActionGroup, all_actions};
    let row_h = lh + 4.0;
    let col_w = ((area.w - 24.0) / 2.0).max(0.0);
    let mut rows = Vec::new();
    for (col, groups) in [
        vec![ActionGroup::Chrome, ActionGroup::Navigate],
        vec![ActionGroup::Camera, ActionGroup::Travel],
    ]
    .iter()
    .enumerate()
    {
        let mut y = area.y + 8.0;
        for group in groups {
            // Reserve the group header row above the group's rows.
            y += lh + 14.0;
            for action in all_actions().into_iter().filter(|a| &a.group() == group) {
                let rect = Rect {
                    x: area.x + 8.0 + col as f32 * (col_w + 8.0),
                    y,
                    w: col_w,
                    h: row_h,
                };
                rows.push(ControlsRow { rect, action });
                y += row_h + 2.0;
            }
        }
    }
    ControlsPlan { rows }
}

/// Settings → UNIVERSE section plan (v0.3.2 `settings-seed-loader`):
/// the single editable seed field lives in the Settings left dock —
/// full-width field, full-width Load button (touch-first targets),
/// active-seed hint. Pure function of (`left`, `lh`) — the UI builder
/// and the click router share this, so hit rects match drawn widgets
/// by construction.
pub struct SettingsLeftPlan {
    pub header: Rect,
    pub field: Rect,
    pub load: Rect,
    pub hint: Rect,
}

fn settings_left_plan(left: Rect, lh: f32) -> SettingsLeftPlan {
    let pad = ui::DOCK_PAD;
    let mut y = left.y + pad;
    let header = Rect {
        x: left.x,
        y,
        w: left.w,
        h: lh + 8.0,
    };
    y += lh + 12.0;
    let row = |y: &mut f32, h: f32| {
        let rect = Rect {
            x: left.x + pad,
            y: *y,
            w: (left.w - 2.0 * pad).max(0.0),
            h,
        };
        *y += h + 4.0;
        rect
    };
    let field = row(&mut y, lh + 6.0);
    let load = row(&mut y, lh + 6.0);
    let hint = row(&mut y, lh);
    SettingsLeftPlan {
        header,
        field,
        load,
        hint,
    }
}

/// Settings tab body: UNIVERSE seed editor (left dock — the single
/// editable seed field in the shell) + Controls section rendering
/// the registry in the viewport.
fn build_settings_ui(items: &mut UiItems, lh: f32, app: &DebugApp, layout: Layout) {
    if layout.left.w >= 1.0 {
        items.solid(layout.left, C_PANEL_BG);
        let dock = settings_left_plan(layout.left, lh);
        section_bar(items, dock.header, "UNIVERSE");
        items.solid(dock.field, C_FIELD_BG);
        text_row(
            items,
            lh,
            dock.field,
            app.settings.seed_field.text.clone(),
            C_TEXT,
        );
        let load_ok = app.settings.seed_field.text.parse::<u64>().is_ok();
        items.solid(dock.load, if load_ok { C_BTN } else { C_BTN_OFF });
        items.text(
            "Load [Enter]".to_owned(),
            dock.load.x + 8.0,
            dock.load.y + lh - 2.0,
            if load_ok { C_TEXT } else { C_DIM },
        );
        text_row(
            items,
            lh,
            dock.hint,
            format!("universe {} · R re-rolls · Enter applies", app.galaxy.seed),
            C_DIM,
        );
    }
    let plan = controls_plan(layout.viewport, lh);
    let mut last_group: Option<ActionGroup> = None;
    for row in &plan.rows {
        if Some(row.action.group()) != last_group {
            last_group = Some(row.action.group());
            section_bar(
                items,
                Rect {
                    x: row.rect.x,
                    y: row.rect.y - lh - 10.0,
                    w: row.rect.w,
                    h: lh + 6.0,
                },
                row.action.group().title(),
            );
        }
        items.solid(row.rect, C_FIELD_BG);
        items.text(
            row.action.button_label(),
            row.rect.x + 8.0,
            row.rect.y + lh - 3.0,
            C_TEXT,
        );
    }
}

/// One panel text row.
fn text_row(items: &mut UiItems, lh: f32, row: Rect, text: String, color: Color) {
    items.text(text, row.x, row.y + lh - 4.0, color);
}

/// Section header bar.
fn section_bar(items: &mut UiItems, row: Rect, title: &str) {
    items.solid(row, C_SECTION_BG);
    items.text(title.to_owned(), row.x + 6.0, row.y + row.h - 5.0, C_TEXT);
}

/// One overlay checkbox row.
fn checkbox_row(
    items: &mut UiItems,
    lh: f32,
    box_rect: Rect,
    label_row: Rect,
    label: &str,
    checked: bool,
) {
    items.solid(box_rect, C_FIELD_BG);
    if checked {
        items.solid(
            Rect {
                x: box_rect.x + 3.0,
                y: box_rect.y + 3.0,
                w: 10.0,
                h: 10.0,
            },
            C_CHECK,
        );
    }
    items.text(
        label.to_owned(),
        box_rect.x + 22.0,
        label_row.y + lh - 4.0,
        C_TEXT,
    );
}

/// Rects for the shared shader selector rows (see [`shader_rows`]).
struct ShaderRowRects {
    header: Rect,
    hint: Rect,
    button: Rect,
    density_label: Option<Rect>,
    density_track: Option<Rect>,
}

/// Shader selector rows on the planet screen: cycle button + usage
/// hint + checker-density slider (only in Checker mode — inputs appear
/// only when needed).
fn shader_rows(items: &mut UiItems, lh: f32, viewer: &PlanetViewerState, rects: &ShaderRowRects) {
    section_bar(items, rects.header, "SHADER");
    items.solid(rects.button, C_BTN);
    items.text(
        format!(
            "Shader: {} ({}/{})",
            viewer.debug_mode.title(),
            viewer.debug_mode.index() + 1,
            DebugMode::ALL.len(),
        ),
        rects.button.x + 12.0,
        rects.button.y + 19.0,
        C_TEXT,
    );
    text_row(
        items,
        lh,
        rects.hint,
        "click cycles · keys 1-6".to_owned(),
        C_DIM,
    );
    if let (Some(label), Some(track)) = (rects.density_label, rects.density_track) {
        text_row(
            items,
            lh,
            label,
            format!("Checker density: {}", viewer.checker_density),
            C_DIM,
        );
        items.solid(track, C_TRACK);
        items.solid(
            Rect {
                x: viewer.density_slider.knob_x(track) - 5.0,
                y: track.y + 1.0,
                w: 10.0,
                h: track.h - 2.0,
            },
            C_KNOB,
        );
    }
}

/// Planet View UI: left dock (camera presets + shader + planet
/// overlays) + shared right data dock (the 3D draws separately). Each
/// dock groups its widgets under section bars so controls, params,
/// selection and stats stay visually separate instead of one long
/// undifferentiated column.
fn build_planet_ui(
    atlas: &mut GlyphAtlas,
    app: &DebugApp,
    sky: &SkySummary,
    layout: Layout,
) -> UiItems {
    let viewer = &app.viewer;
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_topbar(&mut items, lh, app, layout);

    // ---- Left dock: VIEW (camera presets — planet content only) ----
    // Hidden docks draw nothing (zero-width rects would otherwise leak
    // text rows over the viewport).
    if layout.left.w >= 1.0 {
        let checker = viewer.debug_mode == DebugMode::Checker;
        let left = planet_left_plan(layout.left, lh, checker);
        items.solid(layout.left, C_PANEL_BG);
        section_bar(&mut items, left.view_header, "VIEW");
        text_row(
            &mut items,
            lh,
            left.preset_hint,
            "Camera presets (G/T/B/R)".to_owned(),
            C_DIM,
        );
        // Global camera presets as a 2×2 grid: bigger hit targets than the
        // old 4-in-a-row strip (`G`/`T`/`B`/`R` do the same).
        let preset_labels = [GlobalPreset::ALL[0].1, GlobalPreset::ALL[1].1];
        let preset_labels_bot = [GlobalPreset::ALL[2].1, GlobalPreset::ALL[3].1];
        for (row, labels) in left
            .rects
            .preset_grid
            .iter()
            .zip([preset_labels, preset_labels_bot])
        {
            for (rect, label) in row.iter().zip(labels) {
                items.solid(*rect, C_BTN);
                items.text(label.to_owned(), rect.x + 8.0, rect.y + 19.0, C_TEXT);
            }
        }

        // ---- Left dock: SHADER + OVERLAYS (planet-relevant only) ----
        shader_rows(
            &mut items,
            lh,
            viewer,
            &ShaderRowRects {
                header: left.shader_header,
                hint: left.shader_hint,
                button: left.rects.shader_button,
                density_label: left.density_label,
                density_track: left.rects.density_track,
            },
        );
        section_bar(&mut items, left.overlay_header, "OVERLAYS");
        for (box_rect, label_row, label, checked) in [
            (
                left.rects.wire_box,
                left.wire_label,
                "Wireframe",
                viewer.wireframe,
            ),
            (
                left.rects.pent_box,
                left.pent_label,
                "Pentagons",
                viewer.pentagons,
            ),
            (left.rects.seam_box, left.seam_label, "Seams", viewer.seams),
        ] {
            checkbox_row(&mut items, lh, box_rect, label_row, label, checked);
        }
    }

    if layout.panel.w >= 1.0 {
        build_right_dock(&mut items, lh, viewer, sky, layout.panel);
    }
    items
}

/// Galaxy Map UI (universe-maps): left dock (map info + selection
/// readout + hints) + selection ring overlay in the viewport.
/// `build_right_dock` stays sphere-specific; the map's right dock shows
/// the selected star.
struct GalaxyLeftPlan {
    header: Rect,
    seed_row: Rect,
    stats_row: Rect,
    cam_row: Rect,
    cam_header: Rect,
    hints: [Rect; 6],
}

/// Left-dock row layout for the galaxy screen, derived from
/// (`left`, `lh`) alone — the UI builder and the click router share
/// this, so hit rects match drawn widgets by construction.
fn galaxy_left_plan(left: Rect, lh: f32) -> GalaxyLeftPlan {
    let pad = ui::DOCK_PAD;
    let mut y = left.y + pad;
    let row = |y: &mut f32| {
        let rect = Rect {
            x: left.x + pad,
            y: *y,
            w: (left.w - 2.0 * pad).max(0.0),
            h: lh,
        };
        *y += lh + 4.0;
        rect
    };
    let bar = |y: &mut f32| {
        let rect = Rect {
            x: left.x,
            y: *y,
            w: left.w,
            h: lh + 8.0,
        };
        *y += lh + 12.0;
        rect
    };
    let header = bar(&mut y);
    let seed_row = row(&mut y);
    let stats_row = row(&mut y);
    let cam_row = row(&mut y);
    y += 4.0;
    let cam_header = bar(&mut y);
    let hints = [
        row(&mut y),
        row(&mut y),
        row(&mut y),
        row(&mut y),
        row(&mut y),
        row(&mut y),
    ];
    GalaxyLeftPlan {
        header,
        seed_row,
        stats_row,
        cam_row,
        cam_header,
        hints,
    }
}

fn build_galaxy_ui(atlas: &mut GlyphAtlas, app: &DebugApp, layout: Layout) -> UiItems {
    let galaxy = &app.galaxy;
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_topbar(&mut items, lh, app, layout);

    // ---- Left dock: MAP ---- (hidden docks draw nothing)
    if layout.left.w >= 1.0 {
        items.solid(layout.left, C_PANEL_BG);
        let plan = galaxy_left_plan(layout.left, lh);
        section_bar(&mut items, plan.header, "GALAXY MAP");
        text_row(
            &mut items,
            lh,
            plan.seed_row,
            format!("seed {}", galaxy.seed),
            C_TEXT,
        );
        text_row(
            &mut items,
            lh,
            plan.stats_row,
            format!(
                "{} stars · {} nebulae",
                galaxy.galaxy.stars.len(),
                galaxy.nebulae.len()
            ),
            C_DIM,
        );
        text_row(
            &mut items,
            lh,
            plan.cam_row,
            format!(
                "target {:+.0},{:+.0},{:+.0} D {:.0} ly",
                galaxy.camera.target().x,
                galaxy.camera.target().y,
                galaxy.camera.target().z,
                galaxy.camera.distance()
            ),
            C_DIM,
        );
        section_bar(&mut items, plan.cam_header, "CAMERA");
        for (rect, hint) in plan.hints.iter().zip([
            "wheel: zoom (log)",
            "left-drag: orbit",
            "right-drag: pan",
            "click: select star",
            "Home: top-down",
            "R: re-roll seed",
        ]) {
            text_row(&mut items, lh, *rect, hint.to_owned(), C_DIM);
        }
    }

    // ---- Right dock: SELECTION ----
    if layout.panel.w >= 1.0 {
        let pad = ui::DOCK_PAD;
        items.solid(layout.panel, C_PANEL_BG);
        let mut py = layout.panel.y + pad;
        let prow = |py: &mut f32| {
            let rect = Rect {
                x: layout.panel.x + pad,
                y: *py,
                w: (layout.panel.w - 2.0 * pad).max(0.0),
                h: lh,
            };
            *py += lh + 4.0;
            rect
        };
        section_bar(
            &mut items,
            Rect {
                x: layout.panel.x,
                y: layout.panel.y,
                w: layout.panel.w,
                h: lh + 8.0,
            },
            "SELECTION",
        );
        py += lh + 12.0;
        match galaxy
            .selected
            .and_then(|i| galaxy.galaxy.stars.get(i as usize))
        {
            Some(star) => {
                text_row(
                    &mut items,
                    lh,
                    prow(&mut py),
                    format!("star {} · {:?}", star.star_index, star.spectral_class),
                    C_TEXT,
                );
                text_row(
                    &mut items,
                    lh,
                    prow(&mut py),
                    format!(
                        "pos {:+.0},{:+.0},{:+.0} ly",
                        star.position_ly[0], star.position_ly[1], star.position_ly[2]
                    ),
                    C_DIM,
                );
                text_row(
                    &mut items,
                    lh,
                    prow(&mut py),
                    format!("companions {}", star.companion_count),
                    C_DIM,
                );
            }
            None => {
                text_row(
                    &mut items,
                    lh,
                    prow(&mut py),
                    "click a star".to_owned(),
                    C_DIM,
                );
            }
        }
    }

    // ---- Viewport: selection ring around the picked star ----
    // Projected through the 3D view; hidden when the star is behind
    // the camera.
    if let Some(star) = galaxy
        .selected
        .and_then(|i| galaxy.galaxy.stars.get(i as usize))
    {
        let vp = layout.viewport;
        let view_proj = galaxy.camera.view_proj(vp.w / vp.h);
        if let Some((sx, sy)) = project_to_screen(star_world(star), view_proj, vp) {
            // Ring as four thin rects (matches the panel aesthetic).
            // Clamped into the viewport.
            let r = 7.0;
            let x0 = sx.clamp(vp.x, vp.x + vp.w);
            let y0 = sy.clamp(vp.y, vp.y + vp.h);
            for rect in [
                Rect {
                    x: x0 - r,
                    y: y0 - r,
                    w: 2.0 * r,
                    h: 1.5,
                },
                Rect {
                    x: x0 - r,
                    y: y0 + r,
                    w: 2.0 * r,
                    h: 1.5,
                },
                Rect {
                    x: x0 - r,
                    y: y0 - r,
                    w: 1.5,
                    h: 2.0 * r,
                },
                Rect {
                    x: x0 + r,
                    y: y0 - r,
                    w: 1.5,
                    h: 2.0 * r,
                },
            ] {
                items.solid(rect, C_KNOB);
            }
        }
    }
    items
}

/// System Map UI (universe-maps): left dock (system info + camera +
/// journey layer + hints) + selection/focus ring overlays in the
/// viewport. Right dock shows the selected planet, or the travel offer
/// while one is armed.
fn build_system_ui(atlas: &mut GlyphAtlas, app: &DebugApp, layout: Layout) -> UiItems {
    let system = &app.system;
    let journey = &app.journey;
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_topbar(&mut items, lh, app, layout);

    // ---- Left dock: SYSTEM ---- (hidden docks draw nothing)
    if layout.left.w >= 1.0 {
        let pad = ui::DOCK_PAD;
        items.solid(layout.left, C_PANEL_BG);
        let mut y = layout.left.y + pad;
        let row = |y: &mut f32| {
            let rect = Rect {
                x: layout.left.x + pad,
                y: *y,
                w: (layout.left.w - 2.0 * pad).max(0.0),
                h: lh,
            };
            *y += lh + 4.0;
            rect
        };
        section_bar(
            &mut items,
            Rect {
                x: layout.left.x,
                y: layout.left.y,
                w: layout.left.w,
                h: lh + 8.0,
            },
            "SYSTEM MAP",
        );
        y += lh + 12.0;
        let star = &system.system.star;
        text_row(
            &mut items,
            lh,
            row(&mut y),
            format!("star {} · {:?}", star.star_index, star.spectral_class),
            C_TEXT,
        );
        text_row(
            &mut items,
            lh,
            row(&mut y),
            format!(
                "{} planets · seed {}",
                system.system.planets.len(),
                system.seed
            ),
            C_DIM,
        );
        text_row(
            &mut items,
            lh,
            row(&mut y),
            format!(
                "target {:+.2},{:+.2},{:+.2} D {:.2} AU",
                system.camera.target().x,
                system.camera.target().y,
                system.camera.target().z,
                system.camera.distance()
            ),
            C_DIM,
        );
        text_row(
            &mut items,
            lh,
            row(&mut y),
            format!("journey: {:?}", journey.active_layer()),
            C_DIM,
        );
        if let Some(focus) = system.focus {
            text_row(
                &mut items,
                lh,
                row(&mut y),
                format!("FOCUS planet {focus} (L4)"),
                C_CHECK,
            );
        }
        y += 4.0;
        section_bar(
            &mut items,
            Rect {
                x: layout.left.x,
                y,
                w: layout.left.w,
                h: lh + 8.0,
            },
            "TRAVEL",
        );
        y += lh + 12.0;
        for hint in [
            "wheel: zoom (log)",
            "left-drag: orbit",
            "right-drag: pan",
            "click: select planet",
            "Home: top-down",
            "F: focus planet (L4)",
            "T: offer / cancel",
            "E: begin transit",
            "Q: back to galaxy",
        ] {
            text_row(&mut items, lh, row(&mut y), hint.to_owned(), C_DIM);
        }
    }

    // ---- Right dock: SELECTION / travel offer ----
    if layout.panel.w >= 1.0 {
        let pad = ui::DOCK_PAD;
        items.solid(layout.panel, C_PANEL_BG);
        let mut py = layout.panel.y + pad;
        let prow = |py: &mut f32| {
            let rect = Rect {
                x: layout.panel.x + pad,
                y: *py,
                w: (layout.panel.w - 2.0 * pad).max(0.0),
                h: lh,
            };
            *py += lh + 4.0;
            rect
        };
        section_bar(
            &mut items,
            Rect {
                x: layout.panel.x,
                y: layout.panel.y,
                w: layout.panel.w,
                h: lh + 8.0,
            },
            "SELECTION",
        );
        py += lh + 12.0;
        if let Some(offer) = system.travel_offer {
            text_row(
                &mut items,
                lh,
                prow(&mut py),
                format!("TRAVEL OFFER - planet {offer}"),
                C_TEXT,
            );
            // Deferred cost hook (UMAP-019): computed off the target orbit,
            // displayed, never deducted — the M4 resource model consumes it.
            let orbit = system
                .system
                .planets
                .get(offer as usize)
                .map(|p| p.orbit_radius_au)
                .unwrap_or(0.0);
            let cost = plan_cost(orbit);
            text_row(
                &mut items,
                lh,
                prow(&mut py),
                format!(
                    "fuel {:.1} · energy {:.1} (deferred M4)",
                    cost.fuel, cost.energy
                ),
                C_DIM,
            );
            match system.transit.as_ref() {
                Some(transit) => {
                    let secs = transit.remaining_ticks() as f32 * SIM_DT_SECS;
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        format!("TRANSIT {secs:.1}s · [T] cancel"),
                        C_TEXT,
                    );
                    let bar = Rect {
                        x: layout.panel.x + pad,
                        y: py,
                        w: (layout.panel.w - 2.0 * pad).max(0.0),
                        h: 8.0,
                    };
                    items.solid(bar, C_TRACK);
                    items.solid(
                        Rect {
                            w: bar.w * transit.progress(),
                            ..bar
                        },
                        C_KNOB,
                    );
                }
                None => {
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        "[E] begin · [T] withdraw".to_owned(),
                        C_DIM,
                    );
                }
            }
        } else {
            match system
                .selected
                .and_then(|i| system.system.planets.get(i as usize))
            {
                Some(planet) => {
                    let d = &planet.descriptor;
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        format!("planet {} · {:?}", d.id.planet_index(), d.planet_type),
                        C_TEXT,
                    );
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        format!(
                            "orbit {:.2} AU · R {:.1} km",
                            planet.orbit_radius_au, d.radius_km
                        ),
                        C_DIM,
                    );
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        format!(
                            "g {:.2} · atm {:.2} · moons {}",
                            d.gravity_g, d.atmosphere.density, d.companion_count
                        ),
                        C_DIM,
                    );
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        format!(
                            "E {:.1} M {:.1} W {:.1} O {:.1} R {:.1}",
                            d.resource_bias.energy,
                            d.resource_bias.metal,
                            d.resource_bias.water_ice,
                            d.resource_bias.organics,
                            d.resource_bias.rare
                        ),
                        C_DIM,
                    );
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        "[T] travel offer".to_owned(),
                        C_DIM,
                    );
                }
                None => {
                    text_row(
                        &mut items,
                        lh,
                        prow(&mut py),
                        "click a planet".to_owned(),
                        C_DIM,
                    );
                }
            }
        }
    }

    // ---- Viewport: selection + focus rings ----
    // Projected through the 3D view; a ring hides when its planet is
    // behind the camera.
    let vp = layout.viewport;
    let view_proj = system.camera.view_proj(vp.w / vp.h);
    let mut ring = |planet_index: u32, color: Color| {
        let planet = &system.system.planets[planet_index as usize];
        let (px, py, pz) = planet_slot(planet.orbit_radius_au, planet_index);
        let Some((sx, sy)) =
            project_to_screen(Vec3::new(px as f32, py as f32, pz as f32), view_proj, vp)
        else {
            return;
        };
        let r = 9.0;
        let x0 = sx.clamp(vp.x, vp.x + vp.w);
        let y0 = sy.clamp(vp.y, vp.y + vp.h);
        for rect in [
            Rect {
                x: x0 - r,
                y: y0 - r,
                w: 2.0 * r,
                h: 1.5,
            },
            Rect {
                x: x0 - r,
                y: y0 + r,
                w: 2.0 * r,
                h: 1.5,
            },
            Rect {
                x: x0 - r,
                y: y0 - r,
                w: 1.5,
                h: 2.0 * r,
            },
            Rect {
                x: x0 + r,
                y: y0 - r,
                w: 1.5,
                h: 2.0 * r,
            },
        ] {
            items.solid(rect, color);
        }
    };
    if let Some(selected) = system.selected
        && system.system.planets.get(selected as usize).is_some()
    {
        ring(selected, C_KNOB);
    }
    if let Some(focus) = system.focus
        && Some(focus) != system.selected
        && system.system.planets.get(focus as usize).is_some()
    {
        ring(focus, C_CHECK);
    }
    items
}

/// Right data dock on the planet screen: INPUTS holds mesh params,
/// SELECTION holds chunk + player, STATS is read-only. The player block
/// shrinks to a single "off" line when the player is off, so walk/cam
/// key hints only exist when the player is on.
fn build_right_dock(
    items: &mut UiItems,
    lh: f32,
    viewer: &PlanetViewerState,
    sky: &SkySummary,
    panel: Rect,
) {
    let warn = parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
    let plan = right_panel_plan(panel, lh, warn, viewer.player.active);
    items.solid(panel, C_PANEL_BG);

    // ---- Right dock: INPUTS ----
    let rrects = &plan.rects;
    section_bar(items, plan.inputs_header, "INPUTS");
    text_row(
        items,
        lh,
        plan.subdiv_label,
        "Subdivisions (0-8)".to_owned(),
        C_DIM,
    );

    // Subdivisions field + slider + live cost hint.
    items.solid(rrects.subdiv_field, C_FIELD_BG);
    items.text(
        viewer.subdiv_field.text.clone(),
        rrects.subdiv_field.x + 6.0,
        rrects.subdiv_field.y + 17.0,
        C_TEXT,
    );
    items.solid(rrects.subdiv_track, C_TRACK);
    items.solid(
        Rect {
            x: viewer.subdiv_slider.knob_x(rrects.subdiv_track) - 5.0,
            y: rrects.subdiv_track.y + 1.0,
            w: 10.0,
            h: rrects.subdiv_track.h - 2.0,
        },
        C_KNOB,
    );
    match parse_subdivisions(&viewer.subdiv_field.text) {
        Ok(n) => text_row(
            items,
            lh,
            plan.subdiv_hint,
            format!("→ {} cells", fmt_int(cell_count_hint(n))),
            C_DIM,
        ),
        Err(error) => text_row(items, lh, plan.subdiv_hint, error.hint().to_owned(), C_ERR),
    }
    if let Some(warn_row) = plan.warn_line {
        text_row(
            items,
            lh,
            warn_row,
            "above N=6: seconds per regen".to_owned(),
            C_WARN,
        );
    }
    text_row(
        items,
        lh,
        plan.radius_label,
        "Radius (> 0)".to_owned(),
        C_DIM,
    );

    // Radius field + validation hint.
    items.solid(rrects.radius_field, C_FIELD_BG);
    items.text(
        viewer.radius_field.text.clone(),
        rrects.radius_field.x + 6.0,
        rrects.radius_field.y + 17.0,
        C_TEXT,
    );
    if let Err(error) = parse_radius(&viewer.radius_field.text) {
        text_row(items, lh, plan.radius_hint, error.hint().to_owned(), C_ERR);
    } else {
        text_row(
            items,
            lh,
            plan.radius_hint,
            "Enter = defocus".to_owned(),
            C_DIM,
        );
    }

    // Regenerate (dimmed while invalid; clicks ignored then).
    let ok = viewer.can_regenerate();
    items.solid(rrects.regen_button, if ok { C_BTN } else { C_BTN_OFF });
    items.text(
        "Regenerate".to_owned(),
        rrects.regen_button.x + 12.0,
        rrects.regen_button.y + 19.0,
        if ok { C_TEXT } else { C_DIM },
    );

    // ---- Right dock: SELECTION (chunk + player readouts) ----
    section_bar(items, plan.selection_header, "SELECTION");
    // Chunk hover/pin readout (cell-chunks): the pin wins over a
    // fleeting hover; nothing selected shows em-dashes.
    let shown = viewer.shown_chunk();
    let chunk_line = match shown {
        None => "chunk:     —".to_owned(),
        Some(chunk) => format!("chunk:     {}", fmt_int(chunk.index() as usize)),
    };
    let (type_line, neigh_line) = match shown.and_then(|chunk| viewer.chunk_sides(chunk)) {
        None => ("type:      —".to_owned(), "neighbors: —".to_owned()),
        Some(5) => ("type:      pentagon".to_owned(), "neighbors: 5".to_owned()),
        Some(6) => ("type:      hexagon".to_owned(), "neighbors: 6".to_owned()),
        Some(_) => ("type:      ?".to_owned(), "neighbors: ?".to_owned()),
    };
    let state_line = match (viewer.pinned, viewer.hovered) {
        (Some(_), _) => "state:     pinned".to_owned(),
        (None, Some(_)) => "state:     hover".to_owned(),
        (None, None) => "state:     —".to_owned(),
    };
    // Player overlay readout: lon/lat/heading, camera mode, streamed
    // chunk count + the walk keys — all only when the player is on
    // (`U` toggles). Off state is a single line.
    let player = &viewer.player;
    let player_rows: Vec<String> = if player.active {
        let (lon, lat) = player.lon_lat_deg();
        vec![
            format!("lon:      {lon:7.2} deg"),
            format!("lat:      {lat:7.2} deg"),
            format!(
                "cam: {} +{} hdg {:5.1}",
                short_mode(player.mode()),
                player.loaded_count(),
                player.heading_deg()
            ),
            "U:off WASD:move P:cam".to_owned(),
        ]
    } else {
        vec!["player:   off (U)".to_owned()]
    };
    for (row, line) in plan.chunk_lines.iter().chain(plan.player_lines.iter()).zip(
        [chunk_line, type_line, neigh_line, state_line]
            .into_iter()
            .chain(player_rows),
    ) {
        text_row(items, lh, *row, line, C_TEXT);
    }

    // ---- Right dock: STATS (read-only) ----
    section_bar(items, plan.stats_header, "STATS");
    let stats = &viewer.stats;
    let sky_mode = if sky.model_only { "model" } else { "tiles" };
    for (row, line) in plan.stat_lines.iter().zip([
        format!("cells:     {}", fmt_int(stats.cells)),
        format!("corners:    {}", fmt_int(stats.corners)),
        format!("pentagons:  {}", stats.pentagons),
        format!("hash:       {}", stats.hash8),
        format!("gen:        {:.1} ms", stats.gen_ms),
        format!("view:       {}", viewer.debug_mode.title()),
        // Catalog sky (star-catalog-streaming, dev-only): resident
        // catalog tiles vs fallback tiles, expanded stars + cache,
        // loader latency. Never player-facing.
        format!(
            "sky:       cat {} fb {} ({})",
            fmt_int(sky.resident_tiles),
            fmt_int(sky.fallback_tiles),
            sky_mode
        ),
        format!(
            "sky:       {} stars {:.1} MB",
            fmt_int(sky.resident_stars + sky.fallback_stars),
            sky.cache_bytes as f64 / 1_048_576.0
        ),
        format!("sky:       p50 {:.0} p95 {:.0} ms", sky.p50_ms, sky.p95_ms),
    ]) {
        text_row(items, lh, *row, line, C_TEXT);
    }
}

/// Tools window UI: tab nav + per-tab content. FPS shows the live
/// recorder; Transitions shows the waypoint descriptor/event surface.
/// Unified top bar (ADR-022): three items — Game Demo, Dimensions
/// (live breadcrumb of the active journey layer), Settings. Always
/// visible. Pure builder: the click router shares
/// [`ui::topbar_button`], so hit rects match drawn buttons by
/// construction.
fn build_topbar(items: &mut UiItems, lh: f32, app: &DebugApp, layout: Layout) {
    items.solid(layout.nav, C_NAV_BG);
    let active = app.active_waypoint();
    let titles = [
        "GAME DEMO".to_owned(),
        format!("DIMENSIONS: {} ●", active.name()),
        "SETTINGS".to_owned(),
    ];
    let current = match app.screen {
        Screen::GameDemo => 0,
        Screen::Dimensions(_) => 1,
        Screen::Settings => 2,
    };
    for (i, title) in titles.iter().enumerate() {
        let rect = ui::topbar_button(layout.nav, i);
        if i == current {
            items.solid(rect, C_TAB_ACTIVE);
        }
        items.text(
            title.clone(),
            rect.x + ui::TOP_PAD,
            rect.y + (rect.h + lh) / 2.0 - 3.0,
            C_TEXT,
        );
    }
    // Corner strip lives inside the bar, right-aligned.
    build_corner_strip_in_bar(items, lh, app, layout.nav.w);
    // The dropdown is NOT built here: `ui_items_to_vertices` draws all
    // solids in push order before any text, so a menu emitted with the
    // bar would land UNDER the dock/content solids pushed after it.
    // `draw_main` appends it dead last instead (topmost chrome — the
    // click handler tests it first for the same reason).
}

/// Dimensions dropdown: the ten waypoints largest-first with the
/// active-layer marker, the selected entry highlighted, the hovered
/// row lifted, row separators, and the digit key per row (S1/S5).
/// `draw_main` appends these items after every other chrome surface,
/// so the near-opaque alpha-blended panel + drop shadow always sit on
/// top — matching the click handler, which hit-tests the dropdown
/// first (topmost surface wins).
fn build_dropdown(
    items: &mut UiItems,
    atlas: &mut GlyphAtlas,
    app: &DebugApp,
    nav: Rect,
    cursor: Option<(f32, f32)>,
) {
    let lh = atlas.line_height();
    let panel = ui::dropdown_panel(nav);
    // Drop shadow: a soft dark slab offset down-right behind the panel,
    // so the menu reads as floating above the scene.
    items.solid(
        Rect {
            x: panel.x + 4.0,
            y: panel.y + 6.0,
            w: panel.w,
            h: panel.h,
        },
        C_DROPDOWN_SHADOW,
    );
    items.solid(panel, C_DROPDOWN_BG);
    // 1px border outline (four thin rects).
    for rect in [
        Rect {
            x: panel.x,
            y: panel.y,
            w: panel.w,
            h: 1.5,
        },
        Rect {
            x: panel.x,
            y: panel.y + panel.h - 1.5,
            w: panel.w,
            h: 1.5,
        },
        Rect {
            x: panel.x,
            y: panel.y,
            w: 1.5,
            h: panel.h,
        },
        Rect {
            x: panel.x + panel.w - 1.5,
            y: panel.y,
            w: 1.5,
            h: panel.h,
        },
    ] {
        items.solid(rect, C_DROPDOWN_BORDER);
    }
    let hovered = cursor.and_then(|(cx, cy)| {
        (0..10).find(|&index| ui::dropdown_row(panel, index).contains(cx, cy))
    });
    for (index, (waypoint, is_active)) in app.dropdown_rows().iter().enumerate() {
        let rect = ui::dropdown_row(panel, index);
        let selected = app.screen == Screen::Dimensions(*waypoint);
        if selected {
            items.solid(rect, C_TAB_ACTIVE);
        } else if hovered == Some(index) {
            items.solid(rect, C_DROPDOWN_HOVER);
        }
        // Hairline separator between rows (not above the first).
        if index > 0 {
            items.solid(
                Rect {
                    x: rect.x,
                    y: rect.y,
                    w: rect.w,
                    h: 1.0,
                },
                C_DROPDOWN_SEP,
            );
        }
        let digit = match digit_for_dimension_index(index) {
            Some(0) => "0".to_owned(),
            Some(d) => d.to_string(),
            None => "?".to_owned(),
        };
        let status = if *is_active {
            "active frame ●"
        } else {
            "inactive · last state"
        };
        let baseline = rect.y + (rect.h + lh) / 2.0 - 3.0;
        // Column layout: digit key (dim) · waypoint name · right-aligned
        // status. The active frame row goes green; the rest stay neutral.
        let ink = if *is_active { C_CHECK } else { C_TEXT };
        items.text(digit, rect.x + 2.0, baseline, C_DIM);
        items.text(waypoint.name().to_owned(), rect.x + 26.0, baseline, ink);
        let status_x = rect.x + rect.w - atlas.measure(status).0 - 4.0;
        items.text(
            status.to_owned(),
            status_x,
            baseline,
            if *is_active { C_CHECK } else { C_DIM },
        );
    }
}

/// Overlay chrome shared by every tab: transition strip + dev widget
/// go into `items`; the Dimensions dropdown goes into the separate
/// `drop` buffer, which `draw_main` uploads and draws after ALL other
/// UI — so the menu is topmost by GPU command order, not just by push
/// order within one buffer (no later content can ever cover it).
/// This matches the click handler, which hit-tests the dropdown first
/// (topmost surface wins). Extracted from `draw_main` so tests can
/// assert the composition headlessly.
fn compose_overlay_ui(
    items: &mut UiItems,
    drop: &mut UiItems,
    atlas: &mut GlyphAtlas,
    app: &DebugApp,
    layout: Layout,
    win: (f32, f32),
    cursor: Option<(f32, f32)>,
) {
    let (win_w, win_h) = win;
    let lh = atlas.line_height();
    build_transition_strip(items, lh, app, win_w, win_h);
    build_widget(items, lh, app, win_w, win_h);
    if app.dropdown_open {
        build_dropdown(drop, atlas, app, layout.nav, cursor);
    }
    // Player dot: the walker projected through the main-view matrices
    // (planet content only).
    if app.viewer.player.active && app.screen_content() == Some(ViewContent::PlanetView) {
        let player = &app.viewer.player;
        let vp = layout.viewport;
        let main_vp = player.projection_matrix(vp.w / vp.h) * player.view_matrix();
        // Facing tip sized to a readable pixel length (eye-distance
        // proportional, then clamped).
        if let Some(origin) = world_to_pixels(main_vp, player.position(), vp) {
            let tip = world_to_pixels(main_vp, sphere_tip_world(player, player.position()), vp);
            draw_player_marker(items, origin, tip);
        }
    }
    // Cosmic marker: the ship projected through the player camera
    // (demo tab only — the WS5 inspector shows the player point
    // instead). Marker and scene share the upload-origin frame, so the
    // dot rides the web with no relative jitter.
    if app.screen == Screen::GameDemo {
        let cosmic = &app.cosmic;
        let vp = layout.viewport;
        let main_vp = cosmic.camera.view_proj(vp.w / vp.h);
        let ship_f64 = cosmic.player.position_mpc();
        let ship = recenter(ship_f64, cosmic.upload_origin);
        if let Some(dot) = world_to_pixels(main_vp, ship, vp) {
            let eye_dist = (cosmic.camera.eye_world() - ship_f64).length() as f32;
            let facing = cosmic.player.facing();
            let facing_f32 = Vec3::new(facing.x as f32, facing.y as f32, facing.z as f32);
            let tip = world_to_pixels(main_vp, cosmic_tip_world(ship, facing_f32, eye_dist), vp);
            draw_player_marker(items, dot, tip);
        }
        // Target highlight (UX-3 affordance): amber ring around the
        // selected fly-to node, projected through the player camera in
        // the upload-origin frame the buffers share. Hidden when the
        // node is behind the camera or off-screen — same rule as the
        // ship marker. The ring tracks `target_node`, so it rides the
        // destination through `E` fly-to until arrival / cancel /
        // re-click clears it.
        if let Some(node) = cosmic.player.target_node
            && let Some(descriptor) = cosmic.web.nodes.get(node as usize)
            && let Some(center) = world_to_pixels(
                main_vp,
                recenter(DVec3::from(descriptor.position_mpc), cosmic.upload_origin),
                vp,
            )
        {
            draw_target_ring(items, center, TARGET_RING_R, C_WARN);
        }
        // Vista intro hint (CVI-005/FR5): `press any key` fades in over
        // the second hold second (debug-shell text, bottom-center over
        // the viewport); gone once the dive starts or on skip. Alpha
        // from `vista_hint_alpha` (pinned in lib tests).
        {
            use game_debug::cosmic_vista::vista_hint_alpha;
            let vista = &cosmic.vista;
            let alpha = vista_hint_alpha(vista.phase, vista.t);
            if alpha > 0.0 {
                let label = "press any key";
                let w = label.len() as f32 * 8.0 + 24.0;
                items.solid(
                    Rect {
                        x: vp.x + (vp.w - w) * 0.5,
                        y: vp.y + vp.h - 52.0,
                        w,
                        h: 24.0,
                    },
                    [0.05, 0.06, 0.10, 0.80 * alpha],
                );
                items.text(
                    label.to_owned(),
                    vp.x + (vp.w - w) * 0.5 + 12.0,
                    vp.y + vp.h - 52.0 + 17.0,
                    [C_TEXT[0], C_TEXT[1], C_TEXT[2], alpha],
                );
            }
        }
    }
    // Transition fade + notice banner (UMAP-017): a fullscreen black
    // ramp over the fresh layer, then a banner pill top-center of
    // the viewport. Both ride the UI pass (alpha-blended).
    let fade = app.fx.fade_alpha();
    if fade > 0.0 {
        items.solid(
            Rect {
                x: 0.0,
                y: 0.0,
                w: win_w,
                h: win_h,
            },
            [0.0, 0.0, 0.0, fade],
        );
    }
    if let Some(text) = app.fx.notice_text() {
        let lh = atlas.line_height();
        let vp = layout.viewport;
        let pill_w = (text.len() as f32 * 8.0 + 24.0).min(vp.w).max(0.0);
        let pill = Rect {
            x: vp.x + (vp.w - pill_w) * 0.5,
            y: vp.y + 10.0,
            w: pill_w,
            h: lh + 10.0,
        };
        items.solid(pill, [0.05, 0.06, 0.10, 0.92]);
        items.text(text.to_owned(), pill.x + 12.0, pill.y + lh - 2.0, C_TEXT);
    }
    // Staged universe load (settings-seed-loader): modal determinate
    // progress over every tab. Composed into the dropdown buffer
    // (drawn after all other UI — topmost by command order): one
    // buffer draws solids before text, so composing here is the only
    // way panel solids sit above screen text (same isolation as the
    // dropdown). Full-window dim, viewport-centered panel with the
    // seed, bar + percent, and the live step label (text + bar, never
    // color-only). Input stays blocked while this is up (event gates);
    // there is no cancel in v1.
    if let Some(plan) = &app.loading {
        let total = LoadPlan::total();
        let done = plan.done().min(total);
        let step_label = if done == 0 {
            "Starting"
        } else {
            LoadStep::ALL[(done - 1).min(total - 1)].label()
        };
        let frac = plan.progress();
        let vp = layout.viewport;
        drop.solid(
            Rect {
                x: 0.0,
                y: 0.0,
                w: win_w,
                h: win_h,
            },
            [0.0, 0.0, 0.0, 0.55],
        );
        let panel_w = 340.0_f32.min(vp.w - 20.0).max(0.0);
        let panel_h = lh * 4.0 + 30.0;
        let panel = Rect {
            x: vp.x + (vp.w - panel_w) * 0.5,
            y: vp.y + (vp.h - panel_h) * 0.5,
            w: panel_w,
            h: panel_h,
        };
        drop.solid(panel, C_PANEL_BG);
        drop.text(
            format!("LOADING UNIVERSE · seed {}", plan.seed),
            panel.x + 12.0,
            panel.y + lh + 2.0,
            C_TEXT,
        );
        let track = Rect {
            x: panel.x + 12.0,
            y: panel.y + lh * 2.0 + 8.0,
            w: (panel.w - 24.0).max(0.0),
            h: 10.0,
        };
        drop.solid(track, C_FIELD_BG);
        drop.solid(
            Rect {
                x: track.x,
                y: track.y,
                w: track.w * frac,
                h: track.h,
            },
            C_BTN,
        );
        drop.text(
            format!("{}% · {step_label}", (frac * 100.0).round() as u32),
            panel.x + 12.0,
            panel.y + lh * 3.0 + 14.0,
            C_DIM,
        );
    }
}

/// Transition pill (S2): floating bottom-center overlay, drawn only
/// while a fly-to leg is in flight (v0.3.2 — the journey-transition
/// preview data is deleted; the pill shows live easing progress).
/// Overlays content — layout never shifts.
fn build_transition_strip(items: &mut UiItems, lh: f32, app: &DebugApp, win_w: f32, win_h: f32) {
    let exec = match &app.cosmic.player.exec {
        Some(exec) if mode_of(Some(exec)) == ShipMode::FlyTo => exec,
        _ => return,
    };
    let plan = exec.plan();
    let now = app.cosmic.player.clock.sim_time_s();
    let progress = ((now - plan.t_start_s) / plan.duration_s).clamp(0.0, 1.0);
    let label = app
        .cosmic
        .target_label()
        .unwrap_or_else(|| "node".to_owned());
    let pill = ui::transition_strip(win_w, win_h);
    items.solid(pill, C_PILL_BG);
    items.text(
        format!("▶ FLY-TO {label} · {:.0}%", progress * 100.0),
        pill.x + ui::TOP_PAD,
        pill.y + (pill.h + lh) / 2.0 - 3.0,
        C_WARN,
    );
}

/// Corner strip (parity anchor): toggle buttons living inside the
/// top bar, right-aligned — always visible because the bar is.
/// Three buttons: left dock, right dock, dev widget. The widget
/// button carries the live FPS value (S3: glanceable without opening
/// the widget). Pure builder sharing [`ui::corner_button`] with the
/// click router.
fn build_corner_strip_in_bar(items: &mut UiItems, lh: f32, app: &DebugApp, win_w: f32) {
    let strip = ui::corner_strip(win_w);
    let states = [
        ("DOCK-L [F9]".to_owned(), app.chrome.left_dock),
        ("DOCK-R [F10]".to_owned(), app.chrome.right_dock),
        (format!("DEV {:.0} [`]", app.fps.fps()), app.widget_visible),
    ];
    for (i, (label, on)) in states.iter().enumerate() {
        let rect = ui::corner_button(strip, i);
        items.solid(rect, if *on { C_BTN } else { C_BTN_OFF });
        items.text(
            label.clone(),
            rect.x + 8.0,
            rect.y + (rect.h + lh) / 2.0 - 3.0,
            C_TEXT,
        );
    }
}

/// Dev widget: fixed bottom-right overlay, header sub-tabs
/// (FPS/Console/Inspector) + body for the active sub-tab. Clicking
/// anywhere inside focuses the widget.
fn build_widget(items: &mut UiItems, lh: f32, app: &DebugApp, win_w: f32, win_h: f32) {
    if !app.widget_visible {
        return;
    }
    let widget = ui::widget_rect(win_w, win_h);
    items.solid(widget, C_PANEL_BG);
    for (i, tab) in WidgetTab::ALL.iter().enumerate() {
        let rect = ui::widget_tab_button(widget, i);
        if *tab == app.widget_tab {
            items.solid(rect, C_TAB_ACTIVE);
        }
        items.text(
            tab.title().to_owned(),
            rect.x + 8.0,
            rect.y + (rect.h + lh) / 2.0 - 3.0,
            C_TEXT,
        );
    }
    let body = widget_body_rect(widget);
    match app.widget_tab {
        WidgetTab::Fps => build_widget_fps(items, lh, app, body),
        WidgetTab::Console => build_widget_console(items, lh, app, body),
        WidgetTab::Inspector => build_widget_inspector(items, lh, app, body),
    }
}

/// Widget body rect from the widget rect (shared by the draw and the
/// fog-slider hit-test — one formula, two callers).
fn widget_body_rect(widget: Rect) -> Rect {
    Rect {
        x: widget.x + ui::DOCK_PAD,
        y: widget.y + ui::WIDGET_TAB_H + 4.0,
        w: (widget.w - 2.0 * ui::DOCK_PAD).max(0.0),
        h: (widget.h - ui::WIDGET_TAB_H - 12.0).max(0.0),
    }
}

/// Demo-fog slider track (`cosmic-depth-window` FR5, debug-only):
/// after the 4 inspector text rows + label row (all `lh + 4`), a
/// 20 px track. Shared by the widget draw and the mouse hit-test.
fn fog_slider_track(body: Rect, lh: f32) -> Rect {
    Rect {
        x: body.x,
        y: body.y + 5.0 * (lh + 4.0),
        w: body.w,
        h: 20.0,
    }
}

/// Widget FPS body: the frame-health tab, drawn into the widget
/// body rect (it already lays out into any area).
fn build_widget_fps(items: &mut UiItems, lh: f32, app: &DebugApp, body: Rect) {
    build_fps_tab(items, lh, &app.fps, body);
}

/// Widget Console body: the live fly-to event feed (newest last,
/// clipped to the body).
fn build_widget_console(items: &mut UiItems, lh: f32, app: &DebugApp, body: Rect) {
    let row_h = lh + 4.0;
    let capacity = (body.h / row_h.max(1.0)).floor().max(1.0) as usize;
    let lines = &app.console.lines()[app.console.len().saturating_sub(capacity)..];
    let mut y = body.y + body.h - lines.len() as f32 * row_h;
    for line in lines {
        text_row(
            items,
            lh,
            Rect {
                x: body.x,
                y,
                w: body.w,
                h: lh,
            },
            line.clone(),
            C_TEXT,
        );
        y += row_h;
    }
    if app.console.is_empty() {
        text_row(items, lh, body, "no events yet".to_owned(), C_DIM);
    }
}

/// Widget Inspector body: read-only journey summary (no simulation
/// state is touched).
fn build_widget_inspector(items: &mut UiItems, lh: f32, app: &DebugApp, body: Rect) {
    let mut rows = ui::PanelRows::new(body, 0.0);
    let flyto = match &app.cosmic.player.exec {
        Some(exec) if mode_of(Some(exec)) == ShipMode::FlyTo => {
            let plan = exec.plan();
            let now = app.cosmic.player.clock.sim_time_s();
            let progress = ((now - plan.t_start_s) / plan.duration_s).clamp(0.0, 1.0);
            format!(
                "fly-to:     {} · {:.0}%",
                app.cosmic
                    .target_label()
                    .unwrap_or_else(|| "node".to_owned()),
                progress * 100.0
            )
        }
        _ => "fly-to:     off".to_owned(),
    };
    for line in [
        format!("layer:      {:?}", app.journey.active_layer()),
        format!("waypoint:   {}", app.active_waypoint().name()),
        flyto,
        format!("console:     {} lines", app.console.len()),
    ] {
        text_row(items, lh, rows.next(lh, 4.0), line, C_TEXT);
    }
    // Demo-fog slider (`cosmic-depth-window` FR5, debug-only): the
    // track rect is shared with the mouse hit-test
    // (`fog_slider_track`).
    text_row(
        items,
        lh,
        rows.next(lh, 4.0),
        format!("demo fog:   {:.0} Mpc", app.cosmic.fog_l_mpc),
        C_TEXT,
    );
    let track = fog_slider_track(body, lh);
    items.solid(track, C_TRACK);
    let knob_value = ui::Slider::new(30, 400, app.cosmic.fog_l_mpc as u32);
    items.solid(
        Rect {
            x: knob_value.knob_x(track) - 5.0,
            y: track.y + 1.0,
            w: 10.0,
            h: track.h - 2.0,
        },
        C_KNOB,
    );
}

/// Widget FPS body: live numbers + a sparkline of the newest
/// [`FPS_SPARKLINE`] samples (right = newest, 0–50 ms full height).
fn build_fps_tab(items: &mut UiItems, lh: f32, fps: &FpsOverlay, area: Rect) {
    let mut rows = ui::PanelRows::new(area, ui::DOCK_PAD);
    section_bar(items, rows.next(lh + 6.0, 4.0), "FRAME HEALTH");
    for line in [
        format!("fps:       {:5.1}", fps.fps()),
        format!(
            "frame:      {:5.1} ms avg · {:5.1} ms max",
            fps.avg_ms(),
            fps.max_ms()
        ),
        format!("samples:    {}", fps.count()),
    ] {
        text_row(items, lh, rows.next(lh, 4.0), line, C_TEXT);
    }
    text_row(
        items,
        lh,
        rows.next(lh, 4.0),
        "last 120 frames (right = newest)".to_owned(),
        C_DIM,
    );
    let plot = rows.next(120.0, 4.0);
    let plot = Rect {
        x: plot.x,
        y: plot.y,
        w: plot.w,
        h: 120.0,
    };
    items.solid(plot, C_FIELD_BG);
    // Oldest left, newest right; fixed slots so the trace doesn't
    // rescale while samples accumulate. At least one slot so the trace
    // never vanishes on an empty window.
    let mut samples = fps.recent_ms(FPS_SPARKLINE);
    samples.reverse();
    let shown = samples
        .len()
        .clamp(1, FPS_SPARKLINE)
        .min(plot.w.max(1.0) as usize);
    let start = samples.len().saturating_sub(shown);
    let slot = plot.w / FPS_SPARKLINE as f32;
    for (i, ms) in samples[start..].iter().enumerate() {
        let h = (ms / 50.0).clamp(0.0, 1.0) * plot.h;
        let color = if *ms < 20.0 {
            C_FPS_OK
        } else if *ms < 34.0 {
            C_FPS_WARN
        } else {
            C_FPS_HITCH
        };
        items.solid(
            Rect {
                x: plot.x + (FPS_SPARKLINE - shown + i) as f32 * slot,
                y: plot.y + plot.h - h,
                w: slot.max(1.0),
                h,
            },
            color,
        );
    }
}

/// Convert frame UI into GPU vertices (solids as two triangles each).
fn ui_items_to_vertices(items: &UiItems, atlas: &mut GlyphAtlas) -> Vec<UiVertex> {
    let mut verts = Vec::new();
    for (rect, color) in &items.solids {
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
        let quad = |x: f32, y: f32| UiVertex {
            pos: [x, y],
            uv: [0.0, 0.0],
            color: *color,
        };
        verts.extend_from_slice(&[
            quad(x0, y0),
            quad(x1, y0),
            quad(x0, y1),
            quad(x1, y0),
            quad(x1, y1),
            quad(x0, y1),
        ]);
    }
    let mut quads = Vec::new();
    for (tri, color) in &items.tris {
        let vert = |p: (f32, f32)| UiVertex {
            pos: [p.0, p.1],
            uv: [0.0, 0.0],
            color: *color,
        };
        verts.extend_from_slice(&[vert(tri.0), vert(tri.1), vert(tri.2)]);
    }
    for run in &items.texts {
        let before = quads.len();
        atlas.push_text(&mut quads, &run.text, run.x, run.baseline);
        for quad in &quads[before..] {
            let (x0, y0, x1, y1) = (quad.pos[0], quad.pos[1], quad.pos[2], quad.pos[3]);
            let (u0, v0, u1, v1) = (quad.uv[0], quad.uv[1], quad.uv[2], quad.uv[3]);
            let vert = |x: f32, y: f32, u: f32, v: f32| UiVertex {
                pos: [x, y],
                uv: [u, v],
                color: run.color,
            };
            verts.extend_from_slice(&[
                vert(x0, y0, u0, v0),
                vert(x1, y0, u1, v0),
                vert(x0, y1, u0, v1),
                vert(x1, y0, u1, v0),
                vert(x1, y1, u1, v1),
                vert(x0, y1, u0, v1),
            ]);
        }
        quads.clear();
    }
    verts
}

// ---------------------------------------------------------------------------
// Vulkan helpers.
// ---------------------------------------------------------------------------

/// Compile one viewer GLSL source through `engine::render` (naga).
fn compile_shader(
    device: &Arc<Device>,
    kind: ShaderKind,
    source: &str,
    what: &str,
) -> Arc<ShaderModule> {
    let words = compile_glsl_to_spirv(kind, source)
        .unwrap_or_else(|error| panic!("{what} shader must compile: {error}"));
    // SAFETY: SPIR-V words come from `compile_glsl_to_spirv`, which runs
    // naga validation and pins the `main` entry point; each module is used
    // only with the matching vertex input layout below.
    unsafe { ShaderModule::new(device.clone(), ShaderModuleCreateInfo::new(&words)) }
        .unwrap_or_else(|error| panic!("{what} shader module must load: {error}"))
}

fn pipeline_layout_for(
    device: &Arc<Device>,
    vs: EntryPoint,
    fs: EntryPoint,
) -> (Arc<PipelineLayout>, [PipelineShaderStageCreateInfo; 2]) {
    let stages = [
        PipelineShaderStageCreateInfo::new(vs),
        PipelineShaderStageCreateInfo::new(fs),
    ];
    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone())
            .expect("pipeline layout must build"),
    )
    .expect("pipeline layout must create");
    (layout, stages)
}

/// Compiled viewer shader modules, shared by both windows
/// (device-level objects — only the `GraphicsPipeline`s are built per
/// render pass, once per window).
struct ShaderSet {
    fill_vert: Arc<ShaderModule>,
    fill_frag: Arc<ShaderModule>,
    line_vert: Arc<ShaderModule>,
    line_frag: Arc<ShaderModule>,
    ui_vert: Arc<ShaderModule>,
    ui_frag: Arc<ShaderModule>,
    map_vert: Arc<ShaderModule>,
    map_frag: Arc<ShaderModule>,
    glow_vert: Arc<ShaderModule>,
    glow_frag: Arc<ShaderModule>,
    splat_vert: Arc<ShaderModule>,
    splat_frag: Arc<ShaderModule>,
    splat_proc_vert: Arc<ShaderModule>,
    post_vert: Arc<ShaderModule>,
    prefilter_frag: Arc<ShaderModule>,
    down_frag: Arc<ShaderModule>,
    up_frag: Arc<ShaderModule>,
    march_frag: Arc<ShaderModule>,
    resolve_frag: Arc<ShaderModule>,
}

impl ShaderSet {
    fn compile(device: &Arc<Device>) -> Self {
        ShaderSet {
            fill_vert: compile_shader(device, ShaderKind::Vertex, FILL_VERT, "fill vertex"),
            fill_frag: compile_shader(device, ShaderKind::Fragment, FILL_FRAG, "fill fragment"),
            line_vert: compile_shader(device, ShaderKind::Vertex, LINE_VERT, "line vertex"),
            line_frag: compile_shader(device, ShaderKind::Fragment, LINE_FRAG, "line fragment"),
            ui_vert: compile_shader(device, ShaderKind::Vertex, UI_VERT, "ui vertex"),
            ui_frag: compile_shader(device, ShaderKind::Fragment, UI_FRAG, "ui fragment"),
            map_vert: compile_shader(device, ShaderKind::Vertex, MAP_VERT, "map vertex"),
            map_frag: compile_shader(device, ShaderKind::Fragment, MAP_FRAG, "map fragment"),
            glow_vert: compile_shader(device, ShaderKind::Vertex, GLOW_VERT, "glow vertex"),
            glow_frag: compile_shader(device, ShaderKind::Fragment, GLOW_FRAG, "glow fragment"),
            splat_vert: compile_shader(device, ShaderKind::Vertex, SPLAT_VERT, "splat vertex"),
            splat_frag: compile_shader(device, ShaderKind::Fragment, SPLAT_FRAG, "splat fragment"),
            splat_proc_vert: compile_shader(
                device,
                ShaderKind::Vertex,
                SPLAT_PROC_VERT,
                "procedural splat vertex",
            ),
            post_vert: compile_shader(device, ShaderKind::Vertex, RESOLVE_VERT, "post vertex"),
            prefilter_frag: compile_shader(
                device,
                ShaderKind::Fragment,
                BLOOM_PREFILTER_FRAG,
                "bloom prefilter fragment",
            ),
            down_frag: compile_shader(
                device,
                ShaderKind::Fragment,
                BLOOM_DOWN_FRAG,
                "bloom down fragment",
            ),
            up_frag: compile_shader(
                device,
                ShaderKind::Fragment,
                BLOOM_UP_FRAG,
                "bloom up fragment",
            ),
            march_frag: compile_shader(
                device,
                ShaderKind::Fragment,
                MARCH_FRAG,
                "veil march fragment",
            ),
            resolve_frag: compile_shader(
                device,
                ShaderKind::Fragment,
                &resolve_frag_bloom_march(),
                "bloom resolve fragment",
            ),
        }
    }
}

fn build_fill_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .fill_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .fill_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = FillVertex::per_vertex()
        .definition(&vs)
        .expect("fill vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::Back,
                // `OrbitCamera::projection_matrix` outputs
                // framebuffer-true NDC (NDC +1 = top row, no Y-flip),
                // so the mesh's CCW-outward fans
                // (`fill_faces_point_outward`) classify as CCW front
                // faces directly: the near side is kept, the far side
                // culled. (The retired Y-flipped projection mirrored
                // winding, which is why this used to be Clockwise —
                // issue-2026-09-14-2113.)
                front_face: FrontFace::CounterClockwise,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: true,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("fill graphics pipeline must create")
}

fn build_line_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .line_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .line_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = LineVertex::per_vertex()
        .definition(&vs)
        .expect("line vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::LineList,
                ..Default::default()
            }),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                // Coplanarity with the fill is handled by radial inflation
                // in the line vertex shader (LINE_INFLATE), not depth bias
                // — bias alone z-fought at grazing angles.
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(AttachmentBlend::alpha()),
                    ..Default::default()
                },
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: true,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("line graphics pipeline must create")
}

fn build_ui_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .ui_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .ui_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = UiVertex::per_vertex()
        .definition(&vs)
        .expect("ui vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(AttachmentBlend::alpha()),
                    ..Default::default()
                },
            )),
            // UI draws last with the depth test disabled (state present
            // because the subpass owns a depth attachment, test off so the
            // panel always sits on top).
            depth_stencil_state: Some(DepthStencilState::default()),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("ui graphics pipeline must create")
}

/// Galaxy-map point pipeline (universe-maps): `PointList`, alpha blend
/// for nebula impostors, no depth write (the map is one flat layer;
/// draw order decides overdraw, same rule as the flat fill).
fn build_map_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .map_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .map_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = MapVertex::per_vertex()
        .definition(&vs)
        .expect("map vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::PointList,
                ..Default::default()
            }),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(AttachmentBlend::alpha()),
                    ..Default::default()
                },
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: false,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("map graphics pipeline must create")
}

/// Cosmic glow point pipeline (update-2026-09-18-2328): `PointList`,
/// premultiplied-additive blend, no depth write (overlaps accumulate;
/// draw order decides nothing). Same vertex type as the map pipeline
/// — only the shaders, blend, and push block differ.
fn build_glow_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .glow_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .glow_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = MapVertex::per_vertex()
        .definition(&vs)
        .expect("glow vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::PointList,
                ..Default::default()
            }),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(additive_blend()),
                    ..Default::default()
                },
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: false,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("glow graphics pipeline must create")
}

/// Cosmic tracer-splat pipeline (`cosmic-tracer-splat`): `PointList`
/// over [`SplatVertex`] records, premultiplied-additive, no depth
/// write — a pipeline *variant* of the glow path (same pass, same
/// blend), not a new pass or draw.
fn build_splat_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .splat_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .splat_frag
        .entry_point("main")
        .expect("fragment entry point");
    let vertex_input_state = SplatVertex::per_vertex()
        .definition(&vs)
        .expect("splat vertex layout must match shader");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::PointList,
                ..Default::default()
            }),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(additive_blend()),
                    ..Default::default()
                },
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: false,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("splat graphics pipeline must create")
}

/// Procedural-tracer splat pipeline (`cosmic-gpu-tracers`, CGT-005):
/// `PointList` over `gl_VertexIndex` (no vertex input — the post-pipeline
/// precedent: explicit empty state, not `None`), same pass and
/// premultiplied-additive blend as the legacy splat path, not a new
/// pass or draw. Shares `SPLAT_FRAG` with the legacy pipeline.
fn build_splat_proc_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .splat_proc_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .splat_frag
        .entry_point("main")
        .expect("fragment entry point");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            // No vertex buffers: `gl_VertexIndex` drives the draw.
            vertex_input_state: Some(VertexInputState::default()),
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::PointList,
                ..Default::default()
            }),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState {
                    blend: Some(additive_blend()),
                    ..Default::default()
                },
            )),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    write_enable: false,
                    compare_op: CompareOp::Less,
                }),
                ..Default::default()
            }),
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("procedural splat graphics pipeline must create")
}

// ---------------------------------------------------------------------------
// HDR bloom post chain (update-2026-09-18-2328, cosmic views only).
// ---------------------------------------------------------------------------

/// HDR support probe: a candidate format must serve both as the scene
/// color attachment and as the downstream sampler source (the
/// `post::HDR_FORMAT_PREFERENCE` contract — mirrors
/// `game_tools::hdr_support`).
fn hdr_support(
    physical_device: &vulkano::device::physical::PhysicalDevice,
    format: Format,
) -> bool {
    physical_device
        .format_properties(format)
        .map(|props| {
            props
                .optimal_tiling_features
                .contains(FormatFeatures::COLOR_ATTACHMENT | FormatFeatures::SAMPLED_IMAGE)
        })
        .unwrap_or(false)
}

/// HDR scene pass: HDR color + depth, both cleared (the `game_tools`
/// scene-pass shape; the cosmic scene draws here in HDR mode).
fn build_scene_pass(device: &Arc<Device>, hdr_format: Format) -> Arc<RenderPass> {
    vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                format: hdr_format,
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
            depth: {
                format: DEPTH_FORMAT,
                samples: 1,
                load_op: Clear,
                store_op: DontCare,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {depth},
        },
    )
    .expect("HDR scene render pass must create")
}

/// Post pass: one HDR color attachment, cleared (shared by the bright
/// extract and every blur step — all are fullscreen overwrites).
fn build_post_pass(device: &Arc<Device>, hdr_format: Format) -> Arc<RenderPass> {
    vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                format: hdr_format,
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {},
        },
    )
    .expect("post render pass must create")
}

/// Fullscreen-triangle pipeline constructor (bright / blur / resolve):
/// empty vertex input (everything derives from `gl_VertexIndex`), no
/// depth test, no culling, no blending (every pass overwrites fully).
/// The `game_tools` `build_resolve_pipeline` shape, generalized over
/// the fragment module. `depth_state` must mirror the subpass:
/// `Some(default)` where the subpass owns a depth attachment (main
/// pass — VUID-06043), `None` where it does not (post pass).
fn build_post_pipeline(
    device: &Arc<Device>,
    frag: &Arc<ShaderModule>,
    vert: &Arc<ShaderModule>,
    render_pass: &Arc<RenderPass>,
    depth_state: Option<DepthStencilState>,
    what: &str,
) -> Arc<GraphicsPipeline> {
    let vs = vert.entry_point("main").expect("vertex entry point");
    let fs = frag.entry_point("main").expect("fragment entry point");
    let (layout, stages) = pipeline_layout_for(device, vs, fs);
    let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            // No vertex buffers (see the `game_tools` note: explicit
            // empty state, not `None`).
            vertex_input_state: Some(VertexInputState::default()),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(ViewportState::default()),
            rasterization_state: Some(RasterizationState {
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            depth_stencil_state: depth_state,
            dynamic_state: [DynamicState::Viewport].into_iter().collect(),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .unwrap_or_else(|error| panic!("{what} graphics pipeline must create: {error:?}"))
}

/// Transient HDR image view (scene + bloom targets): single-sampled,
/// no mipmaps, `COLOR_ATTACHMENT | SAMPLED` (the `game_tools`
/// `create_hdr_view` shape).
fn create_post_view(
    allocator: &Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
    format: Format,
    what: &str,
) -> Arc<ImageView> {
    let image = Image::new(
        allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format,
            extent: [extent[0], extent[1], 1],
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::SAMPLED,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| panic!("{what} image must create: {error}"));
    ImageView::new_default(image).unwrap_or_else(|error| panic!("{what} view must create: {error}"))
}

/// One sampled-image descriptor set (image + sampler at bindings 0–1 —
/// the `game_tools` `build_resolve_set` shape). The bloom-resolve set
/// (two images) is built by binding its second pair explicitly.
fn post_image_set(
    allocator: &Arc<StandardDescriptorSetAllocator>,
    pipeline: &Arc<GraphicsPipeline>,
    view: &Arc<ImageView>,
    sampler: &Arc<Sampler>,
    what: &str,
) -> Arc<DescriptorSet> {
    let layout = pipeline.layout().set_layouts()[0].clone();
    DescriptorSet::new(
        allocator.clone(),
        layout,
        [
            WriteDescriptorSet::image_view(0, view.clone()),
            WriteDescriptorSet::sampler(1, sampler.clone()),
        ],
        [],
    )
    .unwrap_or_else(|error| panic!("{what} descriptor set must create: {error}"))
}

/// Two-image descriptor set (coarse + fine at bindings 0–3 — the
/// upsample pair): written against the pipeline layout explicitly,
/// like the bloom-resolve pair below.
fn post_image_pair_set(
    allocator: &Arc<StandardDescriptorSetAllocator>,
    pipeline: &Arc<GraphicsPipeline>,
    coarse: &Arc<ImageView>,
    fine: &Arc<ImageView>,
    sampler: &Arc<Sampler>,
    what: &str,
) -> Arc<DescriptorSet> {
    let layout = pipeline.layout().set_layouts()[0].clone();
    DescriptorSet::new(
        allocator.clone(),
        layout,
        [
            WriteDescriptorSet::image_view(0, coarse.clone()),
            WriteDescriptorSet::sampler(1, sampler.clone()),
            WriteDescriptorSet::image_view(2, fine.clone()),
            WriteDescriptorSet::sampler(3, sampler.clone()),
        ],
        [],
    )
    .unwrap_or_else(|error| panic!("{what} descriptor set must create: {error}"))
}

/// Per-window HDR bloom resources (cosmic views only): scene target +
/// depth, mip-bloom pyramid targets, and the sampling sets
/// (`bloom-mip-chain`). Each bloom target is written exactly once per
/// frame and only read afterwards — never rewritten (an A/B ping-pong
/// reuse pattern corrupted its images on Intel UHD 620, diagnosed via
/// the `GAME_DEBUG_COSMIC_BLOOM=0` bisect). One frame executes at a
/// time behind `previous_frame_end`, so one set of transients is
/// enough. Rebuilt on swapchain recreate.
struct HdrChain {
    format: Format,
    // Views live on through the framebuffers + descriptor sets below;
    // only the framebuffers, sets, and extents are read per frame.
    scene_fb: Arc<Framebuffer>,
    /// Full scene extent (prefilter texel source).
    scene_extent: [u32; 2],
    /// Pyramid depth (3 Low / 4 Medium / 5 High).
    levels: u8,
    /// Down pyramid: `down[0]` at half res … `down[levels-1]`.
    down: Vec<BloomLevel>,
    /// Up pyramid, same extents, rebuilt coarse-to-fine.
    up: Vec<BloomLevel>,
    /// Prefilter set (samples the scene).
    prefilter_set: Arc<DescriptorSet>,
    /// `down_set[k]` samples `down[k]` (the `down[k]→down[k+1]` pass).
    down_set: Vec<Arc<DescriptorSet>>,
    /// `up_set[k]` samples the (`up[k+1]`, `down[k]`) pair (the
    /// `up[k]` pass, `k < levels - 1`).
    up_set: Vec<Arc<DescriptorSet>>,
    /// March target (quarter-res HDR, `cosmic-gas-veil-v2`): written
    /// once by the march pass, read only at the resolve. In sprites
    /// mode the pass is skipped and the cleared target adds ~0.
    march_fb: Arc<Framebuffer>,
    /// March target extent (quarter of the scene extent).
    march_extent: [u32; 2],
    /// March set (samples the 3D density volume).
    march_set: Arc<DescriptorSet>,
    /// Resolve set sampling scene + `up[0]` + march.
    resolve_set: Arc<DescriptorSet>,
    /// Resolve set sampling scene + `down[0]` + march: the
    /// `GAME_DEBUG_COSMIC_BLOOM=0` path, where the ups are never
    /// written.
    resolve_nobloom_set: Arc<DescriptorSet>,
}

/// One mip-bloom pyramid target: image + view + framebuffer + extent.
/// Every target is written exactly once per frame, then only read.
struct BloomLevel {
    image: Arc<Image>,
    view: Arc<ImageView>,
    fb: Arc<Framebuffer>,
    extent: [u32; 2],
}

fn upload_fill(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &PlanetViewerState,
) -> (Subbuffer<[FillVertex]>, Subbuffer<[u32]>) {
    let vertices = Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        viewer
            .fill_vertices
            .iter()
            .enumerate()
            .map(|(i, v)| FillVertex {
                position: v.position,
                normal: v.normal,
                tint: v.tint,
                uv: v.uv,
                dbg: [
                    viewer.fill_debug.seam.get(i).copied().unwrap_or(0.0),
                    viewer.fill_debug.island.get(i).copied().unwrap_or(0.0),
                ],
            }),
    )
    .expect("fill vertex buffer upload must succeed");
    let indices = Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::INDEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        viewer.fill_indices.iter().copied(),
    )
    .expect("fill index buffer upload must succeed");
    (vertices, indices)
}

fn upload_lines(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &PlanetViewerState,
) -> Subbuffer<[LineVertex]> {
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        viewer.lines.iter().map(|position| LineVertex {
            position: *position,
        }),
    )
    .expect("wireframe vertex buffer upload must succeed")
}

/// Upload the galaxy-map point buffer (universe-maps, universe-maps-3d):
/// L1 backdrop sprites, nebula impostors, then stars — upload order is
/// draw order for the alpha-blended point draw. Positions are world
/// `(east, up, −north)` (stars carry the real disk thickness; nebulae
/// are plane haze; backdrop is a seeded 3D volume). One upload per
/// regeneration; orbit/pan/zoom ride the view-projection push, never
/// the buffer.
fn upload_map(
    allocator: &Arc<StandardMemoryAllocator>,
    map: &GalaxyMapView,
) -> Subbuffer<[MapVertex]> {
    let backdrop = map.backdrop.iter().map(|sprite| MapVertex {
        map_pos: [sprite.x as f32, sprite.y as f32, -(sprite.z as f32)],
        color: [
            0.55 * sprite.brightness,
            0.60 * sprite.brightness,
            0.75 * sprite.brightness,
        ],
        misc: [1.5, 1.0, 0.0],
    });
    let nebulae = map.nebulae.iter().map(|sprite| MapVertex {
        map_pos: [sprite.x as f32, 0.0, -(sprite.z as f32)],
        color: sprite.tint,
        misc: [sprite.radius as f32, sprite.alpha, 1.0],
    });
    let stars = map.galaxy.stars.iter().map(|star| {
        let size =
            match star.spectral_class {
                game_engine::universe::SpectralClass::O
                | game_engine::universe::SpectralClass::B => 3.0,
                game_engine::universe::SpectralClass::A
                | game_engine::universe::SpectralClass::F => 2.5,
                _ => 2.0,
            };
        let world = star_world(star);
        MapVertex {
            map_pos: world.to_array(),
            color: spectral_color(star.spectral_class),
            misc: [size, 1.0, 0.0],
        }
    });
    // `Buffer::from_iter` needs an `ExactSizeIterator`: chained maps are
    // not, so materialize once per regeneration (not per frame).
    let verts: Vec<MapVertex> = backdrop.chain(nebulae).chain(stars).collect();
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("galaxy map vertex buffer upload must succeed")
}

/// Map shared glow points to vertices (one helper for the reseed
/// path and the rebase swap path — identical bytes by construction,
/// the `cosmic-rebase-async` FR7 pin).
fn upload_glow_points(
    allocator: &Arc<StandardMemoryAllocator>,
    points: &[game_debug::cosmic_rebase::GlowPoint],
) -> Subbuffer<[MapVertex]> {
    let mut verts: Vec<MapVertex> = points
        .iter()
        .map(|(pos, color, misc)| MapVertex {
            map_pos: *pos,
            color: *color,
            misc: *misc,
        })
        .collect();
    if verts.is_empty() {
        // Degenerate params guard (vulkano rejects zero-length vertex
        // buffers): one transparent point, mirroring upload_sky_points.
        verts.push(MapVertex {
            map_pos: [0.0, 0.0, -900.0],
            color: [0.0, 0.0, 0.0],
            misc: [1.0, 0.0, 0.0],
        });
    }
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("cosmic glow vertex buffer upload must succeed")
}

/// Map shared splat records to vertices (one helper for the reseed
/// path and the rebase swap path — identical bytes by construction,
/// the `cosmic-rebase-async` FR7 pin).
fn upload_splat_records(
    allocator: &Arc<StandardMemoryAllocator>,
    records: &[game_debug::cosmic_splat::SplatRecord],
) -> Subbuffer<[SplatVertex]> {
    use game_debug::cosmic_splat::splat_pack;
    let mut verts: Vec<SplatVertex> = records
        .iter()
        .map(|r| {
            let tint = r.class_tint >> 1;
            let b = r.class_tint & 1 == 1;
            SplatVertex {
                pos: r.pos,
                packed: splat_pack(r.overdensity.log2(), tint, b),
            }
        })
        .collect();
    if verts.is_empty() {
        verts.push(SplatVertex {
            pos: [0.0, 0.0, -900.0],
            packed: game_debug::cosmic_splat::splat_pack(-8.0, 0, false),
        });
    }
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("cosmic splat vertex buffer upload must succeed")
}

/// Upload the cosmic glow point buffer (update-2026-09-18-2328):
/// particulate grain, then descriptor dwarf glow, then node impostors
/// (core + halo), laid out by the shared [`cosmic_web`] helpers (one
/// palette for both 3D surfaces). Positions are origin-relative Mpc;
/// camera motion rides the MVP push, never this buffer. Rebuilt on
/// reseed and on rebase (the tick moves the upload origin back under
/// the ship past the rebase distance).
fn upload_cosmic_glow(
    allocator: &Arc<StandardMemoryAllocator>,
    web: &WebDescriptor,
    field: &WebField,
    seed: u64,
    origin: glam::DVec3,
    veil_mode: game_debug::cosmic_veil::VeilMode,
) -> Subbuffer<[MapVertex]> {
    // `cosmic-gas-veil-v2`: the glow buffer holds hub member galaxies
    // + tiered hub impostors, plus — in sprites mode only — the grid
    // cell-sprite veil (deterministic stride-2 subset, ~168k ≤ 200k
    // Low budget; one body, never both). The old descriptor-glow veil
    // is retired. Upload order is draw order for
    // the alpha-blended point draw — veil, then members, then
    // impostors so cores top the scatter. The CPU build lives in
    // `cosmic_rebase` (shared with the rebase worker).
    let points = game_debug::cosmic_rebase::build_demo_glow(web, field, seed, origin, veil_mode);
    upload_glow_points(allocator, &points)
}

/// Upload the cosmic tracer splats (`cosmic-tracer-splat`): one
/// 16 B [`SplatVertex`] per tracer at High (all tracers; Low/Medium
/// stride subsets are the tier constants in `cosmic_splat`, drawn by
/// device profiles — the windowed viewer has no tier switch). A
/// degenerate empty set uploads one guard point (vulkano rejects
/// zero-length vertex buffers).
fn upload_cosmic_splats(
    allocator: &Arc<StandardMemoryAllocator>,
    field: &WebField,
    web: &WebDescriptor,
    origin: glam::DVec3,
) -> Subbuffer<[SplatVertex]> {
    // The CPU build lives in `cosmic_rebase` (shared with the rebase
    // worker — bit-identity pinned there).
    let records = game_debug::cosmic_rebase::build_demo_splats(field, web, origin);
    upload_splat_records(allocator, &records)
}

/// Sub-samples per cell for this run (CGT-006): `GAME_DEBUG_COSMIC_K`
/// strict `1..=8`, else the High default 8 (the windowed binary has no
/// tier switch — windowed and capture both run full density unless
/// bisected, the splat-tier precedent).
fn splat_k_default() -> u8 {
    match std::env::var("GAME_DEBUG_COSMIC_K") {
        Ok(value) => match game_debug::cosmic_splat::SplatK::parse_override(&value) {
            Ok(k) => k.0,
            Err(error) => {
                eprintln!("bad GAME_DEBUG_COSMIC_K={value:?}: {error}; using 8");
                8
            }
        },
        Err(_) => 8,
    }
}

/// Upload the displacement volume (`cosmic-gpu-tracers` CGT-004):
/// RGBA16_SNORM 3D texture from the field displacement grid, one
/// staging copy + fence wait (the atlas-upload shape). Created on the
/// seed path only, never on rebase (tracers never rebuild per travel —
/// ADR-026 §2). Returns image + view; the sampler is the shared linear
/// post sampler. `None` when the field is degenerate or the device
/// lacks the format (R-3 fallback: the legacy splat path stays live).
fn upload_displacement_volume(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    device: &Arc<Device>,
    field: &WebField,
) -> Option<(Arc<Image>, Arc<ImageView>)> {
    use game_engine::universe::web::field_export::displacement_image_bytes;
    let bytes = displacement_image_bytes(field);
    let n = field.grid_cells;
    if bytes.is_empty() || n == 0 {
        return None;
    }
    let sampled = device
        .physical_device()
        .format_properties(Format::R16G16B16A16_SNORM)
        .map(|props| {
            props
                .optimal_tiling_features
                .contains(FormatFeatures::SAMPLED_IMAGE)
        })
        .unwrap_or(false);
    if !sampled {
        tracing::warn!("displacement RGBA16_SNORM not samplable: legacy splat path");
        return None;
    }
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R16G16B16A16_SNORM,
            extent: [n, n, n],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("displacement volume image must create");
    let staging = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        bytes,
    )
    .expect("displacement staging buffer must create");
    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("displacement command buffer builder must create");
    builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(staging, image.clone()))
        .expect("displacement copy must record");
    let command_buffer = builder
        .build()
        .expect("displacement command buffer must build");
    sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("displacement upload must submit")
        .then_signal_fence_and_flush()
        .expect("displacement fence must flush")
        .wait(None)
        .expect("displacement upload must complete");
    let view = ImageView::new_default(image.clone()).expect("displacement view must create");
    Some((image, view))
}

/// Upload the procedural-tracer cell list (`cosmic-gpu-tracers`
/// CGT-004): the Lagrangian cells whose displaced centre lands inside
/// the sphere, as a storage buffer for the vertex stage. Built once per
/// seed on the reseed / load path — never per travel. `None` when
/// empty (the legacy splat path covers the draw).
fn upload_cell_list(
    allocator: &Arc<StandardMemoryAllocator>,
    cells: &[u32],
) -> Option<Subbuffer<[u32]>> {
    if cells.is_empty() {
        return None;
    }
    Some(
        Buffer::from_iter(
            allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::STORAGE_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            cells.iter().copied(),
        )
        .expect("cell list buffer upload must succeed"),
    )
}

/// Procedural-splat descriptor set (CGT-004/005): displacement +
/// density volumes with the shared linear clamp sampler, the cell-list
/// SSBO, and the per-frame params UBO — written against the proc
/// pipeline's own layout, per frame (the atlas-set precedent: layouts
/// are per-window, resources per-seed, params per-frame).
#[allow(clippy::too_many_arguments)] // one set per frame; explicit resources are the pin
fn splat_proc_set(
    allocator: &Arc<StandardDescriptorSetAllocator>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    pipeline: &Arc<GraphicsPipeline>,
    disp_view: &Arc<ImageView>,
    density_view: &Arc<ImageView>,
    sampler: &Arc<Sampler>,
    cells: &Subbuffer<[u32]>,
    params: SplatProcParams,
) -> Arc<DescriptorSet> {
    let layout = pipeline.layout().set_layouts()[0].clone();
    let params_buf = Buffer::from_data(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::UNIFORM_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        params,
    )
    .expect("splat proc params upload must succeed");
    DescriptorSet::new(
        allocator.clone(),
        layout,
        [
            WriteDescriptorSet::image_view(0, disp_view.clone()),
            WriteDescriptorSet::sampler(1, sampler.clone()),
            WriteDescriptorSet::image_view(2, density_view.clone()),
            WriteDescriptorSet::sampler(3, sampler.clone()),
            WriteDescriptorSet::buffer(4, cells.clone()),
            WriteDescriptorSet::buffer(5, params_buf),
        ],
        [],
    )
    .expect("splat proc descriptor set must create")
}

/// Upload the inspector player point: one origin-relative vertex (near-
/// white, larger than any node sprite so it reads distinct). Rebuilt per
/// frame while the Cosmic Web tab shows — the ship moves continuously.
fn upload_cosmic_player_point(
    allocator: &Arc<StandardMemoryAllocator>,
    pos: [f32; 3],
) -> Subbuffer<[MapVertex]> {
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        vec![MapVertex {
            map_pos: pos,
            color: [0.90, 0.96, 1.00],
            misc: [6.0, 1.0, 0.0],
        }],
    )
    .expect("cosmic player point upload must succeed")
}

/// Upload the gas-veil density volume (`cosmic-gas-veil-v2` CGV-004):
/// R8 3D texture from the field grid, one staging copy + fence wait
/// (the atlas-upload shape). Returns image + view; the sampler is the
/// shared linear post sampler.
fn upload_veil_volume(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    device: &Arc<Device>,
    field: &WebField,
) -> Option<(Arc<Image>, Arc<ImageView>)> {
    use game_debug::cosmic_veil::veil_volume_bytes;
    let bytes = veil_volume_bytes(field);
    let n = field.grid_cells;
    if bytes.is_empty() || n == 0 {
        return None;
    }
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UNORM,
            extent: [n, n, n],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("veil volume image must create");
    let staging = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        bytes,
    )
    .expect("veil staging buffer must create");
    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("veil command buffer builder must create");
    builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(staging, image.clone()))
        .expect("veil copy must record");
    let command_buffer = builder.build().expect("veil command buffer must build");
    sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("veil upload must submit")
        .then_signal_fence_and_flush()
        .expect("veil fence must flush")
        .wait(None)
        .expect("veil upload must complete");
    let view = ImageView::new_default(image.clone()).expect("veil volume view must create");
    Some((image, view))
}

/// Veil body mode for this run: `GAME_DEBUG_COSMIC_VEIL` override, or
/// High-tier march (the debug binary has no tier switch — windowed
/// and capture both run the full march and stay pixel-identical, the
/// splat-tier precedent).
fn veil_mode() -> game_debug::cosmic_veil::VeilMode {
    use game_debug::cosmic_veil::VeilMode;
    match std::env::var("GAME_DEBUG_COSMIC_VEIL") {
        Ok(value) => VeilMode::parse_override(&value).unwrap_or_else(|error| {
            eprintln!("bad GAME_DEBUG_COSMIC_VEIL={value:?}: {error}; using march");
            VeilMode::for_tier_high()
        }),
        Err(_) => VeilMode::for_tier_high(),
    }
}

/// One splat draw inside an open scene/subpass (`cosmic-gpu-tracers`
/// CGT-005): the procedural path when the frame carries proc resources
/// (descriptor set built here against the proc pipeline's own layout),
/// else the legacy vertex-buffer path. Same push constants, same
/// blend — only the vertex source differs.
#[allow(clippy::too_many_arguments)] // one draw, two sources; explicit resources are the pin
fn record_splat_draw(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipeline: &Arc<GraphicsPipeline>,
    proc_pipeline: &Arc<GraphicsPipeline>,
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    sampler: &Arc<Sampler>,
    frame: &CosmicFrame,
    push: SplatPush,
) {
    if let Some(proc) = frame.proc_draw.as_ref() {
        let set = splat_proc_set(
            descriptor_set_allocator,
            memory_allocator,
            proc_pipeline,
            &proc.disp_view,
            &proc.density_view,
            sampler,
            &proc.cells,
            proc.params,
        );
        builder
            .bind_pipeline_graphics(proc_pipeline.clone())
            .expect("pipeline must bind")
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                proc_pipeline.layout().clone(),
                0,
                set,
            )
            .expect("splat proc set must bind")
            .push_constants(proc_pipeline.layout().clone(), 0, push)
            .expect("splat push constants must upload");
        // SAFETY: the proc draw has no vertex input — `count = cells ×
        // k` unbuffered invocations are the whole draw.
        unsafe { builder.draw(proc.count, 1, 0, 0) }.expect("proc splat draw must record");
    } else {
        builder
            .bind_pipeline_graphics(pipeline.clone())
            .expect("pipeline must bind")
            .bind_vertex_buffers(0, frame.splats.clone())
            .expect("vertex buffer must bind")
            .push_constants(pipeline.layout().clone(), 0, push)
            .expect("splat push constants must upload");
        // SAFETY: same PointList contract as the glow draw.
        unsafe { builder.draw(frame.splats.len() as u32, 1, 0, 0) }
            .expect("HDR scene splat draw must record");
    }
}

/// Shared cosmic HDR pre-pass recording (`cosmic-capture-harness`
/// CAP-001 seam): indigo scene (glow then splats) + mip-bloom pyramid
/// through the write-once targets. Called by the windowed
/// frame loop AND the offscreen `--capture` path — one recording
/// function, two targets (NFR1). Everything target-independent arrives
/// as params; the transients ride `hdr`, the draws ride `frame`.
#[allow(clippy::too_many_arguments)] // seam fn: explicit params are the no-drift guarantee
fn record_cosmic_hdr_prepass(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipes: &Pipelines,
    hdr: &HdrChain,
    frame: &CosmicFrame,
    glow_exposure: f32,
    redshift: f32,
    params: &MipBloomParams,
    bloom_enabled: bool,
    splat_alpha_k: f32,
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    sampler: &Arc<Sampler>,
) {
    // Scene: indigo clear, glow then splats (smoke retired CGV-009).
    builder
        .begin_render_pass(
            RenderPassBeginInfo {
                clear_values: vec![Some(COSMIC_BACKDROP.into()), Some(ClearValue::Depth(1.0))],
                ..RenderPassBeginInfo::framebuffer(hdr.scene_fb.clone())
            },
            SubpassBeginInfo {
                contents: SubpassContents::Inline,
                ..Default::default()
            },
        )
        .expect("HDR scene pass must begin")
        .set_viewport(0, [frame.viewport.clone()].into_iter().collect())
        .expect("viewport must set")
        .bind_pipeline_graphics(pipes.glow_scene.clone())
        .expect("pipeline must bind")
        .bind_vertex_buffers(0, frame.glow.clone())
        .expect("vertex buffer must bind")
        .push_constants(
            pipes.glow_scene.layout().clone(),
            0,
            GlowPush {
                mvp: frame.mvp,
                px_scale: frame.px_scale,
                exposure: glow_exposure,
                redshift,
                fog_l: frame.fog_l,
                slab_center: frame.slab_center,
                slab_half: frame.slab_half,
            },
        )
        .expect("glow push constants must upload");
    // SAFETY: same PointList contract as the galaxy map.
    unsafe { builder.draw(frame.glow.len() as u32, 1, 0, 0) }
        .expect("HDR scene glow draw must record");
    // Tracer splats (`cosmic-tracer-splat` + `cosmic-gpu-tracers`): the
    // field render rides the same scene pass through the splat
    // pipeline (legacy) or the proc pipeline (no vertex input).
    record_splat_draw(
        builder,
        &pipes.splat_scene,
        &pipes.splat_proc_scene,
        descriptor_set_allocator,
        memory_allocator,
        sampler,
        frame,
        SplatPush {
            mvp: frame.mvp,
            eye: [frame.eye[0], frame.eye[1], frame.eye[2], 0.0],
            px_scale: frame.px_scale,
            exposure: glow_exposure,
            redshift,
            h0: SPLAT_H0,
            alpha_k: splat_alpha_k,
            fog_l: frame.fog_l,
            slab_center: frame.slab_center,
            slab_half: frame.slab_half,
        },
    );
    builder
        .end_render_pass(Default::default())
        .expect("HDR scene pass must end");
    // Mip-bloom pyramid (`bloom-mip-chain`): prefilter + downs +
    // alias-free pass-through + ups, recorded from the same pass
    // description the write-once pin checks.
    record_bloom_chain(builder, pipes, hdr, params, bloom_enabled);
}

/// Fullscreen-triangle post-pass opener: black clear + viewport at
/// the target extent. Shared by every mip-bloom pyramid pass.
fn begin_post_pass(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    fb: Arc<Framebuffer>,
    extent: [u32; 2],
) -> &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer> {
    let black: ClearValue = [0.0, 0.0, 0.0, 1.0].into();
    builder
        .begin_render_pass(
            RenderPassBeginInfo {
                clear_values: vec![Some(black)],
                ..RenderPassBeginInfo::framebuffer(fb)
            },
            SubpassBeginInfo {
                contents: SubpassContents::Inline,
                ..Default::default()
            },
        )
        .expect("bloom pass must begin")
        .set_viewport(
            0,
            [Viewport {
                offset: [0.0, 0.0],
                extent: [extent[0] as f32, extent[1] as f32],
                depth_range: 0.0..=1.0,
            }]
            .into_iter()
            .collect(),
        )
        .expect("viewport must set")
}

/// Execute the mip-bloom pyramid (`bloom-mip-chain` BMC-003/004):
/// soft-knee prefilter (scene → `down[0]`) + downs + alias-free
/// pass-through (`down[last]` → `up[last]` exact image copy, never a
/// filtered blit) + tent ups. The passes follow the same
/// `describe_bloom_chain` description the write-once pin checks
/// (debug-asserted — zero cost in release). Every level image is
/// written once, then only read — never ping-ponged (Intel rule).
fn record_bloom_chain(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipes: &Pipelines,
    hdr: &HdrChain,
    params: &MipBloomParams,
    bloom_enabled: bool,
) {
    use game_debug::cosmic_bloom::{
        BloomPassKind, assert_write_once, bloom_img_down, bloom_img_up, describe_bloom_chain,
        describe_bloom_chain_bloom_off, describe_veil_chain,
    };
    debug_assert_eq!(
        params.levels, hdr.levels,
        "bloom params and chain must agree on levels"
    );
    let levels = hdr.levels;
    let descs = if bloom_enabled {
        describe_bloom_chain(levels)
    } else {
        describe_bloom_chain_bloom_off(levels)
    };
    debug_assert!(
        assert_write_once(&descs).is_ok(),
        "bloom description must be write-once"
    );
    // The veil march extends the same description (CGV-006); assert
    // the covered form too — zero cost in release.
    debug_assert!(
        assert_write_once(&describe_veil_chain(levels, true, bloom_enabled)).is_ok(),
        "veil chain must be write-once"
    );
    for desc in &descs {
        match desc.kind {
            BloomPassKind::Prefilter => {
                let target = &hdr.down[0];
                begin_post_pass(builder, target.fb.clone(), target.extent)
                    .bind_pipeline_graphics(pipes.prefilter.clone())
                    .expect("pipeline must bind")
                    .bind_descriptor_sets(
                        PipelineBindPoint::Graphics,
                        pipes.prefilter.layout().clone(),
                        0,
                        hdr.prefilter_set.clone(),
                    )
                    .expect("bloom prefilter set must bind")
                    .push_constants(
                        pipes.prefilter.layout().clone(),
                        0,
                        BloomPrefilterPush {
                            threshold: params.threshold,
                            knee: params.knee,
                            texel: [
                                1.0 / hdr.scene_extent[0] as f32,
                                1.0 / hdr.scene_extent[1] as f32,
                            ],
                        },
                    )
                    .expect("bloom prefilter push must upload");
                // SAFETY: fullscreen-triangle pipeline, no vertex
                // input — 3 unbuffered vertices are the whole draw.
                unsafe { builder.draw(3, 1, 0, 0) }.expect("bloom prefilter draw must record");
                builder
                    .end_render_pass(Default::default())
                    .expect("bloom prefilter pass must end");
            }
            BloomPassKind::Down => {
                let k = (desc.writes - bloom_img_down(0)) as usize;
                let src = &hdr.down[k - 1];
                let dst = &hdr.down[k];
                begin_post_pass(builder, dst.fb.clone(), dst.extent)
                    .bind_pipeline_graphics(pipes.down.clone())
                    .expect("pipeline must bind")
                    .bind_descriptor_sets(
                        PipelineBindPoint::Graphics,
                        pipes.down.layout().clone(),
                        0,
                        hdr.down_set[k - 1].clone(),
                    )
                    .expect("bloom down set must bind")
                    .push_constants(
                        pipes.down.layout().clone(),
                        0,
                        BloomDownPush {
                            texel: [1.0 / src.extent[0] as f32, 1.0 / src.extent[1] as f32],
                        },
                    )
                    .expect("bloom down push must upload");
                // SAFETY: fullscreen-triangle pipeline, no vertex input.
                unsafe { builder.draw(3, 1, 0, 0) }.expect("bloom down draw must record");
                builder
                    .end_render_pass(Default::default())
                    .expect("bloom down pass must end");
            }
            BloomPassKind::PassThrough => {
                // Alias-free copy: the smallest up level equals the
                // smallest down level via one exact image copy (a tiny
                // draw that keeps the "one writer per image" rule
                // simple — never an alias, never a blit).
                let last = usize::from(levels) - 1;
                builder
                    .copy_image(CopyImageInfo::images(
                        hdr.down[last].image.clone(),
                        hdr.up[last].image.clone(),
                    ))
                    .expect("bloom pass-through copy must record");
            }
            BloomPassKind::Up => {
                let k = (desc.writes - bloom_img_up(levels, 0)) as usize;
                let dst = &hdr.up[k];
                begin_post_pass(builder, dst.fb.clone(), dst.extent)
                    .bind_pipeline_graphics(pipes.up.clone())
                    .expect("pipeline must bind")
                    .bind_descriptor_sets(
                        PipelineBindPoint::Graphics,
                        pipes.up.layout().clone(),
                        0,
                        hdr.up_set[k].clone(),
                    )
                    .expect("bloom up set must bind")
                    .push_constants(
                        pipes.up.layout().clone(),
                        0,
                        BloomUpPush {
                            weight: params.level_weights[k + 1],
                        },
                    )
                    .expect("bloom up push must upload");
                // SAFETY: fullscreen-triangle pipeline, no vertex input.
                unsafe { builder.draw(3, 1, 0, 0) }.expect("bloom up draw must record");
                builder
                    .end_render_pass(Default::default())
                    .expect("bloom up pass must end");
            }
            BloomPassKind::Resolve => {
                // The composite resolve lives in the view arm (main
                // pass), never in the pyramid.
            }
            BloomPassKind::March => {
                // The veil march records separately
                // (`record_veil_march`, after the pyramid) — never
                // inside the bloom chain.
            }
        }
    }
}

/// Execute the gas-veil march (`cosmic-gas-veil-v2` CGV-005/006):
/// one fullscreen pass into the quarter-res march target (the
/// `March` row of the veil pass description — write-once pinned).
/// Call between the bloom pyramid and the view arm; skipped in
/// sprites mode (the cleared target then adds ~0 at the resolve).
fn record_veil_march(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipes: &Pipelines,
    hdr: &HdrChain,
    march: &MarchPush,
) {
    begin_post_pass(builder, hdr.march_fb.clone(), hdr.march_extent)
        .bind_pipeline_graphics(pipes.march.clone())
        .expect("pipeline must bind")
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipes.march.layout().clone(),
            0,
            hdr.march_set.clone(),
        )
        .expect("veil march set must bind")
        .push_constants(pipes.march.layout().clone(), 0, *march)
        .expect("veil march push must upload");
    // SAFETY: fullscreen-triangle pipeline, no vertex input.
    unsafe { builder.draw(3, 1, 0, 0) }.expect("veil march draw must record");
    builder
        .end_render_pass(Default::default())
        .expect("veil march pass must end");
}

/// Shared cosmic view-arm recording (CAP-001 seam, second half): the
/// `ViewContent::CosmicWeb` arm of the main pass — HDR mode resolves
/// the pre-recorded scene + bloom + march over the target, LDR bypass
/// draws glow + splats direct, then the inspector player point.
/// Called by the windowed frame loop (inside the swapchain pass,
/// among the other views + UI) AND the offscreen `--capture` path
/// (alone in its own pass) — one recording function, two targets.
/// `viewport` is the per-view rect; `full_extent` sizes the HDR
/// resolve triangle.
#[allow(clippy::too_many_arguments)]
fn record_cosmic_view_arm(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipes: &Pipelines,
    hdr: Option<&HdrChain>,
    bloom_enabled: bool,
    frame: &CosmicFrame,
    viewport: Viewport,
    full_extent: [f32; 2],
    glow_exposure: f32,
    resolve_exposure: f32,
    bloom_intensity: f32,
    redshift: f32,
    splat_alpha_k: f32,
    march_gain: f32,
    player_point: Subbuffer<[MapVertex]>,
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    sampler: &Arc<Sampler>,
) {
    if let Some(hdr) = hdr {
        let full_vp = Viewport {
            offset: [0.0, 0.0],
            extent: full_extent,
            depth_range: 0.0..=1.0,
        };
        // Full bloom chain: scene + final bloom (E).
        // `GAME_DEBUG_COSMIC_BLOOM=0`: scene + bright
        // extract (A) — E is never written there.
        let resolve_set = if bloom_enabled {
            hdr.resolve_set.clone()
        } else {
            hdr.resolve_nobloom_set.clone()
        };
        builder
            .set_viewport(0, [full_vp.clone()].into_iter().collect())
            .expect("viewport must set")
            .bind_pipeline_graphics(pipes.resolve.clone())
            .expect("pipeline must bind")
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                pipes.resolve.layout().clone(),
                0,
                resolve_set,
            )
            .expect("bloom resolve set must bind")
            .push_constants(
                pipes.resolve.layout().clone(),
                0,
                BloomMarchResolvePush {
                    exposure: resolve_exposure,
                    intensity: bloom_intensity,
                    march_gain,
                },
            )
            .expect("bloom resolve push must upload");
        // SAFETY: fullscreen-triangle pipeline, no vertex
        // input — 3 unbuffered vertices are the whole draw.
        unsafe { builder.draw(3, 1, 0, 0) }.expect("bloom resolve draw must record");
    } else {
        let glow_len = frame.glow.len();
        builder
            .set_viewport(0, [viewport].into_iter().collect())
            .expect("viewport must set")
            .bind_pipeline_graphics(pipes.map_glow.clone())
            .expect("pipeline must bind")
            .bind_vertex_buffers(0, frame.glow.clone())
            .expect("vertex buffer must bind")
            .push_constants(
                pipes.map_glow.layout().clone(),
                0,
                GlowPush {
                    mvp: frame.mvp,
                    px_scale: frame.px_scale,
                    exposure: glow_exposure,
                    redshift,
                    fog_l: frame.fog_l,
                    slab_center: frame.slab_center,
                    slab_half: frame.slab_half,
                },
            )
            .expect("glow push constants must upload");
        // SAFETY: same PointList contract as the galaxy map.
        unsafe { builder.draw(glow_len as u32, 1, 0, 0) }.expect("cosmic glow draw must record");
        // Tracer splats (`cosmic-tracer-splat` + `cosmic-gpu-tracers`):
        // same LDR bypass path through the splat pipeline (legacy) or
        // the proc pipeline (no vertex input).
        record_splat_draw(
            builder,
            &pipes.splat,
            &pipes.splat_proc,
            descriptor_set_allocator,
            memory_allocator,
            sampler,
            frame,
            SplatPush {
                mvp: frame.mvp,
                eye: [frame.eye[0], frame.eye[1], frame.eye[2], 0.0],
                px_scale: frame.px_scale,
                exposure: glow_exposure,
                redshift,
                h0: SPLAT_H0,
                alpha_k: splat_alpha_k,
                fog_l: frame.fog_l,
                slab_center: frame.slab_center,
                slab_half: frame.slab_half,
            },
        );
    }
    if !frame.is_demo {
        // Inspector player point: drawn last through the alpha map
        // pipeline — after the resolve in HDR mode, so
        // the marker stays legible over the glow. The
        // pipeline bind is explicit: the previously
        // bound pipeline here is `resolve` (HDR) or
        // `map_glow` (LDR), whose push layouts are
        // incompatible with `MapPush` (VUID-06425).
        builder
            .bind_pipeline_graphics(pipes.map.clone())
            .expect("pipeline must bind")
            .bind_vertex_buffers(0, player_point)
            .expect("vertex buffer must bind")
            .push_constants(
                pipes.map.layout().clone(),
                0,
                MapPush {
                    mvp: frame.mvp,
                    px_scale: frame.px_scale,
                    exposure: 1.0,
                },
            )
            .expect("map push constants must upload");
        // SAFETY: single-vertex PointList, no index buffer.
        unsafe { builder.draw(1, 1, 0, 0) }.expect("cosmic player point draw must record");
    }
}

/// Upload catalog-sky points as Backdrop sprites (`misc` = pixel size,
/// alpha, kind 0). Re-uploaded only when the tile set changes —
/// camera motion rides the MVP push, never this buffer. Empty sets
/// upload one transparent guard point (vulkano rejects zero-length
/// vertex buffers).
fn upload_sky_points(
    allocator: &Arc<StandardMemoryAllocator>,
    points: &[game_engine::render::stars::StarPoint],
) -> Subbuffer<[MapVertex]> {
    let mut verts: Vec<MapVertex> = points
        .iter()
        .map(|point| MapVertex {
            map_pos: point.pos,
            color: point.color,
            misc: [point.size_px, point.alpha, 0.0],
        })
        .collect();
    if verts.is_empty() {
        verts.push(MapVertex {
            map_pos: [0.0, 0.0, -900.0],
            color: [0.0, 0.0, 0.0],
            misc: [1.0, 0.0, 0.0],
        });
    }
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("sky vertex buffer upload must succeed")
}

/// Upload the system-map point buffer (universe-maps, universe-maps-3d):
/// the central star then the planets at their tilted world slots,
/// tinted by spectral class / atmosphere color straight off the
/// descriptors. One upload per system load; focus and camera ride
/// push constants.
fn upload_system_points(
    allocator: &Arc<StandardMemoryAllocator>,
    system: &SystemMapView,
) -> Subbuffer<[MapVertex]> {
    let star = std::iter::once(MapVertex {
        map_pos: [0.0, 0.0, 0.0],
        color: spectral_color(system.system.star.spectral_class),
        misc: [9.0, 1.0, 0.0],
    });
    let planets = system.system.planets.iter().enumerate().map(|(i, planet)| {
        let (px, py, pz) = planet_slot(planet.orbit_radius_au, i as u32);
        MapVertex {
            map_pos: [px as f32, py as f32, pz as f32],
            color: planet.descriptor.atmosphere.color,
            misc: [3.0 + planet.descriptor.radius_km * 0.4, 1.0, 0.0],
        }
    });
    let verts: Vec<MapVertex> = star.chain(planets).collect();
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("system map vertex buffer upload must succeed")
}

/// Upload the system-map orbit rings as line segments
/// (universe-maps, universe-maps-3d): each ring tessellates into
/// segment pairs for the `LineList` pipeline, in tilted world space.
fn upload_system_lines(
    allocator: &Arc<StandardMemoryAllocator>,
    system: &SystemMapView,
) -> Subbuffer<[LineVertex]> {
    let mut verts = Vec::new();
    for (i, planet) in system.system.planets.iter().enumerate() {
        let ring = orbit_ring_points(planet.orbit_radius_au, i as u32);
        for pair in ring.windows(2) {
            verts.push(LineVertex {
                position: [pair[0].0, pair[0].1, pair[0].2],
            });
            verts.push(LineVertex {
                position: [pair[1].0, pair[1].1, pair[1].2],
            });
        }
    }
    Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        verts,
    )
    .expect("system map wireframe buffer upload must succeed")
}

fn create_depth_view(allocator: &Arc<StandardMemoryAllocator>, extent: [u32; 2]) -> Arc<ImageView> {
    let image = Image::new(
        allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: DEPTH_FORMAT,
            extent: [extent[0], extent[1], 1],
            usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("depth image must create");
    ImageView::new_default(image).expect("depth image view must create")
}

fn create_atlas_image(allocator: &Arc<StandardMemoryAllocator>, extent: u32) -> Arc<Image> {
    Image::new(
        allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8_UNORM,
            extent: [extent, extent, 1],
            usage: ImageUsage::TRANSFER_DST | ImageUsage::SAMPLED,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("atlas image must create")
}

/// Blocking full-texture upload of the CPU atlas (staging buffer +
/// one-shot command buffer, fenced). Runs outside any render pass, only
/// when [`GlyphAtlas::version`] changed.
fn upload_atlas(
    device: &Arc<Device>,
    queue: &Arc<Queue>,
    allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    image: &Arc<Image>,
    texture: &[u8],
) {
    let staging = Buffer::from_iter(
        allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        texture.iter().copied(),
    )
    .expect("atlas staging buffer must create");
    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("atlas command buffer builder must create");
    builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(staging, image.clone()))
        .expect("atlas copy must record");
    let command_buffer = builder.build().expect("atlas command buffer must build");
    sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("atlas upload must submit")
        .then_signal_fence_and_flush()
        .expect("atlas fence must flush")
        .wait(None)
        .expect("atlas upload must complete");
}

// ---------------------------------------------------------------------------
// App.
// ---------------------------------------------------------------------------

struct ViewerApp {
    debug: DebugApp,
    camera: OrbitCamera,
    atlas: GlyphAtlas,
    uploaded_atlas_version: Option<u64>,
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    sampler: Arc<Sampler>,
    post_sampler: Arc<Sampler>,
    shaders: ShaderSet,
    fill_vertices: Subbuffer<[FillVertex]>,
    fill_indices: Subbuffer<[u32]>,
    line_vertices: Subbuffer<[LineVertex]>,
    map_vertices: Subbuffer<[MapVertex]>,
    system_points: Subbuffer<[MapVertex]>,
    system_lines: Subbuffer<[LineVertex]>,
    /// Cosmic glow point buffer (veil sprites in sprites mode, hub
    /// members + impostors always; `cosmic-gas-veil-v2`): uploaded
    /// relative to the demo upload origin, rebuilt on reseed + rebase.
    cosmic_glow: Subbuffer<[MapVertex]>,
    /// Cosmic tracer splats (16 B vertices, same origin frame;
    /// `cosmic-tracer-splat`).
    cosmic_splats: Subbuffer<[SplatVertex]>,
    /// Rebase worker (`cosmic-rebase-async`, ADR-026 §3): off-thread
    /// demo-buffer rebuilds over `Arc` clones of the seed's immutable
    /// inputs. Recreated on reseed; `None` only when the spawn fails
    /// (the tick then falls back to the synchronous demo rebuild).
    rebase_worker: Option<game_debug::cosmic_rebase::RebaseWorker>,
    /// Rebase generation counter: each request takes the next value;
    /// only the matching result swaps (stale generations drop).
    rebase_generation: u64,
    /// Outstanding rebase request, if any (at most one in flight — a
    /// newer crossing waits for the swap, then re-requests).
    rebase_pending: Option<game_debug::cosmic_rebase::RebasePending>,
    /// Frame counter for rebase telemetry (`swapped after F frames`).
    tick_count: u64,
    /// Inspector glow buffer (fixed web-center origin, rebuilt on
    /// reseed only — the tab never rebases).
    cosmic_tab_glow: Subbuffer<[MapVertex]>,
    /// Inspector tracer splats (fixed web-center origin).
    cosmic_tab_splats: Subbuffer<[SplatVertex]>,
    /// Gas-veil density volume (`cosmic-gas-veil-v2`): R8 3D texture
    /// uploaded once per seed from the field grid + view. Rebuilt on
    /// reseed (never on rebase — rebase rides the march push origin).
    /// `None` until the first upload (LDR bypass never needs it).
    veil_volume: Option<(Arc<Image>, Arc<ImageView>)>,
    /// Procedural-tracer displacement volume (`cosmic-gpu-tracers`
    /// CGT-004): SNORM 3D texture from the field displacement grid +
    /// view. Per seed, never on rebase. `None` when the format is
    /// unsupported (the legacy splat path stays live — R-3 fallback).
    disp_volume: Option<(Arc<Image>, Arc<ImageView>)>,
    /// Procedural-tracer cell list (`cosmic-gpu-tracers` CGT-004):
    /// storage buffer over the Lagrangian cells, per seed, never on
    /// rebase. `None` when empty (the legacy splat path covers it).
    cell_list: Option<Subbuffer<[u32]>>,
    /// Cell count behind `cell_list` (the proc draw is `cells × k`).
    cell_count: usize,
    /// Sub-samples per cell for this run (CGT-006): the
    /// `GAME_DEBUG_COSMIC_K` override or the High default 8.
    splat_k: u8,
    /// Veil body mode (sprites on Low-tier override, march default):
    /// read once at boot from `GAME_DEBUG_COSMIC_VEIL`.
    veil_mode: game_debug::cosmic_veil::VeilMode,
    /// Inspector player point (one vertex, rebuilt per frame while the
    /// tab shows — the ship moves continuously).
    cosmic_tab_player: Subbuffer<[MapVertex]>,
    /// Catalog sky runtime (scheduler + loader + cache + fallback) and
    /// its Backdrop point buffer, drawn first in the Planet View.
    sky: CatalogSky,
    sky_vertices: Subbuffer<[MapVertex]>,
    /// Twilight demo stage (F5 cycles Day → Civil → Nautical →
    /// Astronomical): keys the sky-luminance input of the star
    /// fade-in (exposure-tone-mapping DoD-2 captures).
    twilight_stage: u8,
    /// Fog-slider drag in the widget Inspector tab
    /// (`cosmic-depth-window` FR5, debug-only).
    dragging_fog: bool,
    /// Windowed screenshot request (`F12`, `cosmic-capture-harness`):
    /// the next frame copies its swapchain image to a host buffer and
    /// writes `captures/<surface>-<seed>-<ts>.png`. Exploration only —
    /// DoD evidence always comes from `--capture` presets.
    pending_capture: bool,
    /// Partial sim-step accumulator for the transit countdown
    /// (fixed-step consumption of the frame dt).
    transit_acc: f32,
    atlas_image: Option<Arc<Image>>,
    dragging_orbit: bool,
    /// Right/middle-button viewport drag on the map screens: pans the
    /// 3D map camera (universe-maps-3d; left-drag orbits there).
    dragging_pan: bool,
    dragging_slider: bool,
    dragging_density: bool,
    /// Last frame time: the player movement `dt` source (clamped).
    last_frame: Option<Instant>,
    /// Last streaming sync: throttles the player-hemisphere reloads
    /// that feed the chunk streamer while player mode is active.
    last_stream_sync: Option<Instant>,
    /// Cached streaming desired set (the player's own hemisphere,
    /// refreshed on [`ViewerApp::last_stream_sync`]).
    stream_desired: Vec<u32>,
    /// Cursor position at left-button press (planet-viewport presses
    /// only): release within [`CLICK_MAX_DRAG_PX`] of it counts as a click
    /// (chunk pin) rather than an orbit drag.
    press_cursor: Option<(f32, f32)>,
    /// Last FPS sample: the frame-rate clock (once per event-loop
    /// iteration on the single window).
    last_fps_tick: Option<Instant>,
    main: Option<WindowContext>,
    /// Mouse-held walk direction from a Settings Controls row
    /// (hold-to-press parity for WASD): cleared on mouse release.
    mouse_walk: Option<WalkDir>,
    /// Shift modifier state (tracked via `ModifiersChanged`): `Shift`+wheel
    /// adjusts the cosmic cruise pace instead of zooming the camera.
    shift_held: bool,
}

/// Hold-to-press walk direction (mouse parity for the WASD keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkDir {
    North,
    South,
    West,
    East,
}

/// Per-window GPU state: surface/swapchain/framebuffers/depth +
/// pipelines + atlas descriptor set + cursor. The device, allocators,
/// mesh buffers and atlas image live on [`ViewerApp`] and are shared.
struct WindowContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    /// Swapchain images (F12 readback indexes the acquired one).
    swapchain_images: Vec<Arc<Image>>,
    render_pass: Arc<RenderPass>,
    pipelines: Pipelines,
    framebuffers: Vec<Arc<Framebuffer>>,
    depth_view: Arc<ImageView>,
    atlas_set: Option<Arc<DescriptorSet>>,
    atlas_set_extent: u32,
    last_cursor: Option<(f32, f32)>,
    recreate_swapchain: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
    /// Selected HDR scene format (`None` = LDR bypass).
    hdr_format: Option<Format>,
    /// False when `GAME_DEBUG_COSMIC_BLOOM=0` (bright extract only,
    /// blur passes skipped).
    bloom_enabled: bool,
    /// HDR scene pass (HDR color + depth, cosmic views only).
    scene_pass: Arc<RenderPass>,
    /// Shared post pass (bright extract + blur steps).
    post_pass: Arc<RenderPass>,
    /// Transient HDR bloom resources (`None` in LDR bypass).
    hdr: Option<HdrChain>,
}

impl WindowContext {
    fn size(&self) -> (f32, f32) {
        let size = self.window.inner_size();
        (size.width as f32, size.height as f32)
    }
}

impl ViewerApp {
    fn new(event_loop: &EventLoop<()>, seed: Option<u64>) -> Self {
        let library = VulkanLibrary::new().expect("Vulkan loader must be present");
        let required_extensions = Surface::required_extensions(event_loop)
            .expect("surface extensions must query cleanly");
        let instance = create_instance(library, required_extensions, cfg!(debug_assertions))
            .expect("Vulkan 1.1 instance must create");

        let device_extensions = required_device_extensions();
        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .expect("physical device enumeration must succeed")
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.presentation_support(i as u32, event_loop)
                                .unwrap_or(false)
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| device_score(p.properties().device_type))
            .expect("no suitable Vulkan 1.1 physical device found");

        log_physical_device(&physical_device);

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: device_extensions,
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .expect("logical device must create");
        let queue = queues.next().expect("one queue requested");

        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        ));
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
            device.clone(),
            Default::default(),
        ));
        let sampler = Sampler::new(
            device.clone(),
            SamplerCreateInfo {
                mag_filter: Filter::Linear,
                min_filter: Filter::Linear,
                ..Default::default()
            },
        )
        .expect("atlas sampler must create");
        // Post-chain sampler (update-2026-09-18-2328): linear for the
        // bright downsample, clamp-to-edge so blur taps never wrap.
        let post_sampler = Sampler::new(
            device.clone(),
            SamplerCreateInfo {
                mag_filter: Filter::Linear,
                min_filter: Filter::Linear,
                address_mode: [SamplerAddressMode::ClampToEdge; 3],
                ..Default::default()
            },
        )
        .expect("post sampler must create");

        // The windowed viewer opens at N=4: at the N=6 headless default
        // cells are subpixel (faces and pentagon sites unreadable), which
        // defeats the inspection goal of the default view. The `--headless`
        // path stays N=6 to cross-check the committed engine mesh hash
        // (update-2026-09-14-2008).
        let mut debug = DebugApp::with_viewer(PlanetViewerState::with_values(
            WINDOWED_SUBDIV,
            WINDOWED_RADIUS,
        ));
        // `--seed N` opens the viewer on universe N: the staged
        // loader runs on the first windowed frames (same machine as
        // the Settings Load button) and the viewer stays on the
        // default Game Demo tab, which shows the newly seeded cosmic
        // web. The sky keeps the boot seed either way.
        if let Some(seed) = seed {
            debug.loading = Some(LoadPlan::new(seed, LoadSource::Boot));
        }
        let viewer = &debug.viewer;
        tracing::info!(
            subdivisions = viewer.subdiv,
            radius = viewer.radius,
            cells = viewer.stats.cells,
            hash8 = viewer.stats.hash8.as_str(),
            "planet view mesh generated",
        );
        let (fill_vertices, fill_indices) = upload_fill(&memory_allocator, viewer);
        let line_vertices = upload_lines(&memory_allocator, viewer);
        let map_vertices = upload_map(&memory_allocator, &debug.galaxy);
        let system_points = upload_system_points(&memory_allocator, &debug.system);
        let system_lines = upload_system_lines(&memory_allocator, &debug.system);
        // Cosmic web for the demo tab (same master seed as the maps).
        let cosmic_origin = debug.cosmic.upload_origin;
        let cosmic_seed = debug.cosmic.seed;
        let veil_mode = veil_mode();
        let cosmic_glow = upload_cosmic_glow(
            &memory_allocator,
            &debug.cosmic.web,
            &debug.cosmic.field,
            cosmic_seed,
            cosmic_origin,
            veil_mode,
        );
        let cosmic_splats = upload_cosmic_splats(
            &memory_allocator,
            &debug.cosmic.field,
            &debug.cosmic.web,
            cosmic_origin,
        );
        // Inspector buffers at the fixed web-center origin (WS5).
        let cosmic_tab_glow = upload_cosmic_glow(
            &memory_allocator,
            &debug.cosmic.web,
            &debug.cosmic.field,
            cosmic_seed,
            glam::DVec3::ZERO,
            veil_mode,
        );
        let cosmic_tab_splats = upload_cosmic_splats(
            &memory_allocator,
            &debug.cosmic.field,
            &debug.cosmic.web,
            glam::DVec3::ZERO,
        );
        let cosmic_tab_player = upload_cosmic_player_point(&memory_allocator, [0.0, 0.0, 0.0]);
        // Gas-veil density volume (per seed; the march samples it).
        let veil_volume = upload_veil_volume(
            &memory_allocator,
            &command_buffer_allocator,
            &queue,
            &device,
            &debug.cosmic.field,
        );
        // Procedural-tracer resources (per seed, never on rebase):
        // displacement volume + cell-list storage buffer (CGT-004).
        let disp_volume = upload_displacement_volume(
            &memory_allocator,
            &command_buffer_allocator,
            &queue,
            &device,
            &debug.cosmic.field,
        );
        let cells = game_engine::universe::web::cell_list(&debug.cosmic.field);
        let cell_count = cells.len();
        let cell_list = upload_cell_list(&memory_allocator, &cells);
        let splat_k = splat_k_default();
        // manifest ⇒ procedural fallback sky (model-only, logged). The
        // sky shares the universe seed so fallback content is stable
        // per seed.
        let sky_seed = seed.unwrap_or(DEFAULT_GALAXY_SEED);
        let sky = CatalogSky::open("assets/catalog".into(), SKY_ORDER, sky_seed)
            .expect("catalog sky must open (order is valid)");
        let sky_vertices = upload_sky_points(&memory_allocator, &[]);
        // Shader modules compile once here; each window builds its own
        // pipelines from them (see `build_pipelines`).
        let shaders = ShaderSet::compile(&device);
        // Build tag (update-2026-09-18-2328 round 2): proves which
        // visual code is actually running — a stale `game_debug.exe`
        // (e.g. relink blocked by a still-running viewer holding the
        // exe lock) silently keeps the old look. If this line is
        // missing from the log, the binary predates the fix.
        tracing::info!(
            "cosmic visual build r4: illustris-look (stretched sheath quads + frayed strands + gold beads over bifurcation/spine skeleton) + rim-zero aniso falloff + bounded tint + world halos + resolve clamp"
        );
        // Rebase worker over the boot seed's inputs (off-thread demo
        // rebuilds from the first crossing; reseed recreates it).
        let rebase_worker = match game_debug::cosmic_rebase::RebaseWorker::try_spawn(
            Arc::new(debug.cosmic.web.clone()),
            Arc::new(debug.cosmic.field.clone()),
            cosmic_seed,
            veil_mode,
        ) {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(error = %error, "cosmic rebase worker unavailable: synchronous fallback");
                None
            }
        };
        ViewerApp {
            camera: OrbitCamera::framing_planet(viewer.radius),
            debug,
            atlas: GlyphAtlas::new(UI_PX),
            uploaded_atlas_version: None,
            instance,
            device,
            queue,
            memory_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
            sampler,
            post_sampler,
            shaders,
            fill_vertices,
            fill_indices,
            line_vertices,
            map_vertices,
            system_points,
            system_lines,
            cosmic_glow,
            cosmic_splats,
            rebase_worker,
            rebase_generation: 0,
            rebase_pending: None,
            tick_count: 0,
            cosmic_tab_glow,
            cosmic_tab_splats,
            cosmic_tab_player,
            veil_volume,
            disp_volume,
            cell_list,
            cell_count,
            splat_k,
            veil_mode,
            sky,
            sky_vertices,
            twilight_stage: 0,
            dragging_fog: false,
            pending_capture: false,
            transit_acc: 0.0,
            atlas_image: None,
            dragging_orbit: false,
            dragging_pan: false,
            dragging_slider: false,
            dragging_density: false,
            last_frame: None,
            last_stream_sync: None,
            stream_desired: Vec::new(),
            press_cursor: None,
            last_fps_tick: None,
            main: None,
            mouse_walk: None,
            shift_held: false,
        }
    }

    /// Cosmic demo tick: cruise from held input intent, track the
    /// camera, and rebase the buffers past 50 Mpc of travel — without
    /// ever stalling the frame (`cosmic-rebase-async`): a crossing only
    /// *requests* an off-thread rebuild (once per generation); every
    /// frame polls for the result and swaps it in. Parked tabs clear
    /// stale thrust (keys never stick across switches). The dt floor
    /// keeps the flight assert (`dt > 0`) green on the very first frame.
    fn tick_cosmic(&mut self, dt: f32) {
        if !matches!(self.debug.screen, Screen::GameDemo) {
            self.debug.cosmic.held.clear();
            return;
        }
        self.tick_count += 1;
        if self.debug.cosmic.tick(dt.max(1e-6) as f64) && self.rebase_pending.is_none() {
            // The ship outran the upload origin: queue a rebuild of the
            // demo buffers only. The old buffers stay live with the old
            // origin until the swap — no snap (FR4).
            let origin = self.debug.cosmic.player.position_mpc();
            self.rebase_generation += 1;
            let generation = self.rebase_generation;
            match self.rebase_worker.as_ref() {
                Some(worker) => {
                    worker.request(origin, generation);
                    self.rebase_pending = Some(game_debug::cosmic_rebase::RebasePending {
                        generation,
                        origin,
                        request_frame: self.tick_count,
                    });
                    self.debug
                        .console
                        .push(format!("rebase: queued gen {generation}"));
                }
                None => {
                    // Spawn-failure fallback (never on a healthy
                    // machine): synchronous demo-only rebuild.
                    self.rebuild_demo_buffers(origin);
                    self.debug.cosmic.rebased();
                    self.debug.console.push(format!(
                        "rebase: swapped gen {generation} after 0 frames (sync fallback)"
                    ));
                }
            }
        }
        // Per-frame poll: upload + swap BOTH buffers in the same frame,
        // then move the origin with them (never one without the other —
        // A-2). Stale generations drop here silently.
        if let Some(pending) = self.rebase_pending {
            let result = self.rebase_worker.as_ref().and_then(|worker| worker.poll());
            if let Some(result) = result
                && result.generation == pending.generation
            {
                self.cosmic_glow = upload_glow_points(&self.memory_allocator, &result.glow);
                self.cosmic_splats = upload_splat_records(&self.memory_allocator, &result.splats);
                self.debug.cosmic.rebased_to(result.origin);
                let frames = self.tick_count - pending.request_frame;
                self.debug.console.push(format!(
                    "rebase: swapped gen {} after {frames} frames",
                    result.generation
                ));
                self.rebase_pending = None;
            }
        }
        // Vista intro (CVI-005/FR5): drain transition events into the
        // Console (`vista: hold/dive/skipped/done`).
        for event in self.debug.cosmic.vista_events.drain(..) {
            self.debug.console.push(format!("vista: {event}"));
        }
    }

    /// Rebuild GPU mesh buffers + reframe the camera after Regenerate.
    fn refresh_mesh(&mut self) {
        let viewer = &self.debug.viewer;
        (self.fill_vertices, self.fill_indices) = upload_fill(&self.memory_allocator, viewer);
        self.line_vertices = upload_lines(&self.memory_allocator, viewer);
        self.camera = OrbitCamera::framing_planet(viewer.radius);
        tracing::info!(
            subdivisions = viewer.subdiv,
            radius = viewer.radius,
            cells = viewer.stats.cells,
            hash8 = viewer.stats.hash8.as_str(),
            gen_ms = format!("{:.1}", viewer.stats.gen_ms),
            "planet view mesh regenerated",
        );
    }

    /// Rebuild the galaxy-map point buffer after a seed re-roll (the
    /// camera resets inside the view state; nothing else changes).
    fn refresh_map(&mut self) {
        self.map_vertices = upload_map(&self.memory_allocator, &self.debug.galaxy);
        tracing::info!(
            seed = self.debug.galaxy.seed,
            stars = self.debug.galaxy.galaxy.stars.len(),
            "galaxy map regenerated",
        );
    }

    /// Rebuild the system-map buffers after a system load (rings +
    /// points are static per system; focus and pan ride push constants).
    fn refresh_system(&mut self) {
        self.system_points = upload_system_points(&self.memory_allocator, &self.debug.system);
        self.system_lines = upload_system_lines(&self.memory_allocator, &self.debug.system);
        tracing::info!(
            seed = self.debug.system.seed,
            star = self.debug.system.system.id.star_index(),
            planets = self.debug.system.system.planets.len(),
            "system map loaded",
        );
    }

    /// Rebuild all cosmic GPU resources for a new seed (boot, reseed,
    /// staged load): veil volume + HDR chain + inspector buffers + demo
    /// buffers. NEVER on rebase — the rebase path (`tick_cosmic` →
    /// worker → swap) rebuilds the demo buffers only, so no fence wait
    /// is reachable from the frame loop (FR5). Camera motion between
    /// rebuilds rides the MVP push, never these buffers. The inspector
    /// buffers (fixed web-center origin) rebuild here only.
    fn refresh_cosmic_seed(&mut self) {
        let origin = self.debug.cosmic.upload_origin;
        let seed = self.debug.cosmic.seed;
        let veil_mode = self.veil_mode;
        // Veil volume follows the seed (rebase rides the march push);
        // the chain's march set samples it, so the chain rebuilds too
        // (same path as swapchain recreate).
        self.veil_volume = upload_veil_volume(
            &self.memory_allocator,
            &self.command_buffer_allocator,
            &self.queue,
            &self.device,
            &self.debug.cosmic.field,
        );
        // Procedural-tracer resources follow the seed for the same
        // reason (rebase never touches tracers — ADR-026 §2).
        self.disp_volume = upload_displacement_volume(
            &self.memory_allocator,
            &self.command_buffer_allocator,
            &self.queue,
            &self.device,
            &self.debug.cosmic.field,
        );
        let cells = game_engine::universe::web::cell_list(&self.debug.cosmic.field);
        self.cell_count = cells.len();
        self.cell_list = upload_cell_list(&self.memory_allocator, &cells);
        if let Some(ctx) = self.main.as_mut() {
            ctx.hdr = ctx.hdr_format.map(|format| {
                Self::build_hdr_chain(
                    &self.memory_allocator,
                    &self.descriptor_set_allocator,
                    &self.post_sampler,
                    format,
                    ctx.swapchain.image_extent(),
                    &ctx.scene_pass,
                    &ctx.post_pass,
                    &ctx.pipelines,
                    &self
                        .veil_volume
                        .as_ref()
                        .expect("veil volume must upload with the field")
                        .1,
                )
            });
        }
        self.cosmic_glow = upload_cosmic_glow(
            &self.memory_allocator,
            &self.debug.cosmic.web,
            &self.debug.cosmic.field,
            seed,
            origin,
            veil_mode,
        );
        self.cosmic_splats = upload_cosmic_splats(
            &self.memory_allocator,
            &self.debug.cosmic.field,
            &self.debug.cosmic.web,
            origin,
        );
        self.cosmic_tab_glow = upload_cosmic_glow(
            &self.memory_allocator,
            &self.debug.cosmic.web,
            &self.debug.cosmic.field,
            seed,
            glam::DVec3::ZERO,
            veil_mode,
        );
        self.cosmic_tab_splats = upload_cosmic_splats(
            &self.memory_allocator,
            &self.debug.cosmic.field,
            &self.debug.cosmic.web,
            glam::DVec3::ZERO,
        );
        tracing::info!(
            seed = self.debug.cosmic.seed,
            nodes = self.debug.cosmic.web.nodes.len(),
            links = self.debug.cosmic.web.links.len(),
            "cosmic web buffers rebuilt",
        );
        self.reset_rebase_worker();
    }

    /// Synchronously rebuild the demo glow + splat buffers at `origin`
    /// (rebase fallback + the shared build behind the worker — the
    /// worker result and this path produce identical bytes via
    /// `upload_glow_points` / `upload_splat_records`). Veil volume,
    /// HDR chain and inspector buffers are untouched here by
    /// construction (DoD 2).
    fn rebuild_demo_buffers(&mut self, origin: glam::DVec3) {
        let seed = self.debug.cosmic.seed;
        let veil_mode = self.veil_mode;
        self.cosmic_glow = upload_cosmic_glow(
            &self.memory_allocator,
            &self.debug.cosmic.web,
            &self.debug.cosmic.field,
            seed,
            origin,
            veil_mode,
        );
        self.cosmic_splats = upload_cosmic_splats(
            &self.memory_allocator,
            &self.debug.cosmic.field,
            &self.debug.cosmic.web,
            origin,
        );
    }

    /// Recreate the rebase worker over the current seed's inputs and
    /// drop any outstanding request (its generation can never match
    /// again — the swap gate drops it). The old worker joins on drop;
    /// a spawn failure leaves `None` (synchronous fallback in the
    /// tick). Called from the seed path only, never from the tick.
    fn reset_rebase_worker(&mut self) {
        let seed = self.debug.cosmic.seed;
        let veil_mode = self.veil_mode;
        self.rebase_worker = match game_debug::cosmic_rebase::RebaseWorker::try_spawn(
            Arc::new(self.debug.cosmic.web.clone()),
            Arc::new(self.debug.cosmic.field.clone()),
            seed,
            veil_mode,
        ) {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(error = %error, "cosmic rebase worker unavailable: synchronous fallback");
                None
            }
        };
        self.rebase_pending = None;
    }

    /// Precomputed per-frame cosmic draw state (update-2026-09-18-2328):
    /// MVP + sprite scale + per-surface buffers + viewport, shared by
    /// the LDR direct path and the HDR scene/resolve path so both
    /// record identical draws. Also rebuilds the inspector player
    /// point (the ship moves continuously).
    fn cosmic_frame(&mut self, vp: Rect) -> CosmicFrame {
        // Depth window (`cosmic-depth-window`): the demo fades past
        // ~2 fog lengths into the backdrop (length on the dev-widget
        // slider, default 90 Mpc); the inspector shows the
        // full depth unless slab mode windows it.
        let (mvp, px_scale, is_demo, eye, fog_l, slab_center, slab_half) =
            if self.debug.screen == Screen::GameDemo {
                let camera = &self.debug.cosmic.camera;
                // Eye in the demo buffer frame: world truth minus the
                // upload (rebase) origin the demo buffers share.
                let eye_w = camera.eye_world();
                let origin = self.debug.cosmic.upload_origin;
                // Vista intro (CVI-005): while active, fog/slab ride the
                // interpolated pose (slab view → immersive values); the
                // MVP already follows via the camera's external pose.
                // Done restores the slider fog with the slab off.
                let (fog_l, slab_center, slab_half) = if self.debug.cosmic.vista_active() {
                    let pose = self.debug.cosmic.vista.pose();
                    (pose.fog_l_mpc(), pose.slab_center_mpc, pose.slab_half_mpc)
                } else {
                    (self.debug.cosmic.fog_l_mpc, 0.0, 0.0)
                };
                (
                    camera.view_proj(vp.w / vp.h).to_cols_array_2d(),
                    camera.px_scale(vp.h),
                    true,
                    [
                        (eye_w.x - origin.x) as f32,
                        (eye_w.y - origin.y) as f32,
                        (eye_w.z - origin.z) as f32,
                    ],
                    fog_l,
                    slab_center,
                    slab_half,
                )
            } else {
                let inspector = &self.debug.cosmic_inspector;
                // Inspector buffers use the web-center (zero) origin, the
                // same frame the inspector camera's eye is already in.
                let e = inspector.camera.eye();
                let (slab_center, slab_half) = if inspector.slab.on {
                    (
                        inspector.slab.center_mpc,
                        inspector.slab.thickness_mpc * 0.5,
                    )
                } else {
                    (0.0, 0.0)
                };
                (
                    inspector.view_proj(vp.w / vp.h).to_cols_array_2d(),
                    inspector.camera.px_scale(vp.h),
                    false,
                    [e.x, e.y, e.z],
                    0.0,
                    slab_center,
                    slab_half,
                )
            };
        let (glow, splats) = if is_demo {
            (self.cosmic_glow.clone(), self.cosmic_splats.clone())
        } else {
            (self.cosmic_tab_glow.clone(), self.cosmic_tab_splats.clone())
        };
        if !is_demo {
            let ship = self.debug.cosmic.player.position_mpc();
            self.cosmic_tab_player = upload_cosmic_player_point(
                &self.memory_allocator,
                [ship.x as f32, ship.y as f32, ship.z as f32],
            );
        }
        // Veil march push (CGV-005): origin + sphere + grid ride the
        // buffer frame of the active surface (demo origin vs web
        // center); steps from this run's veil mode.
        let march_origin = if is_demo {
            self.debug.cosmic.upload_origin
        } else {
            glam::DVec3::ZERO
        };
        let march_steps = match self.veil_mode {
            game_debug::cosmic_veil::VeilMode::Sprites => 0,
            game_debug::cosmic_veil::VeilMode::March { steps } => steps,
        };
        let march = march_push_for(
            mvp,
            eye,
            march_origin,
            &self.debug.cosmic.field,
            self.debug.cosmic.params.descriptor_radius_mpc as f32,
            fog_l,
            slab_center,
            slab_half,
            march_steps,
        );
        // Procedural-tracer draw (CGT-005): seed resources + the
        // buffer-frame origin (rebase moves it with the swap).
        let proc_draw = match (
            self.disp_volume.as_ref().map(|(_, view)| view.clone()),
            self.veil_volume.as_ref().map(|(_, view)| view.clone()),
            self.cell_list.clone(),
        ) {
            (Some(disp_view), Some(density_view), Some(cells)) => Some(ProcFrame {
                disp_view,
                density_view,
                cells,
                params: SplatProcParams {
                    origin_cell: [
                        march_origin.x as f32,
                        march_origin.y as f32,
                        march_origin.z as f32,
                        self.debug.cosmic.field.cell_size_mpc as f32,
                    ],
                    radius_k: [
                        self.debug.cosmic.field.sphere_radius_mpc as f32,
                        self.splat_k as f32,
                        self.debug.cosmic.field.grid_cells as f32,
                        0.0,
                    ],
                },
                count: (self.cell_count as u32).saturating_mul(self.splat_k as u32),
            }),
            _ => None,
        }
        .filter(|proc| proc.count > 0);
        CosmicFrame {
            mvp,
            px_scale,
            is_demo,
            eye,
            glow,
            splats,
            proc_draw,
            fog_l,
            slab_center,
            slab_half,
            march,
            viewport: Viewport {
                offset: [vp.x, vp.y],
                extent: [vp.w, vp.h],
                depth_range: 0.0..=1.0,
            },
        }
    }

    /// Toggle the inspector slab (`S` on the Cosmic Web tab,
    /// `cosmic-depth-window` FR3): slab mode narrows to 20°
    /// near-orthographic (framing kept by the distance rescale) around
    /// a 30 Mpc slice at the orbit target's view depth; toggling back
    /// restores 60° and the previous framing. Scroll roams the full
    /// sphere depth (clamp ±2600 Mpc — the `SlabState::scroll` radius
    /// is absolute view depth here, not target-relative).
    fn toggle_cosmic_slab(&mut self) {
        use game_debug::cosmic_window::SlabState;
        let inspector = &mut self.debug.cosmic_inspector;
        if inspector.slab.on {
            inspector.slab = SlabState::off();
            inspector.camera.set_fov_keep_framing(60.0);
            self.debug.fx.notify("slab off".to_owned());
        } else {
            let eye = inspector.camera.eye();
            let fwd = (inspector.camera.target() - eye).normalize_or_zero();
            let t = inspector.camera.target();
            let depth =
                ((t.x - eye.x) * fwd.x + (t.y - eye.y) * fwd.y + (t.z - eye.z) * fwd.z).max(0.0);
            let mut slab = SlabState::default_on();
            slab.center_mpc = depth;
            inspector.slab = slab;
            inspector.camera.set_fov_keep_framing(20.0);
            self.debug.fx.notify(format!(
                "slab {} Mpc @ {:.0} Mpc",
                inspector.slab.thickness_mpc, depth
            ));
        }
        tracing::info!(
            on = self.debug.cosmic_inspector.slab.on,
            fov = self.debug.cosmic_inspector.camera.fov_y(),
            "cosmic slab toggled"
        );
    }

    /// Reseed the cosmic demo (web + player + camera + HUD) and rebuild
    /// its buffers: `R` in the demo, `--seed` / seed-field loads.
    fn reseed_cosmic(&mut self, seed: u64) {
        self.debug.cosmic.reseed(seed);
        self.refresh_cosmic_seed();
        self.debug.fx.trigger_fade();
        self.debug.fx.notify(format!(
            "Cosmic web seed {seed} · {} nodes",
            self.debug.cosmic.web.nodes.len()
        ));
        tracing::info!(seed, "cosmic web reseeded");
    }

    /// Toggle fly-to on the click-selected node (`E` in the demo,
    /// shared with the Controls row): a committed leg cancels with a
    /// continuous hand-back, otherwise the selection engages (or hints
    /// when there is no target / the leg is rejected).
    fn toggle_fly_to(&mut self) {
        use game_debug::cosmic_demo::EngageOutcome;

        if self.debug.cosmic.player.cancel_fly_to() {
            self.debug.fx.notify("Fly-to cancelled".to_owned());
            return;
        }
        match self.debug.cosmic.engage_fly_to() {
            EngageOutcome::Engaged => {
                let label = self
                    .debug
                    .cosmic
                    .target_label()
                    .unwrap_or_else(|| "node".to_owned());
                self.debug.fx.notify(format!("Fly-to engaged → {label}"));
            }
            EngageOutcome::NoTarget => {
                self.debug
                    .fx
                    .notify("Click a node to target first · [E] engages".to_owned());
            }
            EngageOutcome::Failed => {
                self.debug
                    .fx
                    .notify("Fly-to rejected: target inside arrival sphere".to_owned());
            }
        }
    }

    /// Route typed text into whichever field holds focus (seed field
    /// on the Settings screen, subdiv/radius on the planet screen).
    /// Every `TextField::insert_char` guards on its own `focused`
    /// flag, so pushing to all three is safe — only the focused one
    /// accepts. Without this, the shortcut arms below (digits, U, R,
    /// E, Q, F, T, G/B) swallow keystrokes meant for the seed field:
    /// digits are the whole u64 seed alphabet, so typing a seed
    /// appears to do nothing.
    fn type_into_focused_fields(&mut self, text: &str) {
        for ch in text.chars() {
            self.debug.settings.seed_field.insert_char(ch);
            self.debug.viewer.subdiv_field.insert_char(ch);
            self.debug.viewer.radius_field.insert_char(ch);
        }
        self.debug.viewer.sync_slider_from_field();
    }

    /// Start a staged universe load for `seed` (single load path for
    /// the Settings Load button, Enter-on-field, `R` re-roll, and the
    /// `--seed` boot flag): one [`LoadStep`] runs per frame in
    /// `draw_main`, so the modal progress bar stays alive. Ignored
    /// while a load is already in flight.
    fn begin_load(&mut self, seed: u64, source: LoadSource) {
        if self.debug.loading.is_none() {
            self.debug.loading = Some(LoadPlan::new(seed, source));
        }
    }

    /// Execute one staged load step (CPU regen or GPU re-upload). The
    /// Finalize step fades over the current screen — a load never
    /// switches tabs — and the notify closes out in `finish_load`
    /// once every step has run.
    fn exec_load_step(&mut self, seed: u64, step: LoadStep) {
        match step {
            LoadStep::Galaxy => self.debug.galaxy.regenerate(seed),
            LoadStep::Journey => self.debug.journey = Journey::new(seed),
            LoadStep::System => {
                let star0 = self.debug.galaxy.galaxy.stars[0].clone();
                self.debug.system.load(seed, &star0);
            }
            LoadStep::Cosmic => self.debug.cosmic.reseed(seed),
            LoadStep::UploadMap => self.refresh_map(),
            LoadStep::UploadSystem => {
                self.transit_acc = 0.0;
                self.refresh_system();
            }
            LoadStep::UploadCosmic => self.refresh_cosmic_seed(),
            LoadStep::Finalize => {
                // Stay on the current screen: a load never switches
                // tabs (the fade + notify announce the swap).
                self.debug.fx.trigger_fade();
            }
        }
    }

    /// Close out a staged load: the Settings field mirrors the new
    /// universe seed, and the notify confirms it.
    fn finish_load(&mut self, plan: LoadPlan) {
        let stars = self.debug.galaxy.galaxy.stars.len();
        self.debug.settings.sync_seed(plan.seed);
        let suffix = match plan.source {
            LoadSource::Panel => format!("Seed {} · {stars} stars", plan.seed),
            LoadSource::Boot => format!("Seed {} · --seed flag", plan.seed),
        };
        self.debug.fx.notify(suffix);
        tracing::info!(seed = plan.seed, "universe seed loaded");
    }

    /// Recompute the hovered chunk from the current cursor: only with
    /// planet 3D content mounted and the cursor inside the main
    /// viewport. A miss (cursor over empty space, panel, or nav)
    /// clears the hover. Hover feeds the fill highlight + panel CHUNK
    /// readout; it never touches the mesh. Callers refresh after every
    /// cursor or camera move so the highlight tracks within one frame.
    fn update_hover(&mut self) {
        let hovered = self
            .main
            .as_ref()
            .and_then(|ctx| ctx.last_cursor.map(|cursor| (cursor, ctx.size())))
            .filter(|_| self.debug.screen_content() == Some(ViewContent::PlanetView))
            .and_then(|((cx, cy), (w, h))| {
                let layout = app_layout(self.debug.chrome, w, h);
                let rect = layout.viewport;
                if !rect.contains(cx, cy) || rect.w < 1.0 || rect.h < 1.0 {
                    return None;
                }
                let aspect = rect.w / rect.h;
                // Player mode picks through the player camera.
                let view_proj = if self.debug.viewer.player.active {
                    let player = &self.debug.viewer.player;
                    player.projection_matrix(aspect) * player.view_matrix()
                } else {
                    self.camera.projection_matrix(aspect) * self.camera.view_matrix()
                };
                let ray = ray_from_cursor((cx, cy), rect, view_proj);
                let hit = intersect_sphere(ray, self.debug.viewer.radius)?;
                let viewer = &self.debug.viewer;
                Some(pick_cell(&viewer.mesh, hit, viewer.hovered))
            });
        self.debug.viewer.hovered = hovered;
    }

    /// (Re)build the atlas image + the window's descriptor set when the
    /// atlas grew; upload texels when the version changed.
    fn sync_atlas(&mut self) {
        let extent = self.atlas.extent();
        let grown = self
            .atlas_image
            .as_ref()
            .is_none_or(|image| image.extent()[0] != extent);
        if grown {
            let image = create_atlas_image(&self.memory_allocator, extent);
            self.atlas_image = Some(image);
            self.uploaded_atlas_version = None;
        }
        let ctx = self.main.as_mut().expect("main window must exist");
        if ctx.atlas_set.is_none() || ctx.atlas_set_extent != extent {
            let set_layout = ctx.pipelines.ui.layout().set_layouts()[0].clone();
            let view =
                ImageView::new_default(self.atlas_image.clone().expect("atlas image must exist"))
                    .expect("atlas view must create");
            let set = DescriptorSet::new(
                self.descriptor_set_allocator.clone(),
                set_layout,
                [
                    WriteDescriptorSet::image_view(0, view),
                    WriteDescriptorSet::sampler(1, self.sampler.clone()),
                ],
                [],
            )
            .expect("atlas descriptor set must create");
            ctx.atlas_set = Some(set);
            ctx.atlas_set_extent = extent;
        }
        if self.uploaded_atlas_version != Some(self.atlas.version()) {
            upload_atlas(
                &self.device,
                &self.queue,
                &self.memory_allocator,
                &self.command_buffer_allocator,
                self.atlas_image.as_ref().expect("atlas image must exist"),
                self.atlas.texture(),
            );
            self.uploaded_atlas_version = Some(self.atlas.version());
        }
    }

    /// Window pipelines + the HDR scene/post passes. The HDR format
    /// selection is a physical-device query (no window needed — the
    /// `game_tools` boot pattern); `None` means LDR bypass (cosmic
    /// views draw the same layouts direct-to-swapchain, minus bloom).
    fn build_pipelines(
        &self,
        swapchain: &Arc<Swapchain>,
    ) -> (
        Arc<RenderPass>,
        Pipelines,
        Option<Format>,
        bool,
        Arc<RenderPass>,
        Arc<RenderPass>,
    ) {
        let render_pass = vulkano::single_pass_renderpass!(
            self.device.clone(),
            attachments: {
                color: {
                    format: swapchain.image_format(),
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                },
                depth: {
                    format: DEPTH_FORMAT,
                    samples: 1,
                    load_op: Clear,
                    store_op: DontCare,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {depth},
            },
        )
        .expect("render pass must create");
        // `GAME_DEBUG_COSMIC_POST=0` forces the LDR direct path and
        // `GAME_DEBUG_COSMIC_BLOOM=0` keeps HDR scene + resolve while
        // skipping the bright/blur chain: diagnostic A/B switches for
        // the post chain, and fallbacks for drivers that misbehave
        // under HDR.
        let post_disabled = std::env::var("GAME_DEBUG_COSMIC_POST").is_ok_and(|value| value == "0");
        let bloom_enabled =
            !std::env::var("GAME_DEBUG_COSMIC_BLOOM").is_ok_and(|value| value == "0");
        let hdr_format = if post_disabled {
            tracing::info!("GAME_DEBUG_COSMIC_POST=0: LDR bypass forced");
            None
        } else {
            match select_hdr_format(|format| hdr_support(self.device.physical_device(), format)) {
                HdrSelection::Hdr(format) => Some(format),
                HdrSelection::LdrBypass => None,
            }
        };
        // Scene + post passes exist regardless (cheap objects); the
        // transient images driving them exist only in HDR mode.
        let scene_format = hdr_format.unwrap_or(swapchain.image_format());
        let scene_pass = build_scene_pass(&self.device, scene_format);
        let post_pass = build_post_pass(&self.device, scene_format);
        let pipelines = Pipelines {
            fill: build_fill_pipeline(&self.device, &self.shaders, &render_pass),
            line: build_line_pipeline(&self.device, &self.shaders, &render_pass),
            ui: build_ui_pipeline(&self.device, &self.shaders, &render_pass),
            map: build_map_pipeline(&self.device, &self.shaders, &render_pass),
            map_glow: build_glow_pipeline(&self.device, &self.shaders, &render_pass),
            splat: build_splat_pipeline(&self.device, &self.shaders, &render_pass),
            splat_proc: build_splat_proc_pipeline(&self.device, &self.shaders, &render_pass),
            glow_scene: build_glow_pipeline(&self.device, &self.shaders, &scene_pass),
            splat_scene: build_splat_pipeline(&self.device, &self.shaders, &scene_pass),
            splat_proc_scene: build_splat_proc_pipeline(&self.device, &self.shaders, &scene_pass),
            prefilter: build_post_pipeline(
                &self.device,
                &self.shaders.prefilter_frag,
                &self.shaders.post_vert,
                &post_pass,
                None,
                "bloom prefilter",
            ),
            down: build_post_pipeline(
                &self.device,
                &self.shaders.down_frag,
                &self.shaders.post_vert,
                &post_pass,
                None,
                "bloom down",
            ),
            up: build_post_pipeline(
                &self.device,
                &self.shaders.up_frag,
                &self.shaders.post_vert,
                &post_pass,
                None,
                "bloom up",
            ),
            march: build_post_pipeline(
                &self.device,
                &self.shaders.march_frag,
                &self.shaders.post_vert,
                &post_pass,
                None,
                "veil march",
            ),
            resolve: build_post_pipeline(
                &self.device,
                &self.shaders.resolve_frag,
                &self.shaders.post_vert,
                &render_pass,
                // State present, test off (the UI-pipeline
                // precedent): the main subpass owns a depth
                // attachment (VUID-06043).
                Some(DepthStencilState::default()),
                "bloom resolve",
            ),
        };
        (
            render_pass,
            pipelines,
            hdr_format,
            bloom_enabled,
            scene_pass,
            post_pass,
        )
    }

    /// Build (or rebuild, on swapchain recreate) the transient HDR
    /// bloom resources for the window: scene target + depth, five
    /// dedicated bloom targets A-E (write-once, never ping-ponged),
    /// and their sampling sets. `None` in LDR
    /// bypass (no transients — the cosmic draws go direct). Associated
    /// function (not a method) so the recreate path can pass disjoint
    /// `self` fields alongside the `&mut` window context.
    #[allow(clippy::too_many_arguments)] // builder fn: explicit resources, two call sites + capture
    fn build_hdr_chain(
        memory_allocator: &Arc<StandardMemoryAllocator>,
        descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
        post_sampler: &Arc<Sampler>,
        format: Format,
        extent: [u32; 2],
        scene_pass: &Arc<RenderPass>,
        post_pass: &Arc<RenderPass>,
        pipes: &Pipelines,
        march_volume: &Arc<ImageView>,
    ) -> HdrChain {
        let levels = BLOOM_LEVELS;
        let scene_view = create_post_view(memory_allocator, extent, format, "HDR scene");
        let scene_depth = create_depth_view(memory_allocator, extent);
        let scene_fb = Framebuffer::new(
            scene_pass.clone(),
            FramebufferCreateInfo {
                attachments: vec![scene_view.clone(), scene_depth.clone()],
                ..Default::default()
            },
        )
        .expect("HDR scene framebuffer must create");
        // Mip pyramid (`bloom-mip-chain`): `down[k]` at `extent >>
        // (k+1)` (floored at 1 px), `up[k]` at the same extents. Every
        // level is its own image — written once, then only read.
        let level_extent = |k: u8| {
            [
                (extent[0] >> (u32::from(k) + 1)).max(1),
                (extent[1] >> (u32::from(k) + 1)).max(1),
            ]
        };
        let mut down = Vec::with_capacity(usize::from(levels));
        let mut up = Vec::with_capacity(usize::from(levels));
        for k in 0..levels {
            for (pyramid, what) in [(&mut down, "down"), (&mut up, "up")] {
                let ext = level_extent(k);
                // TRANSFER_SRC + TRANSFER_DST: the alias-free
                // pass-through copies `down[last]` → `up[last]` with an
                // exact image copy (never a filtered blit).
                let image = Image::new(
                    memory_allocator.clone(),
                    ImageCreateInfo {
                        image_type: ImageType::Dim2d,
                        format,
                        extent: [ext[0], ext[1], 1],
                        usage: ImageUsage::COLOR_ATTACHMENT
                            | ImageUsage::SAMPLED
                            | ImageUsage::TRANSFER_SRC
                            | ImageUsage::TRANSFER_DST,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|error| panic!("bloom {what}[{k}] image must create: {error}"));
                let view = ImageView::new_default(image.clone())
                    .unwrap_or_else(|error| panic!("bloom {what}[{k}] view must create: {error}"));
                let fb = Framebuffer::new(
                    post_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![view.clone()],
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|error| {
                    panic!("bloom {what}[{k}] framebuffer must create: {error:?}")
                });
                pyramid.push(BloomLevel {
                    image,
                    view,
                    fb,
                    extent: ext,
                });
            }
        }
        let prefilter_set = post_image_set(
            descriptor_set_allocator,
            &pipes.prefilter,
            &scene_view,
            post_sampler,
            "bloom prefilter",
        );
        let mut down_set = Vec::with_capacity(usize::from(levels));
        for (k, level) in down.iter().enumerate() {
            down_set.push(post_image_set(
                descriptor_set_allocator,
                &pipes.down,
                &level.view,
                post_sampler,
                &format!("bloom down[{k}]"),
            ));
        }
        // Up sets sample the (coarse, fine) pair for levels below the
        // top: `up[k] = tent(up[k+1])·w + down[k]`.
        let mut up_set = Vec::with_capacity(usize::from(levels).saturating_sub(1));
        for k in 0..levels.saturating_sub(1) {
            up_set.push(post_image_pair_set(
                descriptor_set_allocator,
                &pipes.up,
                &up[usize::from(k) + 1].view,
                &down[usize::from(k)].view,
                post_sampler,
                &format!("bloom up[{k}]"),
            ));
        }
        // Resolve samples scene + pyramid top + march target: the
        // third pair is written against the same layout explicitly.
        let resolve_layout = pipes.resolve.layout().set_layouts()[0].clone();
        let resolve_pair =
            |bloom_view: &Arc<ImageView>, march_view: &Arc<ImageView>, what: &str| {
                DescriptorSet::new(
                    descriptor_set_allocator.clone(),
                    resolve_layout.clone(),
                    [
                        WriteDescriptorSet::image_view(0, scene_view.clone()),
                        WriteDescriptorSet::sampler(1, post_sampler.clone()),
                        WriteDescriptorSet::image_view(2, bloom_view.clone()),
                        WriteDescriptorSet::sampler(3, post_sampler.clone()),
                        WriteDescriptorSet::image_view(4, march_view.clone()),
                        WriteDescriptorSet::sampler(5, post_sampler.clone()),
                    ],
                    [],
                )
                .unwrap_or_else(|error| panic!("{what} descriptor set must create: {error:?}"))
            };
        // March target (`cosmic-gas-veil-v2`): quarter-res HDR, own
        // framebuffer under the post pass — written once by the march
        // pass, read only at the resolve. In sprites mode the pass is
        // skipped and the cleared target adds ~0.
        let march_extent = [(extent[0] / 4).max(1), (extent[1] / 4).max(1)];
        let march_view = create_post_view(memory_allocator, march_extent, format, "veil march");
        let march_fb = Framebuffer::new(
            post_pass.clone(),
            FramebufferCreateInfo {
                attachments: vec![march_view.clone()],
                ..Default::default()
            },
        )
        .expect("veil march framebuffer must create");
        let march_set = post_image_set(
            descriptor_set_allocator,
            &pipes.march,
            march_volume,
            post_sampler,
            "veil march volume",
        );
        let resolve_set = resolve_pair(&up[0].view, &march_view, "bloom resolve");
        let resolve_nobloom_set = resolve_pair(&down[0].view, &march_view, "bloom resolve nobloom");
        HdrChain {
            format,
            scene_fb,
            scene_extent: extent,
            levels,
            down,
            up,
            prefilter_set,
            down_set,
            up_set,
            march_fb,
            march_extent,
            march_set,
            resolve_set,
            resolve_nobloom_set,
        }
    }
}

struct Pipelines {
    fill: Arc<GraphicsPipeline>,
    line: Arc<GraphicsPipeline>,
    ui: Arc<GraphicsPipeline>,
    map: Arc<GraphicsPipeline>,
    /// Cosmic glow sprites (additive, update-2026-09-18-2328).
    map_glow: Arc<GraphicsPipeline>,
    /// Cosmic tracer splats (additive variant, `cosmic-tracer-splat`).
    splat: Arc<GraphicsPipeline>,
    /// Procedural GPU tracers (no vertex input, `cosmic-gpu-tracers`).
    splat_proc: Arc<GraphicsPipeline>,
    /// Scene-pass variants of the cosmic pipelines (HDR mode).
    glow_scene: Arc<GraphicsPipeline>,
    splat_scene: Arc<GraphicsPipeline>,
    splat_proc_scene: Arc<GraphicsPipeline>,
    /// Mip-bloom prefilter (scene → down[0], post pass).
    prefilter: Arc<GraphicsPipeline>,
    /// Mip-bloom downsample step (post pass).
    down: Arc<GraphicsPipeline>,
    /// Mip-bloom upsample step (post pass).
    up: Arc<GraphicsPipeline>,
    /// Gas-veil raymarch (quarter-res target, post pass).
    march: Arc<GraphicsPipeline>,
    /// Bloom-composite ACES resolve (main pass).
    resolve: Arc<GraphicsPipeline>,
}

fn window_size_dependent_setup(
    images: &[Arc<Image>],
    render_pass: &Arc<RenderPass>,
    depth_view: &Arc<ImageView>,
) -> Vec<Arc<Framebuffer>> {
    images
        .iter()
        .map(|image| {
            let view = ImageView::new_default(image.clone()).expect("swapchain image view");
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![view, depth_view.clone()],
                    ..Default::default()
                },
            )
            .expect("framebuffer must create")
        })
        .collect()
}

impl ViewerApp {
    /// Create the OS window + swapchain + per-window GPU state.
    fn create_window(&self, event_loop: &ActiveEventLoop, title: &str) -> WindowContext {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));
        let window = Arc::new(event_loop.create_window(attrs).expect("window must create"));
        let surface = Surface::from_window(self.instance.clone(), window.clone())
            .expect("surface must create");
        let window_size = window.inner_size();
        let (swapchain, images) = {
            let capabilities = self
                .device
                .physical_device()
                .surface_capabilities(&surface, Default::default())
                .expect("surface capabilities must query");
            let (image_format, _) = self
                .device
                .physical_device()
                .surface_formats(&surface, Default::default())
                .expect("surface formats must query")[0];
            Swapchain::new(
                self.device.clone(),
                surface,
                SwapchainCreateInfo {
                    min_image_count: capabilities.min_image_count.max(2),
                    image_format,
                    image_extent: window_size.into(),
                    image_usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
                    composite_alpha: capabilities
                        .supported_composite_alpha
                        .into_iter()
                        .next()
                        .expect("composite alpha mode must exist"),
                    ..Default::default()
                },
            )
            .expect("swapchain must create")
        };
        let (render_pass, pipelines, hdr_format, bloom_enabled, scene_pass, post_pass) =
            self.build_pipelines(&swapchain);
        let depth_view = create_depth_view(&self.memory_allocator, swapchain.image_extent());
        let framebuffers = window_size_dependent_setup(&images, &render_pass, &depth_view);
        let mut ctx = WindowContext {
            window,
            swapchain,
            swapchain_images: images.clone(),
            render_pass,
            pipelines,
            framebuffers,
            depth_view,
            atlas_set: None,
            atlas_set_extent: 0,
            last_cursor: None,
            recreate_swapchain: false,
            previous_frame_end: Some(sync::now(self.device.clone()).boxed()),
            hdr_format,
            bloom_enabled,
            scene_pass,
            post_pass,
            hdr: None,
        };
        ctx.hdr = ctx.hdr_format.map(|format| {
            Self::build_hdr_chain(
                &self.memory_allocator,
                &self.descriptor_set_allocator,
                &self.post_sampler,
                format,
                ctx.swapchain.image_extent(),
                &ctx.scene_pass,
                &ctx.post_pass,
                &ctx.pipelines,
                &self
                    .veil_volume
                    .as_ref()
                    .expect("veil volume must upload at boot")
                    .1,
            )
        });
        if let Some(chain) = ctx.hdr.as_ref() {
            tracing::info!(format = ?chain.format, "HDR cosmic post chain active");
        } else {
            tracing::info!("LDR bypass: cosmic views draw direct-to-swapchain");
        }
        ctx
    }

    /// Whether an event belongs to the window (`false` for stale ids
    /// after the window closed).
    fn is_main_window(&self, window_id: WindowId) -> bool {
        self.main
            .as_ref()
            .is_some_and(|ctx| ctx.window.id() == window_id)
    }
}

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once: only create the window
        // when no live context exists yet.
        if self.main.is_none() {
            self.main = Some(self.create_window(event_loop, "PlanetCrafter — debug"));
        }
        // Atlas image + descriptor set need the UI pipeline: built
        // lazily on the first frame via `sync_atlas`.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.is_main_window(window_id) {
            self.main_window_event(event_loop, event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // One FPS sample per event-loop iteration on the single window.
        let now = Instant::now();
        if let Some(last) = self.last_fps_tick {
            self.debug.fps.record((now - last).as_secs_f32());
        }
        self.last_fps_tick = Some(now);
        // Drain new transition history into the console feed once per
        // iteration (the widget Console tab reads it).
        self.debug.sync_console();
        if let Some(ctx) = self.main.as_ref() {
            ctx.window.request_redraw();
        }
    }
}
impl ViewerApp {
    /// Single-window events (demo / dimension / settings screens).
    fn main_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_held = modifiers.state().shift_key();
            }
            WindowEvent::Resized(_) => {
                if let Some(ctx) = self.main.as_mut() {
                    ctx.recreate_swapchain = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = (position.x as f32, position.y as f32);
                let content = self.debug.screen_content();
                if self.dragging_fog {
                    // Demo-fog slider drag (`cosmic-depth-window` FR5).
                    if let Some(ctx) = self.main.as_ref() {
                        let (w, h) = ctx.size();
                        let lh = self.atlas.line_height();
                        let track = fog_slider_track(widget_body_rect(ui::widget_rect(w, h)), lh);
                        let mut slider =
                            ui::Slider::new(30, 400, self.debug.cosmic.fog_l_mpc as u32);
                        slider.drag_to(track, cursor.0);
                        self.debug.cosmic.fog_l_mpc = slider.value as f32;
                    }
                } else if self.dragging_slider {
                    if let Some(ctx) = self.main.as_ref() {
                        let (w, h) = ctx.size();
                        let viewer = &mut self.debug.viewer;
                        let lh = self.atlas.line_height();
                        let warn =
                            parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                        let layout = app_layout(self.debug.chrome, w, h);
                        let track = right_panel_plan(layout.panel, lh, warn, viewer.player.active)
                            .rects
                            .subdiv_track;
                        viewer.subdiv_slider.drag_to(track, cursor.0);
                        viewer.sync_field_from_slider();
                    }
                } else if self.dragging_density {
                    if let Some(ctx) = self.main.as_ref() {
                        let (w, h) = ctx.size();
                        let viewer = &mut self.debug.viewer;
                        let lh = self.atlas.line_height();
                        let layout = app_layout(self.debug.chrome, w, h);
                        let checker = viewer.debug_mode == DebugMode::Checker;
                        let track = match content {
                            Some(ViewContent::PlanetView) => {
                                planet_left_plan(layout.left, lh, checker)
                                    .rects
                                    .density_track
                            }
                            // No density slider on the map screens.
                            _ => None,
                        };
                        if let Some(track) = track {
                            viewer.density_slider.drag_to(track, cursor.0);
                            viewer.sync_density_from_slider();
                        }
                    }
                } else if self.dragging_orbit {
                    let last = self.main.as_ref().and_then(|ctx| ctx.last_cursor);
                    if let Some(last) = last {
                        let (dx, dy) = (cursor.0 - last.0, cursor.1 - last.1);
                        if content == Some(ViewContent::GalaxyMap)
                            || content == Some(ViewContent::SystemMap)
                        {
                            // Map orbit: left-drag rotates the 3D map
                            // camera (universe-maps-3d; right/middle
                            // drag pans, below).
                            if content == Some(ViewContent::GalaxyMap) {
                                self.debug.galaxy.camera.rotate(dx, dy);
                            } else {
                                self.debug.system.camera.rotate(dx, dy);
                            }
                        } else if matches!(self.debug.screen, Screen::GameDemo) {
                            // Cosmic demo: left-drag steers the ship nose
                            // (Chase/FirstPerson follow it; Orbit keeps
                            // its free-look angles). Checked before player
                            // mode: on this tab there is no walker view,
                            // so an armed walker must not swallow drags.
                            // Vista intro (CVI-004): a ≥ 4 px drag skips
                            // to Chase first; the drag then steers as
                            // usual (controls live immediately after).
                            if dx.hypot(dy) >= 4.0 {
                                self.debug.cosmic.skip_vista();
                            }
                            self.debug.cosmic.steer(dx, dy);
                        } else if content == Some(ViewContent::CosmicWeb) {
                            // Cosmic inspector tab: left-drag orbits the
                            // inspector camera (read-only).
                            self.debug.cosmic_inspector.camera.rotate(dx, dy);
                        } else if self.debug.viewer.player.active {
                            // Player mode orbits the follow camera instead
                            // of the free global one (first/third person
                            // ignore rotation).
                            self.debug.viewer.player.rotate_camera(dx, dy);
                        } else {
                            self.camera.rotate(dx, dy);
                        }
                    }
                } else if self.dragging_pan {
                    // Map pan: right/middle-drag moves the 3D map camera
                    // target in the view plane (content follows the
                    // cursor). The planet content never sets this flag.
                    let last = self.main.as_ref().and_then(|ctx| ctx.last_cursor);
                    if let Some(last) = last
                        && let Some(ctx) = self.main.as_ref()
                    {
                        let (dx, dy) = (cursor.0 - last.0, cursor.1 - last.1);
                        let (w, h) = ctx.size();
                        let vp = app_layout(self.debug.chrome, w, h).viewport;
                        if content == Some(ViewContent::GalaxyMap) {
                            self.debug.galaxy.camera.pan_screen(dx, dy, vp.h);
                        } else if content == Some(ViewContent::SystemMap) {
                            self.debug.system.camera.pan_screen(dx, dy, vp.h);
                        } else if content == Some(ViewContent::CosmicWeb) {
                            // Inspector pan (the demo tab never sets the
                            // pan flag — it steers on left-drag).
                            self.debug.cosmic_inspector.camera.pan_screen(dx, dy, vp.h);
                        }
                    }
                }
                if let Some(ctx) = self.main.as_mut() {
                    ctx.last_cursor = Some(cursor);
                }
                // Hover tracks cursor AND camera moves (orbit drags change
                // the cells under a static cursor).
                self.update_hover();
            }
            WindowEvent::MouseInput { button, state, .. } => {
                let pressed = state == ElementState::Pressed;
                let content = self.debug.screen_content();
                // Right/middle buttons pan the 3D map cameras
                // (universe-maps-3d; left-drag orbits there). Other
                // buttons are ignored.
                if matches!(button, MouseButton::Right | MouseButton::Middle) {
                    if !pressed {
                        self.dragging_pan = false;
                    } else if content == Some(ViewContent::GalaxyMap)
                        || content == Some(ViewContent::SystemMap)
                    {
                        let in_viewport = self.main.as_ref().is_some_and(|ctx| {
                            let (w, h) = ctx.size();
                            ctx.last_cursor.is_some_and(|(cx, cy)| {
                                app_layout(self.debug.chrome, w, h)
                                    .viewport
                                    .contains(cx, cy)
                            })
                        });
                        if in_viewport {
                            self.dragging_pan = true;
                        }
                    }
                    return;
                }
                if button != MouseButton::Left {
                    return;
                }
                if !pressed {
                    // Release: clear a mouse-held walk flag first
                    // (Settings Controls hold-to-press parity) — walker
                    // and cosmic thrust alike, so neither sticks.
                    if self.mouse_walk.take().is_some() {
                        let viewer = &mut self.debug.viewer;
                        let mut keys = viewer.player.keys();
                        keys.north = false;
                        keys.south = false;
                        keys.west = false;
                        keys.east = false;
                        viewer.player.set_keys(keys);
                        self.debug.cosmic.held.clear();
                        return;
                    }
                    // A press that barely traveled counts as a
                    // click — pin the hovered chunk. The release must
                    // still land in the planet viewport (pinning only
                    // exists with planet content).
                    let (cursor, in_viewport) = match self.main.as_ref() {
                        Some(ctx) => {
                            let (w, h) = ctx.size();
                            let layout = app_layout(self.debug.chrome, w, h);
                            (
                                ctx.last_cursor,
                                ctx.last_cursor
                                    .is_some_and(|(cx, cy)| layout.viewport.contains(cx, cy)),
                            )
                        }
                        None => (None, false),
                    };
                    let click =
                        self.press_cursor
                            .zip(cursor)
                            .is_some_and(|((px, py), (cx, cy))| {
                                (cx - px).hypot(cy - py) <= CLICK_MAX_DRAG_PX
                            });
                    self.dragging_orbit = false;
                    self.dragging_slider = false;
                    self.dragging_density = false;
                    // Demo-fog slider release (`cosmic-depth-window`
                    // FR5): Console logs the settled value.
                    if self.dragging_fog {
                        self.dragging_fog = false;
                        let line = format!("demo fog {:.0} Mpc", self.debug.cosmic.fog_l_mpc);
                        tracing::info!("{line}");
                        self.debug.console.push(line);
                    }
                    self.press_cursor = None;
                    if click
                        && content == Some(ViewContent::PlanetView)
                        && in_viewport
                        && let Some(chunk) = self.debug.viewer.hovered
                    {
                        self.debug.viewer.toggle_pin(chunk);
                    }
                    // Galaxy click (same barely-traveled rule): pick the
                    // nearest star into the SELECTION dock + arm the
                    // journey machine.
                    if click
                        && content == Some(ViewContent::GalaxyMap)
                        && in_viewport
                        && let Some((cx, cy)) = cursor
                    {
                        let layout = match self.main.as_ref() {
                            Some(ctx) => {
                                let (w, h) = ctx.size();
                                app_layout(self.debug.chrome, w, h)
                            }
                            None => return,
                        };
                        let vp = layout.viewport;
                        if let Some(i) = self.debug.galaxy.select_at((cx, cy), vp) {
                            self.debug.journey.update(JourneyEvent::SelectStar(i));
                        }
                    }
                    // System click: pick the nearest planet the same way.
                    if click
                        && content == Some(ViewContent::SystemMap)
                        && in_viewport
                        && let Some((cx, cy)) = cursor
                    {
                        let layout = match self.main.as_ref() {
                            Some(ctx) => {
                                let (w, h) = ctx.size();
                                app_layout(self.debug.chrome, w, h)
                            }
                            None => return,
                        };
                        let vp = layout.viewport;
                        if let Some(i) = self.debug.system.select_at((cx, cy), vp) {
                            self.debug.journey.update(JourneyEvent::SelectPlanet(i));
                        }
                        // A miss clears the screen selection; the machine
                        // keeps its armed planet — the transit UI re-arms
                        // from screen state before committing, so the
                        // stale arm can never fire.
                    }
                    // Cosmic inspector click: pick the nearest node into
                    // the readout (read-only — no journey mutation).
                    // The demo tab targets fly-to instead (below).
                    if click
                        && content == Some(ViewContent::CosmicWeb)
                        && !matches!(self.debug.screen, Screen::GameDemo)
                        && in_viewport
                        && let Some((cx, cy)) = cursor
                    {
                        let layout = match self.main.as_ref() {
                            Some(ctx) => {
                                let (w, h) = ctx.size();
                                app_layout(self.debug.chrome, w, h)
                            }
                            None => return,
                        };
                        let vp = layout.viewport;
                        let web = &self.debug.cosmic.web;
                        self.debug
                            .cosmic_inspector
                            .select_at(web, glam::DVec3::ZERO, (cx, cy), vp);
                    }
                    // Cosmic demo click: pick the nearest node as the
                    // fly-to target (a miss clears it). Vista intro
                    // (CVI-004/NFR2): the click skips to Chase first;
                    // selection stays gated while the vista owns the
                    // camera, so a skip-click never also selects.
                    if click
                        && matches!(self.debug.screen, Screen::GameDemo)
                        && in_viewport
                        && let Some((cx, cy)) = cursor
                    {
                        self.debug.cosmic.skip_vista();
                        let layout = match self.main.as_ref() {
                            Some(ctx) => {
                                let (w, h) = ctx.size();
                                app_layout(self.debug.chrome, w, h)
                            }
                            None => return,
                        };
                        let vp = layout.viewport;
                        if let Some(i) = self.debug.cosmic.select_node_at((cx, cy), vp) {
                            self.debug
                                .fx
                                .notify(format!("Target node {i} · [E] fly-to"));
                        }
                    }
                    return;
                }
                let (cx, cy, w, h) = match self.main.as_ref() {
                    Some(ctx) => match ctx.last_cursor {
                        Some((cx, cy)) => {
                            let (w, h) = ctx.size();
                            (cx, cy, w, h)
                        }
                        None => return,
                    },
                    None => return,
                };
                let layout = app_layout(self.debug.chrome, w, h);
                let lh = self.atlas.line_height();
                // Chrome first, topmost surface wins: dropdown panel,
                // top bar, corner strip, dev widget, settings rows.
                // A staged universe load is modal: every click is
                // ignored while it runs (no cancel in v1).
                if self.debug.loading.is_some() {
                    return;
                }
                if self.debug.dropdown_open {
                    let panel = ui::dropdown_panel(layout.nav);
                    for (index, waypoint) in DROPDOWN_ORDER.iter().enumerate() {
                        if ui::dropdown_row(panel, index).contains(cx, cy) {
                            self.debug.select_screen(Screen::Dimensions(*waypoint));
                            return;
                        }
                    }
                    // Click outside the panel dismisses it — unless the
                    // click hits the Dimensions button itself, which
                    // toggles below.
                    if !panel.contains(cx, cy) && !ui::topbar_button(layout.nav, 1).contains(cx, cy)
                    {
                        self.debug.close_dropdown();
                    }
                }
                // Top bar is always visible: check its buttons first.
                for i in 0..3 {
                    if ui::topbar_button(layout.nav, i).contains(cx, cy) {
                        match i {
                            0 => self.debug.select_screen(Screen::GameDemo),
                            1 => self.debug.toggle_dropdown(),
                            _ => self.debug.select_screen(Screen::Settings),
                        }
                        return;
                    }
                }
                {
                    let strip = ui::corner_strip(w);
                    for i in 0..3 {
                        if ui::corner_button(strip, i).contains(cx, cy) {
                            match i {
                                0 => self.debug.toggle_left_dock(),
                                1 => self.debug.toggle_right_dock(),
                                _ => self.debug.toggle_widget(),
                            }
                            return;
                        }
                    }
                }
                if self.debug.widget_visible {
                    let widget = ui::widget_rect(w, h);
                    if widget.contains(cx, cy) {
                        self.debug.widget_focused = true;
                        for (i, tab) in WidgetTab::ALL.iter().enumerate() {
                            if ui::widget_tab_button(widget, i).contains(cx, cy) {
                                self.debug.select_widget_tab(*tab);
                            }
                        }
                        // Demo-fog slider (`cosmic-depth-window` FR5):
                        // Inspector tab body only.
                        if self.debug.widget_tab == WidgetTab::Inspector {
                            let body = widget_body_rect(widget);
                            let track = fog_slider_track(body, self.atlas.line_height());
                            if track.contains(cx, cy) {
                                self.dragging_fog = true;
                                let mut slider =
                                    ui::Slider::new(30, 400, self.debug.cosmic.fog_l_mpc as u32);
                                slider.drag_to(track, cx);
                                self.debug.cosmic.fog_l_mpc = slider.value as f32;
                                return;
                            }
                        }
                        return;
                    }
                }
                if self.debug.screen == Screen::Settings {
                    // UNIVERSE editor (left dock): click focuses the
                    // field, Load starts the staged load.
                    let dock = settings_left_plan(layout.left, lh);
                    self.debug.settings.seed_field.click(dock.field, cx, cy);
                    if dock.load.contains(cx, cy)
                        && let Ok(seed) = self.debug.settings.seed_field.text.parse::<u64>()
                    {
                        self.begin_load(seed, LoadSource::Panel);
                        return;
                    }
                    let plan = controls_plan(layout.viewport, lh);
                    for row in &plan.rows {
                        if row.rect.contains(cx, cy) {
                            self.fire_action(row.action);
                            return;
                        }
                    }
                }
                // Viewport drag starts an orbit drag (left button
                // everywhere now — the 3D map cameras orbit on left,
                // pan on right/middle).
                if layout.viewport.contains(cx, cy) {
                    self.dragging_orbit = true;
                    // A planet-content press may end as a chunk-pin click,
                    // a map-content press as a star/planet-pick click (all
                    // decided on release by travel distance).
                    if content.is_some() {
                        self.press_cursor = Some((cx, cy));
                    }
                    return;
                }
                // Panel widgets (viewer window, both screens).
                let regenerated = {
                    let viewer = &mut self.debug.viewer;
                    let lh = self.atlas.line_height();
                    let warn =
                        parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                    let checker = viewer.debug_mode == DebugMode::Checker;
                    // Content-specific left dock first.
                    if let Some(ViewContent::PlanetView) = content {
                        let lrects = &planet_left_plan(layout.left, lh, checker).rects;
                        if lrects.shader_button.contains(cx, cy) {
                            viewer.cycle_debug_mode();
                        }
                        if let Some(track) = lrects.density_track
                            && track.contains(cx, cy)
                        {
                            self.dragging_density = true;
                            viewer.density_slider.drag_to(track, cx);
                            viewer.sync_density_from_slider();
                        }
                        viewer.wire_cb.click(lrects.wire_box, cx, cy);
                        viewer.pent_cb.click(lrects.pent_box, cx, cy);
                        viewer.seam_cb.click(lrects.seam_box, cx, cy);
                        viewer.sync_toggles();
                    }
                    // Shared right dock.
                    let rrects =
                        &right_panel_plan(layout.panel, lh, warn, viewer.player.active).rects;
                    viewer.subdiv_field.click(rrects.subdiv_field, cx, cy);
                    viewer.radius_field.click(rrects.radius_field, cx, cy);
                    if rrects.subdiv_track.contains(cx, cy) {
                        self.dragging_slider = true;
                        viewer.subdiv_slider.drag_to(rrects.subdiv_track, cx);
                        viewer.sync_field_from_slider();
                    }
                    viewer.can_regenerate()
                        && rrects.regen_button.contains(cx, cy)
                        && viewer.regenerate().is_ok()
                };
                if regenerated {
                    self.refresh_mesh();
                }
                // Global camera presets: 2×2 VIEW grid retargets the free
                // orbit camera (same as G/T/B/R) — planet content only.
                // Grid order is [[Top, Bot], [Right, Persp]] matching
                // `GlobalPreset::ALL`.
                let preset = if content == Some(ViewContent::PlanetView) {
                    let lh = self.atlas.line_height();
                    let checker = self.debug.viewer.debug_mode == DebugMode::Checker;
                    planet_left_plan(layout.left, lh, checker)
                        .rects
                        .preset_grid
                        .iter()
                        .flatten()
                        .position(|rect| rect.contains(cx, cy))
                        .and_then(GlobalPreset::from_index)
                } else {
                    None
                };
                if let Some(preset) = preset {
                    let radius = self.debug.viewer.radius;
                    snap_global_camera(&mut self.camera, preset, radius);
                }
                // Panel clicks and rebuilds change what sits under the
                // cursor — refresh the hover.
                self.update_hover();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // A staged universe load is modal: wheel input is
                // ignored while it runs (no cancel in v1).
                if self.debug.loading.is_some() {
                    return;
                }
                let in_viewport = self.main.as_ref().is_some_and(|ctx| {
                    let (w, h) = ctx.size();
                    ctx.last_cursor.is_some_and(|(cx, cy)| {
                        app_layout(self.debug.chrome, w, h)
                            .viewport
                            .contains(cx, cy)
                    })
                });
                if in_viewport {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                    };
                    let content = self.debug.screen_content();
                    if matches!(self.debug.screen, Screen::GameDemo) {
                        // Vista intro (CVI-004): wheel is ignored while
                        // the vista owns the camera (no pace/zoom fight
                        // mid-dive).
                        if self.debug.cosmic.vista_active() {
                            return;
                        }
                        if self.shift_held {
                            // Cruise pace: wheel-up (positive scroll)
                            // tightens the pace (faster), wheel-down
                            // loosens it — one notch per wheel direction
                            // (wheel alone still zooms the camera below).
                            let factor = if scroll > 0.0 {
                                1.0 / CRUISE_SPEED_NOTCH
                            } else {
                                CRUISE_SPEED_NOTCH
                            };
                            let t = self.debug.cosmic.cruise_speed_by(factor);
                            self.debug
                                .fx
                                .notify(format!("Cruise pace {t:.1}s per scale length"));
                        } else {
                            // Cosmic zoom: wheel-up (positive scroll) moves
                            // the player camera closer. Checked before player
                            // mode for the same reason as drag-steer.
                            let factor = (1.0 - 0.12 * scroll).max(0.05);
                            self.debug.cosmic.camera.zoom(factor);
                        }
                    } else if content == Some(ViewContent::GalaxyMap) {
                        // Log zoom on the map: wheel-up (positive scroll)
                        // shrinks the camera distance.
                        let factor = (1.0 - 0.12 * scroll).max(0.05);
                        self.debug.galaxy.camera.zoom_by(factor);
                    } else if content == Some(ViewContent::SystemMap) {
                        let factor = (1.0 - 0.12 * scroll).max(0.05);
                        self.debug.system.camera.zoom_by(factor);
                    } else if content == Some(ViewContent::CosmicWeb) {
                        if self.shift_held {
                            // Slab scroll (`cosmic-depth-window` FR3):
                            // Shift+wheel steps the slice depth by T/4
                            // per notch when slab mode is on; plain
                            // wheel keeps the orbit zoom below.
                            let inspector = &mut self.debug.cosmic_inspector;
                            if inspector.slab.on {
                                let notches = if scroll > 0.0 { 1 } else { -1 };
                                inspector.slab.scroll(notches, 2600.0);
                                self.debug.fx.notify(format!(
                                    "slab {} Mpc @ {:.0} Mpc",
                                    inspector.slab.thickness_mpc, inspector.slab.center_mpc
                                ));
                            } else {
                                // Inspector log-zoom (demo zoom handled above).
                                let factor = (1.0 - 0.12 * scroll).max(0.05);
                                self.debug.cosmic_inspector.camera.zoom_by(factor);
                            }
                        } else {
                            // Inspector log-zoom (demo zoom handled above).
                            let factor = (1.0 - 0.12 * scroll).max(0.05);
                            self.debug.cosmic_inspector.camera.zoom_by(factor);
                        }
                    } else if self.debug.viewer.player.active {
                        self.debug.viewer.player.zoom_camera(scroll);
                    } else {
                        self.camera.zoom(scroll);
                    }
                    self.update_hover();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state,
                        text,
                        ..
                    },
                ..
            } => {
                // A staged universe load is modal: every key is
                // ignored while it runs (no cancel in v1).
                if self.debug.loading.is_some() {
                    return;
                }
                // Player movement tracks press AND release so keys never
                // stick; focused text fields keep every keystroke instead.
                if let PhysicalKey::Code(code) = physical_key
                    && matches!(
                        code,
                        KeyCode::KeyW
                            | KeyCode::KeyA
                            | KeyCode::KeyS
                            | KeyCode::KeyD
                            | KeyCode::ArrowUp
                            | KeyCode::ArrowDown
                            | KeyCode::ArrowLeft
                            | KeyCode::ArrowRight
                    )
                {
                    let viewer = &mut self.debug.viewer;
                    let fields_free = !viewer.subdiv_field.focused
                        && !viewer.radius_field.focused
                        && !self.debug.settings.seed_field.focused;
                    // Cosmic demo: WASD/arrows are cruise intent (the tab
                    // has no text fields; releases always clear).
                    if matches!(self.debug.screen, Screen::GameDemo) {
                        if fields_free || state == ElementState::Released {
                            let pressed = state == ElementState::Pressed;
                            let held = &mut self.debug.cosmic.held;
                            match code {
                                KeyCode::KeyW | KeyCode::ArrowUp => held.fwd = pressed,
                                KeyCode::KeyS | KeyCode::ArrowDown => held.back = pressed,
                                KeyCode::KeyA | KeyCode::ArrowLeft => held.left = pressed,
                                KeyCode::KeyD | KeyCode::ArrowRight => held.right = pressed,
                                _ => {}
                            }
                        }
                        return;
                    }
                    if viewer.player.active && fields_free {
                        let pressed = state == ElementState::Pressed;
                        let mut keys = viewer.player.keys();
                        match code {
                            KeyCode::KeyW | KeyCode::ArrowUp => keys.north = pressed,
                            KeyCode::KeyS | KeyCode::ArrowDown => keys.south = pressed,
                            KeyCode::KeyA | KeyCode::ArrowLeft => keys.west = pressed,
                            KeyCode::KeyD | KeyCode::ArrowRight => keys.east = pressed,
                            _ => {}
                        }
                        viewer.player.set_keys(keys);
                        return;
                    }
                    if state == ElementState::Released {
                        // Not driving (player off or typing): still clear
                        // a possibly stuck flag, then swallow the release
                        // (releases had no handling before either).
                        let viewer = &mut self.debug.viewer;
                        let mut keys = viewer.player.keys();
                        match code {
                            KeyCode::KeyW | KeyCode::ArrowUp => keys.north = false,
                            KeyCode::KeyS | KeyCode::ArrowDown => keys.south = false,
                            KeyCode::KeyA | KeyCode::ArrowLeft => keys.west = false,
                            KeyCode::KeyD | KeyCode::ArrowRight => keys.east = false,
                            _ => {}
                        }
                        viewer.player.set_keys(keys);
                        return;
                    }
                }
                if state != ElementState::Pressed {
                    return;
                }
                match physical_key {
                    // Esc unwinds UI focus (dropdown → widget); it never
                    // quits — closing the window exits. On the demo tab
                    // it also skips the vista intro (CVI-004).
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.debug.esc_unwind();
                        if matches!(self.debug.screen, Screen::GameDemo) {
                            self.debug.cosmic.skip_vista();
                        }
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        // Enter confirms the focused field: the Settings
                        // seed field starts the staged load (or surfaces
                        // the miss), planet fields just unfocus.
                        if self.debug.settings.seed_field.focused {
                            match self.debug.settings.seed_field.text.parse::<u64>() {
                                Ok(seed) => self.begin_load(seed, LoadSource::Panel),
                                Err(_) => self.debug.fx.notify(format!(
                                    "Invalid seed '{}'",
                                    self.debug.settings.seed_field.text
                                )),
                            }
                            self.debug.settings.seed_field.focused = false;
                        } else {
                            let viewer = &mut self.debug.viewer;
                            viewer.subdiv_field.focused = false;
                            viewer.radius_field.focused = false;
                        }
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        let viewer = &mut self.debug.viewer;
                        viewer.subdiv_field.backspace();
                        viewer.radius_field.backspace();
                        viewer.sync_slider_from_field();
                        self.debug.settings.seed_field.backspace();
                    }
                    PhysicalKey::Code(KeyCode::F1) => {
                        self.debug.select_top_by_fkey(1);
                    }
                    PhysicalKey::Code(KeyCode::F2) => {
                        self.debug.select_top_by_fkey(2);
                    }
                    PhysicalKey::Code(KeyCode::F3) => {
                        self.debug.select_top_by_fkey(3);
                    }
                    PhysicalKey::Code(KeyCode::Backquote) => {
                        self.debug.toggle_widget();
                    }
                    PhysicalKey::Code(KeyCode::F6) => {
                        self.debug.select_widget_tab_by_fkey(6);
                    }
                    PhysicalKey::Code(KeyCode::F7) => {
                        self.debug.select_widget_tab_by_fkey(7);
                    }
                    PhysicalKey::Code(KeyCode::F8) => {
                        self.debug.select_widget_tab_by_fkey(8);
                    }
                    PhysicalKey::Code(KeyCode::F9) => {
                        self.debug.toggle_left_dock();
                    }
                    PhysicalKey::Code(KeyCode::F10) => {
                        self.debug.toggle_right_dock();
                    }
                    PhysicalKey::Code(KeyCode::F5) => {
                        // Twilight stage demo (exposure-tone-mapping
                        // DoD-2): cycle the sky-luminance key; the
                        // Planet-View sky fades per stage.
                        self.twilight_stage = (self.twilight_stage + 1) % 4;
                        let (_, name) = twilight_key(self.twilight_stage);
                        self.debug.fx.notify(format!("Twilight {name}"));
                    }
                    PhysicalKey::Code(KeyCode::F12) => {
                        // Windowed PNG capture
                        // (`cosmic-capture-harness`, exploration only —
                        // DoD evidence comes from `--capture` presets).
                        if self.debug.screen_content() == Some(ViewContent::CosmicWeb) {
                            self.pending_capture = true;
                        } else {
                            self.debug.fx.notify(
                                "F12 captures a cosmic view — switch to Game Demo or Cosmic Web"
                                    .to_owned(),
                            );
                        }
                    }
                    PhysicalKey::Code(
                        KeyCode::Digit0
                        | KeyCode::Digit1
                        | KeyCode::Digit2
                        | KeyCode::Digit3
                        | KeyCode::Digit4
                        | KeyCode::Digit5
                        | KeyCode::Digit6
                        | KeyCode::Digit7
                        | KeyCode::Digit8
                        | KeyCode::Digit9,
                    ) => {
                        // Open dropdown captures digits for dimension
                        // select; otherwise digits are planet-content
                        // debug-mode selects (fields unfocused only).
                        if self.debug.dropdown_open {
                            let d = match physical_key {
                                PhysicalKey::Code(KeyCode::Digit0) => 0,
                                PhysicalKey::Code(KeyCode::Digit1) => 1,
                                PhysicalKey::Code(KeyCode::Digit2) => 2,
                                PhysicalKey::Code(KeyCode::Digit3) => 3,
                                PhysicalKey::Code(KeyCode::Digit4) => 4,
                                PhysicalKey::Code(KeyCode::Digit5) => 5,
                                PhysicalKey::Code(KeyCode::Digit6) => 6,
                                PhysicalKey::Code(KeyCode::Digit7) => 7,
                                PhysicalKey::Code(KeyCode::Digit8) => 8,
                                _ => 9,
                            };
                            self.debug.select_dimension_by_digit(d);
                            return;
                        }
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                            && self.debug.screen_content() == Some(ViewContent::PlanetView)
                        {
                            let i = match physical_key {
                                PhysicalKey::Code(KeyCode::Digit1) => 0,
                                PhysicalKey::Code(KeyCode::Digit2) => 1,
                                PhysicalKey::Code(KeyCode::Digit3) => 2,
                                PhysicalKey::Code(KeyCode::Digit4) => 3,
                                PhysicalKey::Code(KeyCode::Digit5) => 4,
                                PhysicalKey::Code(KeyCode::Digit6) => 5,
                                _ => return,
                            };
                            if let Some(&mode) = DebugMode::ALL.get(i) {
                                self.debug.viewer.debug_mode = mode;
                            }
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyU) => {
                        // Player-mode toggle — but never steal keystrokes
                        // from focused fields (`u` is printable input
                        // there).
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                        {
                            self.debug.viewer.player.toggle();
                            self.update_hover();
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyP) => {
                        // Camera cycle: the cosmic camera on the demo tab,
                        // else the player camera (active player only).
                        // A focused field keeps the keystroke instead.
                        // Vista intro (CVI-004): P is ignored while the
                        // vista owns the camera (no mode fight).
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                        {
                            if matches!(self.debug.screen, Screen::GameDemo) {
                                if !self.debug.cosmic.vista_active() {
                                    self.debug.cosmic.camera.cycle();
                                }
                            } else if viewer.player.active {
                                self.debug.viewer.player.cycle_camera();
                            }
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        // R re-rolls the seed with galaxy content; the
                        // demo tab re-rolls the cosmic web; planet
                        // content keeps R = Right preset.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        if fields_free {
                            let content = self.debug.screen_content();
                            if matches!(self.debug.screen, Screen::GameDemo) {
                                let seed = self.debug.cosmic.seed + 1;
                                self.reseed_cosmic(seed);
                            } else if content == Some(ViewContent::GalaxyMap) {
                                self.begin_load(self.debug.galaxy.seed + 1, LoadSource::Panel);
                            } else if content == Some(ViewContent::PlanetView) {
                                let radius = self.debug.viewer.radius;
                                snap_global_camera(&mut self.camera, GlobalPreset::Right, radius);
                                self.update_hover();
                            }
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyS) => {
                        // Inspector slab toggle (`cosmic-depth-window`
                        // FR3): Cosmic Web tab only, fields keep the
                        // keystroke. (GameDemo S is thrust — handled
                        // above and returned early.)
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                            && self.debug.screen_content() == Some(ViewContent::CosmicWeb)
                            && !matches!(self.debug.screen, Screen::GameDemo)
                        {
                            self.toggle_cosmic_slab();
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::BracketLeft) => {
                        // Slab thinner (`cosmic-depth-window` FR3):
                        // Cosmic Web tab, slab on.
                        if self.debug.screen_content() == Some(ViewContent::CosmicWeb)
                            && !matches!(self.debug.screen, Screen::GameDemo)
                        {
                            let inspector = &mut self.debug.cosmic_inspector;
                            if inspector.slab.on {
                                inspector
                                    .slab
                                    .set_thickness(inspector.slab.thickness_mpc - 10.0);
                                self.debug.fx.notify(format!(
                                    "slab {} Mpc @ {:.0} Mpc",
                                    inspector.slab.thickness_mpc, inspector.slab.center_mpc
                                ));
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::BracketRight) => {
                        // Slab thicker (`cosmic-depth-window` FR3).
                        if self.debug.screen_content() == Some(ViewContent::CosmicWeb)
                            && !matches!(self.debug.screen, Screen::GameDemo)
                        {
                            let inspector = &mut self.debug.cosmic_inspector;
                            if inspector.slab.on {
                                inspector
                                    .slab
                                    .set_thickness(inspector.slab.thickness_mpc + 10.0);
                                self.debug.fx.notify(format!(
                                    "slab {} Mpc @ {:.0} Mpc",
                                    inspector.slab.thickness_mpc, inspector.slab.center_mpc
                                ));
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyV) => {
                        // Vista replay (`cosmic-vista-intro` FR4): the
                        // demo tab restarts Hold → Dive → Done from the
                        // opening pose. A focused field keeps the
                        // keystroke instead (no new binding elsewhere).
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                        {
                            if matches!(self.debug.screen, Screen::GameDemo) {
                                self.debug.cosmic.replay_vista();
                            }
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyE) => {
                        // Drill down: armed galaxy star → SystemMap.
                        // Instant faded map navigation (the timed transit
                        // wraps arrivals, not drill-down).
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        // Cosmic demo first: E toggles fly-to on the
                        // click-selected node (shared with Controls).
                        // Vista intro (CVI-004): E skips first (the
                        // gate in `select_node_at` means there is never
                        // a mid-vista target to engage).
                        if fields_free && matches!(self.debug.screen, Screen::GameDemo) {
                            self.debug.cosmic.skip_vista();
                            self.toggle_fly_to();
                        } else if fields_free
                            && self.debug.screen_content() == Some(ViewContent::GalaxyMap)
                            && let Some(i) = self.debug.galaxy.selected
                        {
                            self.debug.journey.update(JourneyEvent::SelectStar(i));
                            let _fx = self.debug.journey.update(JourneyEvent::EnterSystem);
                            // Fade/prefetch effects surface in the transit
                            // UI; the screen switch is the visible half of
                            // the transition today.
                            if self.debug.journey.active_layer() == Layer::System {
                                let seed = self.debug.galaxy.seed;
                                let star = self.debug.galaxy.galaxy.stars[i as usize].clone();
                                self.debug.system.load(seed, &star);
                                self.refresh_system();
                                self.debug
                                    .select_screen(Screen::Dimensions(WaypointId::SolarSystem));
                                self.debug.fx.trigger_fade();
                                self.debug.fx.notify(format!(
                                    "System star {i} · {} planets",
                                    self.debug.system.system.planets.len()
                                ));
                            }
                        } else if fields_free
                            && self.debug.screen_content() == Some(ViewContent::SystemMap)
                        {
                            // Shared with the Controls row button.
                            self.travel_begin();
                        } else if !fields_free && let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyQ) => {
                        // Back one journey layer (map content only). An
                        // underway transit cancels first — leaving
                        // abandons the hop. Shared with Controls.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        if fields_free
                            && matches!(
                                self.debug.screen_content(),
                                Some(ViewContent::GalaxyMap | ViewContent::SystemMap)
                            )
                        {
                            self.ascend_layer();
                        } else if !fields_free && let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyF) => {
                        // L4 planet-focus toggle (system content only) +
                        // journey mirror. Shared with Controls.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        if fields_free
                            && self.debug.screen_content() == Some(ViewContent::SystemMap)
                        {
                            self.focus_toggle();
                        } else if !fields_free && let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyT) => {
                        // Travel offer arm/withdraw + transit cancel
                        // (system content only). Shared with Controls.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        if fields_free
                            && self.debug.screen_content() == Some(ViewContent::SystemMap)
                        {
                            self.travel_offer_toggle();
                        } else if fields_free
                            && self.debug.screen_content() == Some(ViewContent::PlanetView)
                        {
                            // Planet content keeps T = Top preset.
                            let radius = self.debug.viewer.radius;
                            snap_global_camera(&mut self.camera, GlobalPreset::Top, radius);
                            self.update_hover();
                        } else if !fields_free && let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::Home) => {
                        // Top-down snap toggle: map content only. First
                        // press frames the classic 2D read (north up,
                        // east right); second press restores the
                        // previous tilt.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.settings.seed_field.focused
                        };
                        if fields_free {
                            let content = self.debug.screen_content();
                            if content == Some(ViewContent::GalaxyMap) {
                                self.debug.galaxy.camera.toggle_top_down();
                                self.debug.fx.notify("Top-down · [Home] tilt".to_owned());
                            } else if content == Some(ViewContent::SystemMap) {
                                self.debug.system.camera.toggle_top_down();
                                self.debug.fx.notify("Top-down · [Home] tilt".to_owned());
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyG | KeyCode::KeyB) => {
                        // Global camera presets (same as the VIEW buttons).
                        // T and R have their own arms above.
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.settings.seed_field.focused
                            && self.debug.screen_content() == Some(ViewContent::PlanetView)
                        {
                            let preset = match physical_key {
                                PhysicalKey::Code(KeyCode::KeyB) => GlobalPreset::Bottom,
                                // G (and anything else) = Perspective; T
                                // and R have their own arms above.
                                _ => GlobalPreset::Perspective,
                            };
                            let radius = self.debug.viewer.radius;
                            snap_global_camera(&mut self.camera, preset, radius);
                            self.update_hover();
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    _ => {
                        // The focused seed field eats keystrokes first;
                        // otherwise printable input goes to the planet
                        // fields (their insert guards on focus).
                        if self.debug.settings.seed_field.focused {
                            if let Some(text) = text {
                                for ch in text.chars() {
                                    self.debug.settings.seed_field.insert_char(ch);
                                }
                            }
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.tick_player();
                self.draw_main();
            }
            _ => {}
        }
    }

    /// Fire a Settings Controls row action (parity: the row is the
    /// button for the action's key). Discrete actions perform
    /// immediately; walk actions arm hold-to-press until mouse
    /// release (see the `MouseInput` release path).
    fn fire_action(&mut self, action: Action) {
        match action {
            Action::ToggleLeftDock => self.debug.toggle_left_dock(),
            Action::ToggleRightDock => self.debug.toggle_right_dock(),
            Action::ToggleDevWidget => self.debug.toggle_widget(),
            Action::ShowWidgetFps => self.debug.select_widget_tab(WidgetTab::Fps),
            Action::ShowWidgetConsole => self.debug.select_widget_tab(WidgetTab::Console),
            Action::ShowWidgetInspector => self.debug.select_widget_tab(WidgetTab::Inspector),
            Action::UnwindUi => {
                self.debug.esc_unwind();
            }
            Action::CaptureScreenshot => {
                // Settings Controls parity for F12 (same gate as the key).
                if self.debug.screen_content() == Some(ViewContent::CosmicWeb) {
                    self.pending_capture = true;
                } else {
                    self.debug.fx.notify(
                        "F12 captures a cosmic view — switch to Game Demo or Cosmic Web".to_owned(),
                    );
                }
            }
            Action::NavGameDemo => self.debug.select_screen(Screen::GameDemo),
            Action::NavDimensions => self.debug.toggle_dropdown(),
            Action::NavSettings => self.debug.select_screen(Screen::Settings),
            Action::SelectDimension(i) => {
                if let Some(&waypoint) = DROPDOWN_ORDER.get(i) {
                    self.debug.select_screen(Screen::Dimensions(waypoint));
                }
            }
            Action::WalkNorth => self.hold_walk(WalkDir::North),
            Action::WalkSouth => self.hold_walk(WalkDir::South),
            Action::WalkWest => self.hold_walk(WalkDir::West),
            Action::WalkEast => self.hold_walk(WalkDir::East),
            Action::PlayerToggle => {
                self.debug.viewer.player.toggle();
                self.update_hover();
            }
            Action::CameraCycle => {
                if matches!(self.debug.screen, Screen::GameDemo) {
                    // Vista intro (CVI-004): no mode fight while the
                    // vista owns the camera (same guard as `P`).
                    if !self.debug.cosmic.vista_active() {
                        self.debug.cosmic.camera.cycle();
                    }
                } else if self.debug.viewer.player.active {
                    self.debug.viewer.player.cycle_camera();
                }
            }
            Action::VistaReplay => {
                // Controls-row parity for `V` (demo tab only).
                if matches!(self.debug.screen, Screen::GameDemo) {
                    self.debug.cosmic.replay_vista();
                }
            }
            Action::PresetPerspective => {
                self.snap_preset(GlobalPreset::Perspective);
            }
            Action::PresetTop => self.snap_preset(GlobalPreset::Top),
            Action::PresetBottom => self.snap_preset(GlobalPreset::Bottom),
            Action::PresetRight => self.snap_preset(GlobalPreset::Right),
            Action::RerollSeed => {
                if matches!(self.debug.screen, Screen::GameDemo) {
                    let seed = self.debug.cosmic.seed + 1;
                    self.reseed_cosmic(seed);
                } else if self.debug.screen_content() == Some(ViewContent::GalaxyMap) {
                    self.begin_load(self.debug.galaxy.seed + 1, LoadSource::Panel);
                }
            }
            Action::TopDownSnap => {
                let content = self.debug.screen_content();
                if content == Some(ViewContent::GalaxyMap) {
                    self.debug.galaxy.camera.toggle_top_down();
                } else if content == Some(ViewContent::SystemMap) {
                    self.debug.system.camera.toggle_top_down();
                } else if content == Some(ViewContent::CosmicWeb)
                    && !matches!(self.debug.screen, Screen::GameDemo)
                {
                    self.debug.cosmic_inspector.camera.toggle_top_down();
                }
            }
            Action::TwilightCycle => {
                self.twilight_stage = (self.twilight_stage + 1) % 4;
            }
            Action::SlabToggle => {
                // Settings Controls parity for S (same tab gate).
                if self.debug.screen_content() == Some(ViewContent::CosmicWeb)
                    && !matches!(self.debug.screen, Screen::GameDemo)
                {
                    self.toggle_cosmic_slab();
                }
            }
            Action::SlabThinner | Action::SlabThicker => {
                // Settings Controls parity for [/] (same tab gate).
                if self.debug.screen_content() == Some(ViewContent::CosmicWeb)
                    && !matches!(self.debug.screen, Screen::GameDemo)
                {
                    let inspector = &mut self.debug.cosmic_inspector;
                    if inspector.slab.on {
                        let delta = if matches!(action, Action::SlabThinner) {
                            -10.0
                        } else {
                            10.0
                        };
                        inspector
                            .slab
                            .set_thickness(inspector.slab.thickness_mpc + delta);
                        self.debug.fx.notify(format!(
                            "slab {} Mpc @ {:.0} Mpc",
                            inspector.slab.thickness_mpc, inspector.slab.center_mpc
                        ));
                    }
                }
            }
            Action::ShaderMode(i) => {
                if let Some(&mode) = DebugMode::ALL.get(i) {
                    self.debug.viewer.debug_mode = mode;
                }
            }
            Action::TravelOffer => self.travel_offer_toggle(),
            Action::TravelBegin => self.travel_begin(),
            Action::FlyToToggle => self.toggle_fly_to(),
            Action::CruiseSpeed => {
                // Controls-row parity for the `Shift`+wheel pace action:
                // a click steps one notch faster on the demo tab.
                if matches!(self.debug.screen, Screen::GameDemo) {
                    let t = self.debug.cosmic.cruise_speed_by(1.0 / CRUISE_SPEED_NOTCH);
                    self.debug
                        .fx
                        .notify(format!("Cruise pace {t:.1}s per scale length"));
                }
            }
            Action::AscendLayer => self.ascend_layer(),
            Action::FocusToggle => self.focus_toggle(),
            Action::ConfirmField => self.confirm_focused_field(),
        }
    }

    /// Arm a mouse-held walk direction (player must be active; the
    /// release path clears it). On the demo tab the same parity rows
    /// drive cosmic cruise instead.
    fn hold_walk(&mut self, dir: WalkDir) {
        if matches!(self.debug.screen, Screen::GameDemo) {
            self.mouse_walk = Some(dir);
            let held = &mut self.debug.cosmic.held;
            match dir {
                WalkDir::North => held.fwd = true,
                WalkDir::South => held.back = true,
                WalkDir::West => held.left = true,
                WalkDir::East => held.right = true,
            }
            return;
        }
        if !self.debug.viewer.player.active {
            return;
        }
        self.mouse_walk = Some(dir);
        let viewer = &mut self.debug.viewer;
        let mut keys = viewer.player.keys();
        match dir {
            WalkDir::North => keys.north = true,
            WalkDir::South => keys.south = true,
            WalkDir::West => keys.west = true,
            WalkDir::East => keys.east = true,
        }
        viewer.player.set_keys(keys);
    }

    /// Snap the free orbit camera (planet content only).
    fn snap_preset(&mut self, preset: GlobalPreset) {
        if self.debug.screen_content() == Some(ViewContent::PlanetView) {
            let radius = self.debug.viewer.radius;
            snap_global_camera(&mut self.camera, preset, radius);
            self.update_hover();
        }
    }

    /// Confirm the focused field (Enter parity): the Settings seed
    /// field starts the staged load, planet fields just unfocus.
    fn confirm_focused_field(&mut self) {
        if self.debug.settings.seed_field.focused {
            match self.debug.settings.seed_field.text.parse::<u64>() {
                Ok(seed) => self.begin_load(seed, LoadSource::Panel),
                Err(_) => self.debug.fx.notify(format!(
                    "Invalid seed '{}'",
                    self.debug.settings.seed_field.text
                )),
            }
            self.debug.settings.seed_field.focused = false;
        } else {
            let viewer = &mut self.debug.viewer;
            viewer.subdiv_field.focused = false;
            viewer.radius_field.focused = false;
        }
    }

    /// Travel offer arm/withdraw + transit cancel (system content).
    fn travel_offer_toggle(&mut self) {
        if self.debug.screen_content() != Some(ViewContent::SystemMap) {
            return;
        }
        let had_transit = self.debug.system.transit.is_some();
        let selected = self.debug.system.selected;
        self.debug.system.travel_offer = match (self.debug.system.travel_offer, selected) {
            (Some(_), _) => None,
            (None, Some(i)) => Some(i),
            (None, None) => None,
        };
        match (self.debug.system.travel_offer, had_transit) {
            (Some(i), _) => self
                .debug
                .fx
                .notify(format!("Travel offer: planet {i} · [E] begin transit")),
            (None, true) => {
                self.debug.system.transit = None;
                self.transit_acc = 0.0;
                self.debug.fx.notify("Transit cancelled".to_owned());
            }
            (None, false) => self.debug.fx.notify("Travel offer withdrawn".to_owned()),
        }
    }

    /// Begin the transit countdown on the armed travel offer.
    fn travel_begin(&mut self) {
        if self.debug.screen_content() != Some(ViewContent::SystemMap) {
            return;
        }
        if self.debug.journey.active_layer() != Layer::System {
            self.debug
                .fx
                .notify("Journey out of sync — re-enter via galaxy [E]".to_owned());
            return;
        }
        if let Some(i) = self.debug.system.travel_offer {
            if self.debug.system.transit.is_none() {
                let star = self.debug.system.system.id.star_index();
                self.debug.system.transit = Some(Transit::begin(star, i));
                self.transit_acc = 0.0;
                let orbit = self
                    .debug
                    .system
                    .system
                    .planets
                    .get(i as usize)
                    .map(|p| p.orbit_radius_au)
                    .unwrap_or(0.0);
                let cost = plan_cost(orbit);
                self.debug.fx.notify(format!(
                    "Transit underway → planet {i} · fuel {:.1} (deferred)",
                    cost.fuel
                ));
            }
        } else {
            self.debug
                .fx
                .notify("Arm a travel offer first [T]".to_owned());
        }
    }

    /// Back one journey layer (map content); an underway transit
    /// cancels first.
    fn ascend_layer(&mut self) {
        if !matches!(
            self.debug.screen_content(),
            Some(ViewContent::GalaxyMap | ViewContent::SystemMap)
        ) {
            return;
        }
        if self.debug.system.transit.is_some() {
            self.debug.system.transit = None;
            self.transit_acc = 0.0;
            self.debug.fx.notify("Transit cancelled".to_owned());
        }
        let _fx = self.debug.journey.update(JourneyEvent::Ascend);
        if self.debug.journey.active_layer() == Layer::Galaxy {
            self.debug
                .select_screen(Screen::Dimensions(WaypointId::MilkyWay));
            self.debug.fx.trigger_fade();
            self.debug.fx.notify("Galaxy map".to_owned());
        }
    }

    /// L4 planet-focus toggle (system content) + journey mirror.
    fn focus_toggle(&mut self) {
        if self.debug.screen_content() != Some(ViewContent::SystemMap) {
            return;
        }
        self.debug.system.toggle_focus();
        let focus = self.debug.system.focus;
        self.debug.journey.update(JourneyEvent::FocusPlanet(focus));
        match focus {
            Some(i) => self
                .debug
                .fx
                .notify(format!("Focus planet {i} · [F] unfocus")),
            None => self.debug.fx.notify("Focus cleared".to_owned()),
        }
    }
}

impl ViewerApp {
    /// Advance the player simulation + chunk streaming (once per
    /// main-window frame, before drawing).
    fn tick_player(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.0, 0.25);
        self.last_frame = Some(now);
        self.tick_cosmic(dt);
        // Transit countdown: fixed-step accumulation of the
        // frame dt into sim ticks. Commit fires journey EnterOrbit at
        // duration; the arrival view lands bound to Earth.
        if self.debug.screen_content() == Some(ViewContent::SystemMap)
            && self.debug.system.transit.is_some()
        {
            self.transit_acc += dt;
            while self.transit_acc >= SIM_DT_SECS {
                self.transit_acc -= SIM_DT_SECS;
                if let Some(transit) = self.debug.system.transit.as_mut() {
                    transit.tick();
                }
            }
            if self
                .debug
                .system
                .transit
                .as_ref()
                .is_some_and(|transit| transit.ready())
            {
                let planet = self
                    .debug
                    .system
                    .transit
                    .as_ref()
                    .expect("checked above")
                    .planet_index();
                self.debug.system.transit = None;
                self.transit_acc = 0.0;
                self.debug.system.travel_offer = None;
                self.debug
                    .journey
                    .update(JourneyEvent::SelectPlanet(planet));
                let _fx = self.debug.journey.update(JourneyEvent::EnterOrbit);
                if self.debug.journey.active_layer() != Layer::Orbit {
                    self.debug
                        .fx
                        .notify("Arrival failed: journey left Orbit".to_owned());
                } else if let Some(arrival) = arrival_for(&self.debug.system.system, planet) {
                    // Bind the orbit view to the target descriptor —
                    // the viewer rebuilds at descriptor radius with the
                    // atmosphere tint; the mesh seed rides the held
                    // SeededPlanet into M2/M3.
                    let radius = self.debug.arrive(arrival);
                    self.refresh_mesh();
                    self.debug
                        .select_screen(Screen::Dimensions(WaypointId::Earth));
                    self.debug.fx.trigger_fade();
                    let bound = self.debug.viewer.arrival.as_ref().expect("just bound");
                    self.debug.fx.notify(format!(
                        "Arrived orbit · {:?} planet · R {:.1} km",
                        bound.planet_type, radius
                    ));
                } else {
                    // Stale planet index (system reloaded mid-flight):
                    // surface it, never panic.
                    self.debug.fx.notify(format!(
                        "Arrival failed: planet {planet} not in this system"
                    ));
                }
            }
        }
        if !self.debug.viewer.player.active {
            return;
        }
        let stream_due = self
            .last_stream_sync
            .is_none_or(|last| now - last >= Duration::from_millis(STREAM_SYNC_MS));
        if stream_due {
            let viewer = &self.debug.viewer;
            self.stream_desired =
                visible_hemisphere(&viewer.mesh, viewer.player.position().to_array());
            self.last_stream_sync = Some(now);
        }
        let desired = self.stream_desired.clone();
        self.debug.viewer.player.update(dt, &desired);
    }

    fn draw_main(&mut self) {
        let (win_w, win_h) = match self.main.as_ref() {
            Some(ctx) => ctx.size(),
            None => return,
        };
        if win_w < 1.0 || win_h < 1.0 {
            return;
        }
        // Staged universe load: one step per frame keeps the UI (and
        // the modal progress bar) alive; a finished plan closes out
        // with the field mirror + notify.
        let next = self.debug.loading.as_mut().and_then(LoadPlan::advance);
        if let Some(step) = next {
            let seed = self
                .debug
                .loading
                .as_ref()
                .expect("plan outlives its step")
                .seed;
            self.exec_load_step(seed, step);
        } else if let Some(plan) = self.debug.loading.take() {
            self.finish_load(plan);
        }
        {
            let ctx = self.main.as_mut().expect("main window must exist");
            ctx.previous_frame_end
                .as_mut()
                .expect("frame future")
                .cleanup_finished();
            if ctx.recreate_swapchain {
                let window_size = ctx.window.inner_size();
                let (new_swapchain, new_images) = ctx
                    .swapchain
                    .recreate(SwapchainCreateInfo {
                        image_extent: window_size.into(),
                        ..ctx.swapchain.create_info()
                    })
                    .expect("swapchain recreation must succeed");
                ctx.swapchain = new_swapchain;
                ctx.swapchain_images = new_images.clone();
                ctx.depth_view =
                    create_depth_view(&self.memory_allocator, ctx.swapchain.image_extent());
                ctx.framebuffers =
                    window_size_dependent_setup(&new_images, &ctx.render_pass, &ctx.depth_view);
                // HDR transients track the swapchain extent (the passes
                // and pipelines persist — only images/sets rebuild; the
                // volume texture persists across recreates).
                ctx.hdr = ctx.hdr_format.map(|format| {
                    Self::build_hdr_chain(
                        &self.memory_allocator,
                        &self.descriptor_set_allocator,
                        &self.post_sampler,
                        format,
                        ctx.swapchain.image_extent(),
                        &ctx.scene_pass,
                        &ctx.post_pass,
                        &ctx.pipelines,
                        &self
                            .veil_volume
                            .as_ref()
                            .expect("veil volume must upload at boot")
                            .1,
                    )
                });
                ctx.recreate_swapchain = false;
            }
        }

        let layout = app_layout(self.debug.chrome, win_w, win_h);
        let player_active = self.debug.viewer.player.active;
        let content = self.debug.screen_content();
        // Cosmic draw state, precomputed once per frame (player point
        // rebuild included): the LDR direct path and the HDR
        // scene/resolve path below share it, so both record identical
        // draws. `None` on non-cosmic tabs.
        let cosmic_frame =
            (content == Some(ViewContent::CosmicWeb)).then(|| self.cosmic_frame(layout.viewport));

        // Build frame UI (atlas insertions happen here) and sync the GPU
        // atlas before recording. The top bar + docks render inside the
        // content builders; overlay chrome (strip, corner, widget)
        // composes on top so it floats over every tab.
        let mut items = match self.debug.screen {
            Screen::GameDemo => {
                // v0.3.2 rebuild: the demo mounts only the cosmic player
                // scene (journey maps live on in their dimension tabs).
                let mut demo = UiItems::default();
                build_topbar(&mut demo, self.atlas.line_height(), &self.debug, layout);
                build_demo_ui(
                    &mut demo,
                    self.atlas.line_height(),
                    &mut self.debug,
                    layout.viewport,
                );
                demo
            }
            Screen::Dimensions(WaypointId::MilkyWay) => {
                build_galaxy_ui(&mut self.atlas, &self.debug, layout)
            }
            Screen::Dimensions(WaypointId::CosmicWeb) => {
                build_cosmic_web_ui(&mut self.atlas, &self.debug, layout)
            }
            Screen::Dimensions(WaypointId::SolarSystem) => {
                build_system_ui(&mut self.atlas, &self.debug, layout)
            }
            Screen::Dimensions(WaypointId::Earth) => {
                let sky_summary = self.sky.summary();
                build_planet_ui(&mut self.atlas, &self.debug, &sky_summary, layout)
            }
            Screen::Dimensions(waypoint) => {
                let mut items = UiItems::default();
                build_topbar(&mut items, self.atlas.line_height(), &self.debug, layout);
                build_placeholder_ui(
                    &mut items,
                    self.atlas.line_height(),
                    &self.debug,
                    waypoint,
                    layout.viewport,
                );
                items
            }
            Screen::Settings => {
                let mut items = UiItems::default();
                build_topbar(&mut items, self.atlas.line_height(), &self.debug, layout);
                build_settings_ui(&mut items, self.atlas.line_height(), &self.debug, layout);
                items
            }
        };
        // Overlay chrome over every tab (the corner strip rides
        // inside the top bar). The dropdown gets its own buffer, drawn
        // after all other UI — see `compose_overlay_ui`.
        let cursor = self.main.as_ref().and_then(|ctx| ctx.last_cursor);
        let mut drop_items = UiItems::default();
        compose_overlay_ui(
            &mut items,
            &mut drop_items,
            &mut self.atlas,
            &self.debug,
            layout,
            (win_w, win_h),
            cursor,
        );
        self.sync_atlas();
        let ui_verts = ui_items_to_vertices(&items, &mut self.atlas);
        let drop_verts = ui_items_to_vertices(&drop_items, &mut self.atlas);
        assert!(
            ui_verts.len() as u64 + drop_verts.len() as u64 <= MAX_UI_VERTS,
            "ui vertex budget exceeded"
        );
        // Fresh upload per frame (same pattern as the mesh uploads): the
        // frame's command buffer may still be in flight next frame, so a
        // persistent mapped buffer would hit access conflicts.
        let ui_buffer = Buffer::from_iter(
            self.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            ui_verts.iter().copied(),
        )
        .expect("ui vertex buffer upload must succeed");
        // No upload while the menu is closed (zero-vertex buffers
        // are not valid vertex sources).
        let drop_buffer = if drop_verts.is_empty() {
            None
        } else {
            Some(
                Buffer::from_iter(
                    self.memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::VERTEX_BUFFER,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    drop_verts.iter().copied(),
                )
                .expect("dropdown vertex buffer upload must succeed"),
            )
        };
        let drop_solid_count = (drop_items.solids.len() * 6) as u32;

        let ctx = self.main.as_mut().expect("main window must exist");
        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(ctx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    ctx.recreate_swapchain = true;
                    return;
                }
                Err(error) => panic!("swapchain acquire failed: {error}"),
            };
        if suboptimal {
            ctx.recreate_swapchain = true;
        }

        let mut builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .expect("command buffer builder must create");
        // Windowed capture (F12), first half: copy the acquired
        // swapchain image to a host buffer BEFORE the render pass
        // overwrites it. Encode + write happen after the flush below
        // (one-frame deferred — the loop never blocks more than one
        // frame for a capture).
        let capture_readback = if self.pending_capture {
            let extent = ctx.swapchain.image_extent();
            let buffer = Buffer::from_iter(
                self.memory_allocator.clone(),
                BufferCreateInfo {
                    usage: BufferUsage::TRANSFER_DST,
                    ..Default::default()
                },
                AllocationCreateInfo {
                    memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE
                        | MemoryTypeFilter::PREFER_HOST,
                    ..Default::default()
                },
                (0..extent[0] as usize * extent[1] as usize * 4).map(|_| 0u8),
            )
            .expect("capture readback buffer must create");
            let image = ctx
                .swapchain_images
                .get(image_index as usize)
                .expect("acquired swapchain image must exist")
                .clone();
            builder
                .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, buffer.clone()))
                .expect("capture readback copy must record");
            Some((buffer, extent))
        } else {
            None
        };
        // Orbit backdrop (UMAP-020): the arrival target's atmosphere
        // color, scaled to a near-black space read; the default tint
        // otherwise. Cosmic views clear to deep indigo instead
        // (update-2026-09-18-2328) so voids read as negative space
        // against the additive filaments. Descriptor palette straight
        // to the frame.
        let backdrop: [f32; 4] = if content == Some(ViewContent::CosmicWeb) {
            COSMIC_BACKDROP
        } else {
            self.debug
                .viewer
                .arrival
                .as_ref()
                .map(|arrival| {
                    let c = arrival.atmosphere.color;
                    [c[0] * 0.07, c[1] * 0.07, c[2] * 0.07 + 0.02, 1.0]
                })
                .unwrap_or([0.02, 0.03, 0.08, 1.0])
        };
        // HDR cosmic pre-pass (update-2026-09-18-2328): the scene and
        // bloom chain run offscreen before the main pass begins; the
        // views loop below only resolves into the swapchain image. LDR
        // bypass skips this block and draws direct-to-swapchain in the
        // loop instead. Recording lives in `record_cosmic_hdr_prepass`
        // (CAP-001 seam: the offscreen `--capture` path calls it too).
        if let Some(frame) = cosmic_frame.as_ref()
            && let Some(hdr) = ctx.hdr.as_ref()
        {
            let redshift = game_debug::cosmic_web::COSMIC_REDSHIFT_PER_MPC;
            // Per-surface grade (update-2026-09-19-1933): the
            // inspector's zoomed-out view stacks dozens of sprites/px
            // where the immersive demo stacks a few — one exposure
            // can't serve both.
            let glow_exposure = if frame.is_demo {
                COSMIC_DEMO_GLOW_EXPOSURE
            } else {
                COSMIC_MAP_GLOW_EXPOSURE
            };
            let splat_alpha_k = if frame.is_demo {
                SPLAT_ALPHA_K_DEMO
            } else {
                SPLAT_ALPHA_K_MAP
            };
            let bloom_params = MipBloomParams::for_tier(QualityTier::High);
            record_cosmic_hdr_prepass(
                &mut builder,
                &ctx.pipelines,
                hdr,
                frame,
                glow_exposure,
                redshift,
                &bloom_params,
                ctx.bloom_enabled,
                splat_alpha_k,
                &self.descriptor_set_allocator,
                &self.memory_allocator,
                &self.post_sampler,
            );
            // Gas-veil march (CGV-005/006): skipped in sprites mode.
            if matches!(
                self.veil_mode,
                game_debug::cosmic_veil::VeilMode::March { .. }
            ) {
                record_veil_march(&mut builder, &ctx.pipelines, hdr, &frame.march);
            }
        }
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some(backdrop.into()), Some(ClearValue::Depth(1.0))],
                    ..RenderPassBeginInfo::framebuffer(
                        ctx.framebuffers[image_index as usize].clone(),
                    )
                },
                SubpassBeginInfo {
                    contents: SubpassContents::Inline,
                    ..Default::default()
                },
            )
            .expect("render pass must begin");

        {
            // Single view: the active content fills the viewport (flat
            // tabs skip the 3D pass — `content` is `None` there). The
            // viewport transform clips output to the rect, so no
            // scissor state is needed.
            let views = [(layout.viewport, content)];
            let mode = self.debug.viewer.debug_mode.index() as f32;
            let density = self.debug.viewer.checker_density as f32;
            let highlight = if self.debug.viewer.pentagons {
                1.0
            } else {
                0.0
            };
            let seams_on = if self.debug.viewer.seams { 1.0 } else { 0.0 };
            let hover_cell = self
                .debug
                .viewer
                .hovered
                .map(|chunk| chunk.index() as f32)
                .unwrap_or(-1.0);
            let pin_cell = self
                .debug
                .viewer
                .pinned
                .map(|chunk| chunk.index() as f32)
                .unwrap_or(-1.0);
            for (vp, view) in views {
                if vp.w < 1.0 || vp.h < 1.0 {
                    continue;
                }
                let Some(view) = view else { continue };
                let viewport = Viewport {
                    offset: [vp.x, vp.y],
                    extent: [vp.w, vp.h],
                    depth_range: 0.0..=1.0,
                };
                if view == ViewContent::GalaxyMap {
                    // Galaxy map: one static point buffer (backdrop +
                    // impostors + stars); orbit/pan/zoom ride the
                    // view-projection push.
                    let galaxy = &self.debug.galaxy;
                    let mvp = galaxy.camera.view_proj(vp.w / vp.h).to_cols_array_2d();
                    let px_scale = galaxy.camera.px_scale(vp.h);
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(ctx.pipelines.map.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.map_vertices.clone())
                        .expect("vertex buffer must bind")
                        .push_constants(
                            ctx.pipelines.map.layout().clone(),
                            0,
                            MapPush {
                                mvp,
                                px_scale,
                                exposure: 1.0,
                            },
                        )
                        .expect("map push constants must upload");
                    // SAFETY: `vertex_count` equals the uploaded point
                    // count and the buffer holds exactly those vertices;
                    // no index buffer is bound for this `PointList` draw.
                    unsafe { builder.draw(self.map_vertices.len() as u32, 1, 0, 0) }
                        .expect("map draw must record");
                } else if view == ViewContent::CosmicWeb {
                    // Cosmic player scene (update-2026-09-18-2328): HDR
                    // mode resolves the pre-recorded scene + bloom over
                    // the full window; LDR bypass draws glow + splats
                    // direct-to-swapchain. Both arms share the
                    // precomputed frame, so the draws are identical.
                    // The demo tab renders the player-immersive view;
                    // the Cosmic Web tab renders through the inspector
                    // camera (fixed-center buffers + live player point).
                    // Recording lives in `record_cosmic_view_arm`
                    // (CAP-001 seam: the offscreen path calls it too).
                    let frame = cosmic_frame
                        .as_ref()
                        .expect("cosmic view must precompute its frame");
                    let redshift = game_debug::cosmic_web::COSMIC_REDSHIFT_PER_MPC;
                    let glow_exposure = if frame.is_demo {
                        COSMIC_DEMO_GLOW_EXPOSURE
                    } else {
                        COSMIC_MAP_GLOW_EXPOSURE
                    };
                    let (resolve_exposure, bloom_intensity) = if frame.is_demo {
                        (COSMIC_DEMO_EXPOSURE, COSMIC_DEMO_BLOOM_INTENSITY)
                    } else {
                        (COSMIC_MAP_EXPOSURE, COSMIC_MAP_BLOOM_INTENSITY)
                    };
                    let splat_alpha_k = if frame.is_demo {
                        SPLAT_ALPHA_K_DEMO
                    } else {
                        SPLAT_ALPHA_K_MAP
                    };
                    record_cosmic_view_arm(
                        &mut builder,
                        &ctx.pipelines,
                        ctx.hdr.as_ref(),
                        ctx.bloom_enabled,
                        frame,
                        viewport.clone(),
                        [win_w, win_h],
                        glow_exposure,
                        resolve_exposure,
                        bloom_intensity,
                        redshift,
                        splat_alpha_k,
                        VEIL_MARCH_RESOLVE_GAIN,
                        self.cosmic_tab_player.clone(),
                        &self.descriptor_set_allocator,
                        &self.memory_allocator,
                        &self.post_sampler,
                    );
                } else if view == ViewContent::SystemMap {
                    // System map: orbit rings through the line pipeline,
                    // star + planets through the map point pipeline — both
                    // under the perspective view-projection, rings first.
                    let system = &self.debug.system;
                    let mvp = system.camera.view_proj(vp.w / vp.h).to_cols_array_2d();
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(ctx.pipelines.line.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.system_lines.clone())
                        .expect("vertex buffer must bind")
                        .push_constants(
                            ctx.pipelines.line.layout().clone(),
                            0,
                            LinePush { mvp, inflate: 0.0 },
                        )
                        .expect("line push constants must upload");
                    // SAFETY: same PointList-style contract as the
                    // wireframe draw — buffer holds exactly the uploaded
                    // ring segments, no index buffer bound.
                    unsafe { builder.draw(self.system_lines.len() as u32, 1, 0, 0) }
                        .expect("system rings draw must record");
                    let px_scale = system.camera.px_scale(vp.h);
                    builder
                        .bind_pipeline_graphics(ctx.pipelines.map.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.system_points.clone())
                        .expect("vertex buffer must bind")
                        .push_constants(
                            ctx.pipelines.map.layout().clone(),
                            0,
                            MapPush {
                                mvp,
                                px_scale,
                                exposure: 1.0,
                            },
                        )
                        .expect("map push constants must upload");
                    // SAFETY: same contract as the galaxy map draw.
                    unsafe { builder.draw(self.system_points.len() as u32, 1, 0, 0) }
                        .expect("system points draw must record");
                } else if view == ViewContent::PlanetView {
                    let aspect = vp.w / vp.h;
                    // Player mode renders the planet through the player
                    // camera.
                    let main_vp = if player_active {
                        let player = &self.debug.viewer.player;
                        player.projection_matrix(aspect) * player.view_matrix()
                    } else {
                        self.camera.projection_matrix(aspect) * self.camera.view_matrix()
                    };
                    let mvp = main_vp.to_cols_array_2d();
                    // Catalog sky backdrop (star-catalog-streaming):
                    // plan → fetch → expand on cadence; the GPU buffer
                    // re-uploads only when the tile set changes.
                    let (eye_world, view_f32) = if player_active {
                        let player = &self.debug.viewer.player;
                        (player.eye().as_dvec3(), player.view_matrix())
                    } else {
                        (
                            self.camera.anchor() + self.camera.eye().as_dvec3(),
                            self.camera.view_matrix(),
                        )
                    };
                    let forward = (view_f32.inverse() * Vec4::new(0.0, 0.0, -1.0, 0.0)).truncate();
                    let wide = main_vp.to_cols_array().map(|x| x as f64);
                    self.sky.update(&SkyView {
                        view_proj: DMat4::from_cols_array(&wide),
                        cam_forward_world: forward.as_dvec3(),
                        fov_y_rad: FOV_Y as f64,
                        aspect: aspect as f64,
                        world_to_equatorial: WORLD_TO_EQUATORIAL,
                        velocity_world: DVec3::ZERO,
                    });
                    let (sky_points, sky_changed) = self.sky.points(eye_world);
                    if sky_changed {
                        self.sky_vertices = upload_sky_points(&self.memory_allocator, sky_points);
                    }
                    // Twilight stage (F5): the sky-luminance key drives
                    // star visibility through the exposure kernel, so
                    // the viewer shows the DoD-2 fade-in live (ETM-009:
                    // this is where the catalog photometry calibrates).
                    let (sky_key, _) = twilight_key(self.twilight_stage);
                    let sky_exposure =
                        star_visibility(sky_key, &ExposureParams::spec_defaults()) as f32;
                    // Orbit arrival tint (UMAP-021): the target's
                    // atmosphere color re-lights the mesh; no arrival =
                    // untinted default look.
                    let (tint_rgb, use_tint) = self
                        .debug
                        .viewer
                        .arrival
                        .as_ref()
                        .map(|arrival| (arrival.atmosphere.color, 1.0))
                        .unwrap_or(([0.0, 0.0, 0.0], 0.0));
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(ctx.pipelines.map.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.sky_vertices.clone())
                        .expect("vertex buffer must bind")
                        .push_constants(
                            ctx.pipelines.map.layout().clone(),
                            0,
                            // `px_scale` is inert here: sky sprites are
                            // kind 0 (pixel size), never world-scaled.
                            MapPush {
                                mvp,
                                px_scale: 1.0,
                                exposure: sky_exposure,
                            },
                        )
                        .expect("sky push constants must upload");
                    // SAFETY: same PointList contract as the map draws —
                    // `vertex_count` equals the uploaded point count, no
                    // index buffer bound. Backdrop band: no depth write,
                    // the planet fill overdraws next.
                    unsafe { builder.draw(self.sky_vertices.len() as u32, 1, 0, 0) }
                        .expect("sky draw must record");
                    builder
                        .bind_pipeline_graphics(ctx.pipelines.fill.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.fill_vertices.clone())
                        .expect("vertex buffer must bind")
                        .bind_index_buffer(self.fill_indices.clone())
                        .expect("index buffer must bind")
                        .push_constants(
                            ctx.pipelines.fill.layout().clone(),
                            0,
                            FillPush {
                                mvp,
                                tint_rgb,
                                use_tint,
                                highlight,
                                mode,
                                density,
                                seams_on,
                                hover_cell,
                                pin_cell,
                            },
                        )
                        .expect("fill push constants must upload");
                    unsafe { builder.draw_indexed(self.fill_indices.len() as u32, 1, 0, 0, 0) }
                        .expect("fill draw must record");
                    if self.debug.viewer.wireframe && !self.debug.viewer.lines.is_empty() {
                        builder
                            .bind_pipeline_graphics(ctx.pipelines.line.clone())
                            .expect("pipeline must bind")
                            .bind_vertex_buffers(0, self.line_vertices.clone())
                            .expect("vertex buffer must bind")
                            .push_constants(
                                ctx.pipelines.line.layout().clone(),
                                0,
                                LinePush {
                                    mvp,
                                    inflate: LINE_INFLATE,
                                },
                            )
                            .expect("line push constants must upload");
                        // SAFETY: `vertex_count` equals the uploaded line
                        // count and the buffer holds exactly those vertices;
                        // no index buffer is bound for this `LineList` draw.
                        unsafe { builder.draw(self.debug.viewer.lines.len() as u32, 1, 0, 0) }
                            .expect("wireframe draw must record");
                    }
                }
            }
        }

        // UI pass: full-window viewport, solids untextured, then text.
        let ui_viewport = Viewport {
            offset: [0.0, 0.0],
            extent: [win_w, win_h],
            depth_range: 0.0..=1.0,
        };
        let ortho = ortho_matrix(win_w, win_h);
        let solid_count = (items.solids.len() * 6) as u32;
        let text_count = (ui_verts.len() as u32).saturating_sub(solid_count);
        builder
            .set_viewport(0, [ui_viewport].into_iter().collect())
            .expect("viewport must set")
            .bind_pipeline_graphics(ctx.pipelines.ui.clone())
            .expect("pipeline must bind")
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                ctx.pipelines.ui.layout().clone(),
                0,
                ctx.atlas_set.clone().expect("atlas set must exist"),
            )
            .expect("descriptor set must bind")
            .bind_vertex_buffers(0, ui_buffer.clone())
            .expect("vertex buffer must bind")
            .push_constants(
                ctx.pipelines.ui.layout().clone(),
                0,
                UiPush {
                    ortho,
                    use_tex: 0.0,
                },
            )
            .expect("ui push constants must upload");
        if solid_count > 0 {
            // SAFETY: solids are the first `solid_count` vertices of the
            // uploaded UI buffer (see `ui_items_to_vertices` ordering).
            unsafe { builder.draw(solid_count, 1, 0, 0) }.expect("ui solids draw must record");
        }
        builder
            .push_constants(
                ctx.pipelines.ui.layout().clone(),
                0,
                UiPush {
                    ortho,
                    use_tex: 1.0,
                },
            )
            .expect("ui push constants must upload");
        if text_count > 0 {
            // SAFETY: text quads follow the solids in the uploaded UI
            // buffer; `solid_count + text_count` is the uploaded length.
            unsafe { builder.draw(text_count, 1, solid_count, 0) }
                .expect("ui text draw must record");
        }
        // Dropdown menu: own buffer, drawn after every other UI surface
        // with the same pipeline so it is topmost by command order.
        if let Some(drop_buffer) = drop_buffer {
            let drop_text_count = (drop_verts.len() as u32).saturating_sub(drop_solid_count);
            builder
                .bind_vertex_buffers(0, drop_buffer.clone())
                .expect("dropdown buffer must bind")
                .push_constants(
                    ctx.pipelines.ui.layout().clone(),
                    0,
                    UiPush {
                        ortho,
                        use_tex: 0.0,
                    },
                )
                .expect("ui push constants must upload");
            if drop_solid_count > 0 {
                unsafe { builder.draw(drop_solid_count, 1, 0, 0) }
                    .expect("dropdown solids draw must record");
            }
            builder
                .push_constants(
                    ctx.pipelines.ui.layout().clone(),
                    0,
                    UiPush {
                        ortho,
                        use_tex: 1.0,
                    },
                )
                .expect("ui push constants must upload");
            if drop_text_count > 0 {
                unsafe { builder.draw(drop_text_count, 1, drop_solid_count, 0) }
                    .expect("dropdown text draw must record");
            }
        }
        builder
            .end_render_pass(Default::default())
            .expect("render pass must end");
        let command_buffer = builder.build().expect("command buffer must build");

        let ctx = self.main.as_mut().expect("main window must exist");
        let future = ctx
            .previous_frame_end
            .take()
            .expect("frame future")
            .join(acquire_future)
            .then_execute(self.queue.clone(), command_buffer)
            .expect("command buffer must submit")
            .then_swapchain_present(
                self.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(ctx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush();
        match future.map_err(Validated::unwrap) {
            Ok(future) => {
                if let Some((buffer, extent)) = capture_readback {
                    // Windowed capture (F12), second half: this frame's
                    // GPU work is ≤1 frame by construction — wait for
                    // it, then map + encode + write on the CPU.
                    self.pending_capture = false;
                    match future.wait(None) {
                        Ok(()) => {
                            ctx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
                            self.write_windowed_capture(&buffer, extent);
                        }
                        Err(error) => {
                            ctx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
                            self.debug
                                .fx
                                .notify(format!("F12 capture failed on the GPU: {error:?}"));
                        }
                    }
                } else {
                    ctx.previous_frame_end = Some(future.boxed());
                }
            }
            Err(VulkanError::OutOfDate) => {
                if capture_readback.is_some() {
                    self.pending_capture = false;
                    self.debug.fx.notify(
                        "F12 capture lost (swapchain out of date) — press F12 again".to_owned(),
                    );
                }
                ctx.recreate_swapchain = true;
                ctx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
            }
            Err(error) => panic!("frame flush failed: {error}"),
        }
    }

    /// Windowed capture (F12), CPU half: BGRA→RGBA swizzle + PNG write
    /// + Console/notice log. Failures notify, never panic.
    fn write_windowed_capture(&mut self, buffer: &Subbuffer<[u8]>, extent: [u32; 2]) {
        use game_debug::cosmic_capture::{encode_png_rgba8, windowed_capture_filename};
        let surface = match self.debug.screen {
            Screen::GameDemo => "demo",
            Screen::Dimensions(_) => "inspector",
            Screen::Settings => "settings",
        };
        let path =
            windowed_capture_filename(surface, self.debug.cosmic.seed, &capture_now_timestamp());
        let result = (|| -> Result<String, String> {
            if let Some(parent) = std::path::Path::new(&path).parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
            }
            let guard = buffer.read().map_err(|e| format!("{e:?}"))?;
            let mut rgba = guard.to_vec();
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            let png = encode_png_rgba8(extent[0], extent[1], &rgba).map_err(|e| e.to_string())?;
            std::fs::write(&path, &png).map_err(|e| format!("{e}"))?;
            Ok(path.clone())
        })();
        match result {
            Ok(path) => {
                let line = format!("capture saved: {path}");
                tracing::info!("{line}");
                self.debug.console.push(line.clone());
                self.debug.fx.notify(line);
            }
            Err(error) => {
                self.debug.fx.notify(format!("F12 capture failed: {error}"));
            }
        }
    }
}

fn main() {
    // `RUST_LOG` overrides; default to `info` so the viewer boot + mesh
    // lines show without extra flags.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    std::process::exit(run());
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    let args = match parse_args(&argv) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if args.headless {
        return run_headless(args.seed);
    }
    if let Some(request) = args.capture {
        return run_capture(request, args.seed);
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("event loop failed: {error}");
            return 1;
        }
    };
    let mut app = ViewerApp::new(&event_loop, args.seed);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("viewer failed: {error:?}");
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_int_groups_thousands() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(12), "12");
        assert_eq!(fmt_int(642), "642");
        assert_eq!(fmt_int(40962), "40,962");
        assert_eq!(fmt_int(655362), "655,362");
    }

    #[test]
    fn capture_timestamp_formats_utc() {
        // 2026-09-20 20:00:00 UTC — month/day/hour boundaries pinned.
        assert_eq!(capture_timestamp(1_789_934_400), "20260920-200000");
        assert_eq!(capture_timestamp(0), "19700101-000000");
        assert_eq!(capture_timestamp(86_399), "19700101-235959");
        assert_eq!(capture_timestamp(86_400), "19700102-000000");
        // Leap day 2024-02-29 12:00:00 UTC.
        assert_eq!(capture_timestamp(1_709_208_000), "20240229-120000");
        // Shape: 15 chars, dash at 8.
        let ts = capture_now_timestamp();
        assert_eq!(ts.len(), 15, "bad timestamp shape: {ts}");
        assert_eq!(&ts[8..9], "-");
    }

    #[test]
    fn cli_seed_parsing() {
        let argv = |args: &[&str]| args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Bare + headless.
        let args = parse_args(&argv(&["game_debug"])).expect("bare must parse");
        assert!(!args.headless && args.seed.is_none());
        let args = parse_args(&argv(&["game_debug", "--headless"])).expect("flag must parse");
        assert!(args.headless && args.seed.is_none());
        // Seed forms.
        let args = parse_args(&argv(&["game_debug", "--seed", "42"])).expect("seed must parse");
        assert_eq!(args.seed, Some(42));
        let args = parse_args(&argv(&["game_debug", "--headless", "--seed", "7"]))
            .expect("combined must parse");
        assert!(args.headless && args.seed == Some(7));
        // Failures carry usage.
        for bad in [
            vec!["game_debug", "--seed"],
            vec!["game_debug", "--seed", "abc"],
            vec!["game_debug", "--seed", "-1"],
            vec!["game_debug", "--nope"],
        ] {
            let err = parse_args(&argv(&bad)).expect_err("must reject");
            assert!(err.contains("usage:"), "error lacks usage: {err}");
        }
    }

    #[test]
    fn vista_capture_pose_matches_t0_pose() {
        // CVI-008/DoD 5: the `vista` preset IS the t = 0 vista pose —
        // the capture camera carries `vista_pose()` exactly (eye,
        // target, 25° FOV), with the pose's fog/slab terms.
        use game_debug::cosmic_capture::CaptureView;
        use game_debug::cosmic_vista::{VISTA_FOV_DEG, vista_pose};
        let mut debug = DebugApp::new();
        pose_demo_camera_for_vista_capture(&mut debug);
        let chase = debug.cosmic.chase_pose();
        let want = vista_pose(
            &debug.cosmic.web,
            debug.cosmic.params.descriptor_radius_mpc,
            &chase,
        );
        let eye = debug.cosmic.camera.eye_world();
        assert!((eye - want.eye).length() < 1e-9);
        assert!((debug.cosmic.camera.fov_y() - VISTA_FOV_DEG.to_radians()).abs() < 1e-6);
        assert_eq!(want.fov_y_deg, VISTA_FOV_DEG);
        assert_eq!(want.slab_half_mpc, 20.0);
        assert_eq!(want.inv_fog_l, 0.0);
        // The request routes vista to the demo surface (preset pin).
        assert!(game_debug::cosmic_capture::preset_for(CaptureView::Vista).surface_is_demo);
    }

    #[test]
    fn cli_capture_parsing() {
        use game_debug::cosmic_capture::CaptureView;
        let argv = |args: &[&str]| args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Full form.
        let args = parse_args(&argv(&[
            "game_debug",
            "--capture",
            "shots/a.png",
            "--seed",
            "1337",
            "--view",
            "slab",
            "--size",
            "800x600",
        ]))
        .expect("capture must parse");
        let req = args.capture.expect("capture request must exist");
        assert_eq!(req.path, "shots/a.png");
        assert_eq!(req.view, CaptureView::Slab);
        assert_eq!((req.width, req.height), (800, 600));
        assert_eq!(args.seed, Some(1337));
        // Defaults: inspector preset, target aspect.
        let args = parse_args(&argv(&["game_debug", "--capture", "b.png"])).expect("defaults");
        let req = args.capture.expect("capture request must exist");
        assert_eq!(req.view, CaptureView::Inspector);
        assert_eq!(
            (req.width, req.height),
            game_debug::cosmic_capture::CAPTURE_DEFAULT_SIZE
        );
        // Failures carry usage.
        for bad in [
            vec!["game_debug", "--capture"],
            vec!["game_debug", "--capture", "a.png", "--view", "orbit"],
            vec!["game_debug", "--capture", "a.png", "--size", "abc"],
            vec!["game_debug", "--capture", "a.png", "--size", "0x10"],
            vec!["game_debug", "--capture", "a.png", "--headless"],
            vec!["game_debug", "--headless", "--capture", "a.png"],
        ] {
            let err = parse_args(&argv(&bad)).expect_err("must reject");
            assert!(err.contains("usage:"), "error lacks usage: {err}");
        }
    }

    #[test]
    fn ortho_maps_corners() {
        let m = ortho_matrix(1280.0, 720.0);
        let project = |x: f32, y: f32| (m[0][0] * x + m[3][0], m[1][1] * y + m[3][1]);
        assert_eq!(project(0.0, 0.0), (-1.0, 1.0));
        assert_eq!(project(1280.0, 720.0), (1.0, -1.0));
        assert_eq!(project(640.0, 360.0), (0.0, 0.0));
    }

    #[test]
    fn viewer_shaders_compile() {
        for (kind, source, what) in [
            (ShaderKind::Vertex, FILL_VERT, "fill vert"),
            (ShaderKind::Fragment, FILL_FRAG, "fill frag"),
            (ShaderKind::Vertex, LINE_VERT, "line vert"),
            (ShaderKind::Fragment, LINE_FRAG, "line frag"),
            (ShaderKind::Vertex, UI_VERT, "ui vert"),
            (ShaderKind::Fragment, UI_FRAG, "ui frag"),
            (ShaderKind::Vertex, MAP_VERT, "map vert"),
            (ShaderKind::Fragment, MAP_FRAG, "map frag"),
            (ShaderKind::Vertex, GLOW_VERT, "glow vert"),
            (ShaderKind::Fragment, GLOW_FRAG, "glow frag"),
            (ShaderKind::Vertex, SPLAT_VERT, "splat vert"),
            (ShaderKind::Fragment, SPLAT_FRAG, "splat frag"),
            (ShaderKind::Vertex, SPLAT_PROC_VERT, "proc splat vert"),
            (ShaderKind::Fragment, MARCH_FRAG, "march frag"),
        ] {
            if let Err(error) = compile_glsl_to_spirv(kind, source) {
                panic!("{what} must compile: {error}");
            }
        }
    }

    #[test]
    fn cosmic_vertex_inputs_match_vertex_fields() {
        // Regression pin for the windowed startup panic
        // (update-2026-09-18-2328): vulkano maps shader inputs to
        // vertex-struct fields BY NAME at pipeline creation — naga
        // compilation cannot catch a mismatch, and there is no
        // GPU-free way to run the real check, so the names are pinned
        // here. `MapVertex { map_pos, color, misc }`,
        // `SplatVertex { pos, packed }`.
        for (source, name, what) in [
            (GLOW_VERT, "in vec3 map_pos;", "glow map_pos"),
            (GLOW_VERT, "in vec3 color;", "glow color"),
            (GLOW_VERT, "in vec3 misc;", "glow misc"),
            (SPLAT_VERT, "in vec3 pos;", "splat pos"),
            (SPLAT_VERT, "in uint packed;", "splat packed"),
        ] {
            assert!(
                source.contains(name),
                "{what} input missing from its vertex shader"
            );
        }
        // 16 B vertex: the Low buffer budget (300k × 16 B = 4.8 MB)
        // depends on it.
        assert_eq!(std::mem::size_of::<SplatVertex>(), 16);
    }

    #[test]
    fn rebase_path_touches_demo_buffers_only() {
        // `cosmic-rebase-async` CRA-002 / DoD 2 source pin: the
        // frame-loop rebase path rebuilds only the demo glow + splat
        // buffers — veil volume, HDR chain and inspector buffers are
        // unreachable from it (they rebuild on the seed path only).
        // String pins: no GPU-free way to observe `Subbuffer` identity.
        let source = include_str!("main.rs");
        let tick = viewer_method_body(source, "tick_cosmic");
        for banned in [
            "upload_veil_volume",
            "build_hdr_chain",
            "cosmic_tab_",
            "refresh_cosmic_seed",
            "wait(",
        ] {
            assert!(
                !tick.contains(banned),
                "tick_cosmic must not contain {banned:?}"
            );
        }
        for required in [
            "rebase_worker",
            "rebase_pending",
            "rebase: queued",
            "rebase: swapped",
            "rebased_to",
        ] {
            assert!(
                tick.contains(required),
                "tick_cosmic must contain {required:?}"
            );
        }
        let seed = viewer_method_body(source, "refresh_cosmic_seed");
        for required in [
            "upload_veil_volume",
            "build_hdr_chain",
            "cosmic_tab_glow",
            "cosmic_tab_splats",
        ] {
            assert!(
                seed.contains(required),
                "refresh_cosmic_seed must contain {required:?}"
            );
        }
        let demo = viewer_method_body(source, "rebuild_demo_buffers");
        for banned in [
            "upload_veil_volume",
            "build_hdr_chain",
            "cosmic_tab_",
            "wait(",
        ] {
            assert!(
                !demo.contains(banned),
                "rebuild_demo_buffers must not contain {banned:?}"
            );
        }
        // Fence pin (CRA-006 / FR5): the veil upload's blocking wait is
        // reachable only through the seed path — the veil upload has
        // exactly four call sites: its definition, the offscreen
        // capture entry, the boot upload, and refresh_cosmic_seed. The
        // needle is built at runtime so this pin does not count itself.
        let needle = ["upload_veil_volume", "("].concat();
        let sites: Vec<usize> = source
            .match_indices(needle.as_str())
            .map(|(index, _)| index)
            .collect();
        assert_eq!(sites.len(), 4, "veil upload call sites changed");
        let mut owners: Vec<&str> = sites.iter().map(|pos| enclosing_fn(source, *pos)).collect();
        owners.sort_unstable();
        assert_eq!(
            owners,
            [
                "new",
                "refresh_cosmic_seed",
                "run_capture",
                "upload_veil_volume"
            ],
            "veil upload reached from an unexpected path"
        );
    }

    #[test]
    fn splat_proc_resources_seed_only() {
        // `cosmic-gpu-tracers` CGT-004 seed-only pin: the displacement
        // volume + cell list build on the seed path (boot / reseed /
        // capture) and are unreachable from the frame-loop rebase path
        // (tracers never rebuild per travel — ADR-026 §2). String pins:
        // no GPU-free way to observe image / SSBO identity.
        let source = include_str!("main.rs");
        let tick = viewer_method_body(source, "tick_cosmic");
        for banned in [
            "upload_displacement_volume",
            "upload_cell_list",
            "disp_volume",
            "cell_list",
        ] {
            assert!(
                !tick.contains(banned),
                "tick_cosmic must not contain {banned:?}"
            );
        }
        let seed = viewer_method_body(source, "refresh_cosmic_seed");
        for required in ["upload_displacement_volume", "upload_cell_list"] {
            assert!(
                seed.contains(required),
                "refresh_cosmic_seed must contain {required:?}"
            );
        }
        let demo = viewer_method_body(source, "rebuild_demo_buffers");
        for banned in ["upload_displacement_volume", "upload_cell_list"] {
            assert!(
                !demo.contains(banned),
                "rebuild_demo_buffers must not contain {banned:?}"
            );
        }
    }

    /// Name of the free function or `ViewerApp` method enclosing byte
    /// `pos` (the rebase pins above).
    fn enclosing_fn(source: &str, pos: usize) -> &str {
        let head = &source[..pos];
        let method = head.rfind("\n    fn ");
        let free = head.rfind("\nfn ");
        let start = method.max(free).expect("a fn must precede the call site");
        let tail = &source[start..];
        let rest = &tail[tail.find("fn ").expect("fn keyword") + 3..];
        let end = rest.find(['(', ' ', '\n']).expect("fn name must end");
        rest[..end].trim()
    }

    /// Slice the `impl ViewerApp` method `name` out of this file's own
    /// source (the rebase source pins above — same string-pin
    /// precedent as `cosmic_shader_safety_pins`).
    fn viewer_method_body<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source
            .find(&format!("    fn {name}("))
            .unwrap_or_else(|| panic!("method {name} missing from main.rs"));
        let rest = &source[start..];
        let end = rest
            .find("\n    fn ")
            .map(|i| start + i)
            .unwrap_or(source.len());
        &source[start..end]
    }

    #[test]
    fn cosmic_shader_safety_pins() {
        // Regression pins for the visual-issue fix
        // (update-2026-09-18-2328): the sprite falloff must hit
        // exactly zero at the rim (else full quads), and the redshift
        // depth term must be clamped non-negative and capped (else
        // Inf/NaN/negative channels decorrelate into rainbow squares).
        // String pins, because neither naga nor any GPU-free test can
        // evaluate the shaders. (Smoke pins retired with the smoke
        // path, `cosmic-gas-veil-v2` CGV-009.)
        for (source, literal, what) in [
            (GLOW_FRAG, "1.0 - 4.0 * dot(d, d)", "rim-zero falloff"),
            (GLOW_VERT, "max(clip.w, 0.0)", "glow depth clamp"),
            (GLOW_VERT, "misc.z < 0.5", "glow kind branch"),
            (GLOW_FRAG, "smoothstep(0.2, 0.5, r)", "hub kind-2 core ramp"),
            (
                SPLAT_FRAG,
                "1.0 - 4.0 * dot(d, d)",
                "splat rim-zero falloff",
            ),
            (SPLAT_VERT, "max(clip.w, 0.0)", "splat depth clamp"),
            (SPLAT_VERT, "65535.0 * 16.0 - 8.0", "splat unpack mirror"),
            (
                SPLAT_VERT,
                "smoothstep(h, 2.0 * h, dist)",
                "splat near-eye fade",
            ),
        ] {
            assert!(source.contains(literal), "{what} missing from its shader");
        }
        for source in [GLOW_VERT, SPLAT_VERT] {
            assert!(
                source.contains(", 0.5)"),
                "redshift cap missing from a cosmic vertex shader"
            );
        }
        // Splat fragment: arithmetic-only (mobile fill-rate rule, A-5) —
        // `exp2`/`log2` live in the vertex stage only.
        for banned in ["sin(", "cos(", "exp(", "pow(", "log("] {
            assert!(
                !SPLAT_FRAG.contains(banned),
                "splat fragment must stay arithmetic-only: {banned}"
            );
        }
        // Splat density ramp shares the CPU stop table end-to-end.
        for literal in ["vec3(0.10, 0.08, 0.35)", "vec3(1.00, 0.45, 0.40)"] {
            assert!(
                SPLAT_VERT.contains(literal),
                "splat ramp drifted from DENSITY_RAMP_STOPS: {literal}"
            );
        }
    }

    #[test]
    fn cosmic_window_snippet_shared() {
        // CDW-002/A-4: one `COSMIC_WINDOW_GLSL` source of truth,
        // byte-identical in every cosmic vertex shader (the lib const
        // is the authority — the binary pastes it verbatim).
        use game_debug::cosmic_window::COSMIC_WINDOW_GLSL;
        for (source, what) in [
            (GLOW_VERT, "glow"),
            (SPLAT_VERT, "splat"),
            (SPLAT_PROC_VERT, "proc splat"),
        ] {
            assert!(
                source.contains(COSMIC_WINDOW_GLSL),
                "{what} shader drifted from the shared window snippet"
            );
        }
    }

    #[test]
    fn cosmic_density_ramp_shared() {
        // CGV-001/A-4: one `COSMIC_DENSITY_RAMP_GLSL` source of truth,
        // byte-identical in the splat vertex shader and the veil march
        // fragment shader (the lib const is the authority — the binary
        // pastes it verbatim). Veil sprites ride the glow pipeline with
        // CPU-computed colors (`veil_ramp_cpu`, pinned against
        // `VEIL_RAMP_STOPS` in `cosmic_veil` tests).
        use game_debug::cosmic_veil::COSMIC_DENSITY_RAMP_GLSL;
        for (source, what) in [
            (SPLAT_VERT, "splat"),
            (SPLAT_PROC_VERT, "proc splat"),
            (MARCH_FRAG, "march"),
        ] {
            assert!(
                source.contains(COSMIC_DENSITY_RAMP_GLSL),
                "{what} shader drifted from the shared density-ramp snippet"
            );
        }
    }

    #[test]
    fn splat_proc_shader_pins() {
        // `cosmic-gpu-tracers` CGT-007 / FR7 string pins: the proc
        // vertex shader's CPU-mirrored constants must not drift
        // silently (naga compiles the shape, never the values).
        for (literal, what) in [
            // Sub-offset table: the refine() pattern, x fastest —
            // mirrors `cosmic_splat::splat_sub_offsets` bit-for-bit.
            ("0.25 + 0.5 * float(sub & 1u)", "sub-offset x"),
            ("0.25 + 0.5 * float((sub >> 1u) & 1u)", "sub-offset y"),
            ("0.25 + 0.5 * float((sub >> 2u) & 1u)", "sub-offset z"),
            // SNORM unpack: mirrors `DISP_QUANT_RANGE_CELLS` (±8).
            ("DISP_SNORM_CELLS = 8.0", "SNORM range"),
            // Sphere cull on the box-frame position.
            ("dot(box_pos, box_pos) > radius * radius", "sphere test"),
            // Density unpack: same R8 packing the march inverts.
            ("density_q * 10.0 - 4.0", "density unpack"),
            // Vertex-index drive, no vertex input.
            ("uint(gl_VertexIndex)", "index drive"),
            // Explicit LOD 0: the implicit-`texture()` form is invalid
            // in the vertex stage (no derivatives) — pinned so a
            // refactor can't reintroduce it.
            (
                "textureLod(sampler3D(disp_tex, disp_sampler), q / n, 0.0)",
                "disp fetch",
            ),
            (
                "textureLod(sampler3D(density_tex, density_sampler), e / n, 0.0)",
                "density fetch",
            ),
            ("readonly buffer CellList", "cell list SSBO"),
            ("uint cell_indices[]", "cell list array"),
            // Params block layout (matches `SplatProcParams`).
            ("vec4 origin_cell;", "origin_cell"),
            ("vec4 radius_k;", "radius_k"),
        ] {
            assert!(
                SPLAT_PROC_VERT.contains(literal),
                "{what} missing from the proc splat shader"
            );
        }
        // The proc path shares the legacy fragment (arithmetic-only —
        // A-1): same module object in both pipelines, pinned here so a
        // fork can't sneak in.
        assert!(
            !SPLAT_PROC_VERT.contains("SPLAT_FRAG"),
            "proc shader must not duplicate the fragment"
        );
        // Params UBO: two vec4s, no padding traps (32 B).
        assert_eq!(std::mem::size_of::<SplatProcParams>(), 32);
    }

    #[test]
    fn march_gains_stay_decoupled() {
        // Grade-round-2 lesson (CGV-015): the march push gain and the
        // resolve composite gain are DIFFERENT knobs fed from different
        // consts. One const fed both sides once and blew the composite
        // out 30×30. The push stays a passthrough; the grade lives at
        // the resolve.
        assert_eq!(VEIL_MARCH_GAIN, 1.0, "march push must stay a passthrough");
        assert_eq!(
            VEIL_MARCH_RESOLVE_GAIN, 30.0,
            "resolve grade carries the 30 Mpc reference window"
        );
    }

    #[test]
    fn march_push_fits_vulkan_floor() {
        // CGV-005/A: the march push is exactly the 128 B Vulkan 1.1
        // floor (inverse VP + eye/sphere + grid frame + window terms).
        assert_eq!(std::mem::size_of::<MarchPush>(), 128);
    }

    #[test]
    fn march_emission_matches_grade() {
        // CGV grade knob: the GLSL emission literal tracks
        // `VEIL_MARCH_K` (`a = k·max(0, od − 0.5)`).
        assert!(
            MARCH_FRAG.contains(&format!("{VEIL_MARCH_K} * max(0.0, od - 0.5)")),
            "march emission drifted from VEIL_MARCH_K"
        );
    }

    #[test]
    fn march_loop_stays_narrow() {
        // CGV-005/A-3: the march loop is texture fetch + mix/FMA with
        // one scoped `exp2` (log-density → overdensity; the march never
        // runs on Low). Everything else transcendental stays out.
        for banned in ["pow(", "log(", "sin(", "cos(", "tan("] {
            assert!(
                !MARCH_FRAG.contains(banned),
                "march loop must stay narrow: {banned}"
            );
        }
        assert_eq!(
            MARCH_FRAG.matches("exp2(").count(),
            1,
            "exactly one scoped exp2 in the march loop"
        );
        // Ray setup mirrors `cosmic_veil::march_ray` (CGV-007): same
        // quadratic tokens on both sides.
        for token in ["dot(oc, dir)", "dot(oc, oc) - radius * radius", "b * b - c"] {
            assert!(
                MARCH_FRAG.contains(token),
                "march ray formula drifted from the CPU mirror: {token}"
            );
        }
        // Shared ramp + window snippets (CGV-001/A-4): the march uses
        // the same density ramp and window term as the sprites.
        for literal in ["vec3(0.10, 0.08, 0.35)", "vec3(1.00, 0.45, 0.40)"] {
            assert!(
                MARCH_FRAG.contains(literal),
                "march ramp drifted from DENSITY_RAMP_STOPS: {literal}"
            );
        }
        use game_debug::cosmic_window::COSMIC_WINDOW_GLSL;
        assert!(
            MARCH_FRAG.contains(COSMIC_WINDOW_GLSL),
            "march drifted from the shared window snippet"
        );
        // Grade round 2 (CGV-015): vis-weighted MEAN, not a column —
        // full-depth views must divide by their own weight.
        for token in ["float wsum = 0.0;", "wsum += w;", "acc / max(wsum, 1e-6)"] {
            assert!(
                MARCH_FRAG.contains(token),
                "march mean-normalization regressed: {token}"
            );
        }
    }

    #[test]
    fn fill_push_constants_fit_vulkan_floor() {
        // Vulkan 1.1 guarantees only 128 B of push constants; the fill
        // block (MVP + flags + hover/pin ids) must stay under it on every
        // tier, or pipeline creation fails on the floor devices.
        let bytes = std::mem::size_of::<FillPush>();
        assert!(
            bytes <= 128,
            "FillPush is {bytes} B, over the 128 B Vulkan 1.1 floor"
        );
    }

    #[test]
    fn map_push_constants_fit_vulkan_floor() {
        // Same 128 B floor for the map block (MVP + pixels-per-unit).
        let bytes = std::mem::size_of::<MapPush>();
        assert!(
            bytes <= 128,
            "MapPush is {bytes} B, over the 128 B Vulkan 1.1 floor"
        );
    }

    #[test]
    fn cosmic_push_constants_fit_vulkan_floor() {
        // Cosmic blocks: glow (88 B with the depth-window terms) and
        // splat (112 B) must stay under the 128 B Vulkan 1.1 floor on
        // every tier (march has its own 128 B-exact pin above).
        for (bytes, what) in [
            (std::mem::size_of::<GlowPush>(), "GlowPush"),
            (std::mem::size_of::<SplatPush>(), "SplatPush"),
        ] {
            assert!(
                bytes <= 128,
                "{what} is {bytes} B, over the 128 B Vulkan 1.1 floor"
            );
        }
    }

    #[test]
    fn gnomonic_glsl_matches_engine() {
        use game_engine::render::glsl_const_block;
        // The checker face table is generated from the same base
        // icosahedron as the mesh: the shader must embed the generator
        // output verbatim (literals round-trip bit-exactly). Any drift
        // would silently skew the checker vs the mesh faces.
        let block = glsl_const_block();
        assert!(
            FILL_FRAG.contains(&block),
            "FILL_FRAG gnomonic table diverged from engine::render::checker"
        );
    }

    #[test]
    fn panel_plans_stay_inside_and_ordered() {
        for warn in [false, true] {
            for checker in [false, true] {
                for player_active in [false, true] {
                    let layout = ui::layout(1280.0, 720.0);
                    let planet = planet_left_plan(layout.left, 19.0, checker);
                    let right = right_panel_plan(layout.panel, 19.0, warn, player_active);
                    assert_eq!(right.warn_line.is_some(), warn);
                    // Player block shrinks to one "off" line when the player is off.
                    assert_eq!(right.player_lines.len(), if player_active { 4 } else { 1 });
                    // Density rows only exist in Checker mode.
                    assert_eq!(planet.rects.density_track.is_some(), checker);
                    assert_eq!(planet.density_label.is_some(), checker);
                    // Every widget rect stays inside its dock.
                    for rect in [
                        planet.rects.shader_button,
                        planet.rects.wire_box,
                        planet.rects.pent_box,
                        planet.rects.seam_box,
                    ] {
                        assert!(rect.x >= layout.left.x, "{rect:?}");
                        assert!(
                            rect.x + rect.w <= layout.left.x + layout.left.w + 1e-3,
                            "{rect:?}"
                        );
                    }
                    for rect in planet.rects.density_track.into_iter() {
                        assert!(rect.x >= layout.left.x, "{rect:?}");
                        assert!(
                            rect.x + rect.w <= layout.left.x + layout.left.w + 1e-3,
                            "{rect:?}"
                        );
                    }
                    for rect in [
                        right.rects.subdiv_field,
                        right.rects.subdiv_track,
                        right.rects.radius_field,
                        right.rects.regen_button,
                    ] {
                        assert!(rect.x >= layout.panel.x, "{rect:?}");
                        assert!(
                            rect.x + rect.w <= layout.panel.x + layout.panel.w + 1e-3,
                            "{rect:?}"
                        );
                    }
                    for row in planet.rects.preset_grid {
                        for button in row {
                            assert!(button.x >= layout.left.x, "{button:?}");
                            assert!(
                                button.x + button.w <= layout.left.x + layout.left.w + 1e-3,
                                "{button:?}"
                            );
                        }
                    }
                    // Planet dock flows top-down: view, presets, shader, overlays.
                    assert!(planet.view_header.y < planet.preset_hint.y);
                    assert!(planet.rects.shader_button.y > planet.shader_header.y);
                    assert!(planet.shader_hint.y > planet.rects.shader_button.y);
                    assert!(planet.overlay_header.y < planet.rects.wire_box.y);
                    assert!(planet.rects.wire_box.y < planet.rects.pent_box.y);
                    assert!(planet.rects.pent_box.y < planet.rects.seam_box.y);
                    // Preset grid: top row above bottom row, left column left of right.
                    assert!(planet.rects.preset_grid[0][0].y < planet.rects.preset_grid[1][0].y);
                    assert!(planet.rects.preset_grid[0][0].x < planet.rects.preset_grid[0][1].x);
                    // Right dock flows top-down: inputs, selection, stats.
                    assert!(right.rects.subdiv_field.y < right.rects.subdiv_track.y);
                    assert!(right.rects.subdiv_track.y < right.rects.radius_field.y);
                    assert!(right.rects.radius_field.y < right.rects.regen_button.y);
                    assert!(right.inputs_header.y < right.subdiv_label.y);
                    assert!(right.subdiv_hint.y < right.radius_label.y);
                    assert!(right.chunk_lines.windows(2).all(|w| w[0].y < w[1].y));
                    assert!(right.player_lines.windows(2).all(|w| w[0].y < w[1].y));
                    assert!(right.chunk_lines[3].y < right.player_lines[0].y);
                    assert!(right.stat_lines.windows(2).all(|w| w[0].y < w[1].y));
                    assert!(
                        right.player_lines.last().expect("player line").y < right.stats_header.y
                    );
                }
            }
        }
    }

    #[test]
    fn viewer_ui_contains_panel_content() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        app.select_screen(Screen::Dimensions(WaypointId::Earth));
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let joined = |items: &UiItems| {
            items
                .texts
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Earth tab: top bar + presets + planet overlays.
        let planet = joined(&build_planet_ui(
            &mut atlas,
            &app,
            &SkySummary::default(),
            layout,
        ));
        assert!(!planet.is_empty());
        for needle in [
            "GAME DEMO",
            "DIMENSIONS:",
            "SETTINGS",
            "INPUTS",
            "Subdivisions",
            "Radius",
            "Regenerate",
            "Wireframe",
            "Pentagons",
            "Seams",
            "SELECTION",
            "chunk:",
            "state:",
            "Shader: Lit",
            "off (U)",
            "OVERLAYS",
            "VIEW",
            "Camera presets (G/T/B/R)",
            "Top",
            "Bot",
            "Right",
            "Persp",
            "STATS",
            "cells:",
            "hash:",
            "view:",
            "click cycles",
        ] {
            assert!(planet.contains(needle), "planet ui missing {needle}");
        }
        for absent in [
            "Swap view (V)",
            "click/V",
            "UV wire",
            "UV DEBUG",
            "UV Net",
            "Checker density:",
            "move:     WASD",
            "cam:      P cycles",
        ] {
            assert!(!planet.contains(absent), "planet ui leaks {absent}");
        }
        // Checker mode reveals the density slider on the planet content.
        let mut checker_app = DebugApp::with_viewer(PlanetViewerState::with_values(1, 1.0));
        checker_app.select_screen(Screen::Dimensions(WaypointId::Earth));
        checker_app.viewer.debug_mode = DebugMode::Checker;
        let layout = app_layout(checker_app.chrome, 1280.0, 720.0);
        let planet_checker = joined(&build_planet_ui(
            &mut atlas,
            &checker_app,
            &SkySummary::default(),
            layout,
        ));
        assert!(planet_checker.contains("Checker density:"));
    }

    #[test]
    fn global_presets_frame_the_planet() {
        for (preset, _) in GlobalPreset::ALL {
            let mut camera = OrbitCamera::framing_planet(2.0);
            snap_global_camera(&mut camera, preset, 2.0);
            let offset = camera.eye() - Vec3::ZERO;
            // All presets sit at framing distance.
            assert!((offset.length() - 6.4).abs() < 1e-4, "{preset:?}");
            let up = offset.normalize();
            match preset {
                GlobalPreset::Top => assert!(up.y > 0.99, "{preset:?} {up:?}"),
                GlobalPreset::Bottom => assert!(up.y < -0.99, "{preset:?} {up:?}"),
                GlobalPreset::Right => {
                    assert!(up.x > 0.99, "{preset:?} {up:?}");
                }
                GlobalPreset::Perspective => {
                    assert!(up.y > 0.0 && up.x > 0.0, "{preset:?} {up:?}");
                }
            }
        }
        assert_eq!(GlobalPreset::from_index(0), Some(GlobalPreset::Top));
        assert_eq!(GlobalPreset::from_index(3), Some(GlobalPreset::Perspective));
        assert_eq!(GlobalPreset::from_index(4), None);
    }

    #[test]
    fn world_to_pixels_centers_and_rejects() {
        let rect = Rect {
            x: 0.0,
            y: 28.0,
            w: 200.0,
            h: 100.0,
        };
        // Identity: origin maps to the rect center.
        let at = world_to_pixels(Mat4::IDENTITY, Vec3::ZERO, rect).expect("center");
        assert!((at.0 - 100.0).abs() < 1e-4 && (at.1 - 78.0).abs() < 1e-4);
        // Outside NDC: no marker.
        assert_eq!(
            world_to_pixels(Mat4::IDENTITY, Vec3::new(2.0, 0.0, 0.0), rect),
            None
        );
        // Behind the camera (w <= 0): no marker.
        let proj = glam::camera::rh::proj::vulkan::perspective(1.0, 2.0, 0.1, 100.0);
        assert_eq!(world_to_pixels(proj, Vec3::new(0.0, 0.0, 5.0), rect), None);
        // In front: projects inside.
        assert!(world_to_pixels(proj, Vec3::new(0.0, 0.0, -5.0), rect).is_some());
        // Degenerate rect: no marker.
        assert_eq!(
            world_to_pixels(
                Mat4::IDENTITY,
                Vec3::ZERO,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0
                }
            ),
            None
        );
    }

    #[test]
    fn arrow_tris_points_along_dir() {
        // Origin (0,0), +x dir, 20 long, 4 wide, 8 head: shaft spans
        // x 0..12, head tip lands exactly on (20, 0).
        let [s0, s1, head] = arrow_tris((0.0, 0.0), (1.0, 0.0), 20.0, 4.0, 8.0);
        assert_eq!(s0, ((0.0, 2.0), (12.0, 2.0), (0.0, -2.0)));
        assert_eq!(s1, ((0.0, -2.0), (12.0, 2.0), (12.0, -2.0)));
        assert_eq!(head, ((12.0, 4.0), (20.0, 0.0), (12.0, -4.0)));
    }

    #[test]
    fn draw_player_marker_emits_dot_arrow_and_label() {
        let mut items = UiItems::default();
        draw_player_marker(&mut items, (100.0, 100.0), Some((130.0, 100.0)));
        assert_eq!(items.solids.len(), 1, "one center dot");
        assert_eq!(items.tris.len(), 3, "shaft (2) + head (1)");
        assert_eq!(items.texts.len(), 1);
        assert_eq!(items.texts[0].text, "YOU");
        // No tip: dot + label only.
        let mut bare = UiItems::default();
        draw_player_marker(&mut bare, (100.0, 100.0), None);
        assert_eq!(bare.solids.len(), 1);
        assert!(bare.tris.is_empty());
    }

    #[test]
    fn draw_target_ring_emits_four_rects() {
        let mut items = UiItems::default();
        draw_target_ring(&mut items, (100.0, 100.0), TARGET_RING_R, C_WARN);
        assert_eq!(items.solids.len(), 4, "top/bottom/left/right bars");
        assert!(items.tris.is_empty());
        assert!(items.texts.is_empty());
        assert!(
            items.solids.iter().all(|(_, color)| *color == C_WARN),
            "ring rides one color"
        );
        let r = TARGET_RING_R;
        for want in [
            Rect {
                x: 100.0 - r,
                y: 100.0 - r,
                w: 2.0 * r,
                h: 1.5,
            },
            Rect {
                x: 100.0 - r,
                y: 100.0 + r,
                w: 2.0 * r,
                h: 1.5,
            },
            Rect {
                x: 100.0 - r,
                y: 100.0 - r,
                w: 1.5,
                h: 2.0 * r,
            },
            Rect {
                x: 100.0 + r,
                y: 100.0 - r,
                w: 1.5,
                h: 2.0 * r,
            },
        ] {
            assert!(
                items.solids.iter().any(|(rect, _)| *rect == want),
                "ring missing bar {want:?}"
            );
        }
    }

    /// Count amber (target-ring) solids in a composed frame.
    fn warn_solids(items: &UiItems) -> usize {
        items
            .solids
            .iter()
            .filter(|(_, color)| *color == C_WARN)
            .count()
    }

    #[test]
    fn demo_target_ring_overlays_selected_node() {
        // `DebugApp::new` boots on the Game Demo tab with a fixed seed,
        // so the projection below is deterministic.
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        assert_eq!(app.screen, Screen::GameDemo);
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let compose = |app: &DebugApp, atlas: &mut GlyphAtlas| {
            let mut items = UiItems::default();
            let mut drop = UiItems::default();
            compose_overlay_ui(
                &mut items,
                &mut drop,
                atlas,
                app,
                layout,
                (1280.0, 720.0),
                None,
            );
            items
        };
        let baseline = warn_solids(&compose(&app, &mut atlas));
        // Pick a node the default demo camera actually shows, through
        // the same recenter + projection the overlay uses.
        let vp = layout.viewport;
        let main_vp = app.cosmic.camera.view_proj(vp.w / vp.h);
        let (index, center) = app
            .cosmic
            .web
            .nodes
            .iter()
            .find_map(|node| {
                let world = recenter(DVec3::from(node.position_mpc), app.cosmic.upload_origin);
                world_to_pixels(main_vp, world, vp).map(|at| (node.node_index, at))
            })
            .expect("default demo view must show at least one node");
        app.cosmic.player.target_node = Some(index);
        let items = compose(&app, &mut atlas);
        assert_eq!(
            warn_solids(&items),
            baseline + 4,
            "selected node gets one 4-rect ring"
        );
        let (cx, cy) = center;
        assert!(
            items.solids.iter().any(|(rect, color)| *color == C_WARN
                && *rect
                    == Rect {
                        x: cx - TARGET_RING_R,
                        y: cy - TARGET_RING_R,
                        w: 2.0 * TARGET_RING_R,
                        h: 1.5,
                    }),
            "ring top bar centers on the projected node"
        );
        // Clearing the target clears the ring.
        app.cosmic.player.target_node = None;
        assert_eq!(warn_solids(&compose(&app, &mut atlas)), baseline);
    }

    #[test]
    fn inspector_selected_ring_overlays_node() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let build =
            |app: &DebugApp, atlas: &mut GlyphAtlas| build_cosmic_web_ui(atlas, app, layout);
        let baseline = warn_solids(&build(&app, &mut atlas));
        // Pick a node strictly inside the default inspector viewport
        // (the ring clamps, so the test pins the unclamped center).
        let vp = layout.viewport;
        let view_proj = app.cosmic_inspector.view_proj(vp.w / vp.h);
        let (index, center) = app
            .cosmic
            .web
            .nodes
            .iter()
            .find_map(|node| {
                let world = Vec3::new(
                    node.position_mpc[0] as f32,
                    node.position_mpc[1] as f32,
                    node.position_mpc[2] as f32,
                );
                project_to_screen(world, view_proj, vp).and_then(|(sx, sy)| {
                    (sx >= vp.x && sx <= vp.x + vp.w && sy >= vp.y && sy <= vp.y + vp.h)
                        .then_some((node.node_index, (sx, sy)))
                })
            })
            .expect("default inspector view must show at least one node");
        app.cosmic_inspector.selected = Some(index);
        let items = build(&app, &mut atlas);
        assert_eq!(
            warn_solids(&items),
            baseline + 4,
            "inspected node gets one 4-rect ring"
        );
        let (cx, cy) = center;
        assert!(
            items.solids.iter().any(|(rect, color)| *color == C_WARN
                && *rect
                    == Rect {
                        x: cx - TARGET_RING_R,
                        y: cy - TARGET_RING_R,
                        w: 2.0 * TARGET_RING_R,
                        h: 1.5,
                    }),
            "ring top bar centers on the projected node"
        );
    }

    #[test]
    fn follow_marker_arrow_visible_and_oriented() {
        use game_debug::player_view::MoveKeys;

        let mut viewer = PlanetViewerState::with_values(1, 1.0);
        viewer.player.toggle();
        assert_eq!(viewer.player.mode(), CameraMode::Follow);
        let layout = ui::layout(1280.0, 720.0);
        let vp = layout.viewport;
        let player = &viewer.player;
        let main_vp = player.projection_matrix(vp.w / vp.h) * player.view_matrix();
        let pos = player.position();
        let origin = world_to_pixels(main_vp, pos, vp).expect("player projects");
        // Fresh heading is north: the arrow tip must project, readably
        // long, pointing up-screen (smaller y-down pixel).
        let tip = world_to_pixels(main_vp, sphere_tip_world(player, pos), vp)
            .expect("arrow tip projects in Follow");
        let len = (tip.0 - origin.0).hypot(tip.1 - origin.1);
        assert!(
            len >= 16.0,
            "arrow readable, got {len:.1}px origin={origin:?} tip={tip:?}"
        );
        assert!(
            tip.1 < origin.1,
            "north must be up-screen, origin={origin:?} tip={tip:?}"
        );
        // Turn right (D): facing east must swing toward screen right.
        viewer.player.set_keys(MoveKeys {
            east: true,
            ..MoveKeys::default()
        });
        let desired = visible_hemisphere(&viewer.mesh, viewer.player.position().to_array());
        viewer.player.update(0.5, &desired);
        let player = &viewer.player;
        let main_vp = player.projection_matrix(vp.w / vp.h) * player.view_matrix();
        let pos = player.position();
        let origin = world_to_pixels(main_vp, pos, vp).expect("player projects");
        let tip = world_to_pixels(main_vp, sphere_tip_world(player, pos), vp)
            .expect("turned tip projects");
        assert!(
            tip.0 > origin.0,
            "east must be right-screen, origin={origin:?} tip={tip:?}"
        );
    }

    #[test]
    fn first_person_hides_marker_and_looks_along_heading() {
        let mut viewer = PlanetViewerState::with_values(1, 1.0);
        viewer.player.toggle();
        viewer.player.cycle_camera();
        assert_eq!(viewer.player.mode(), CameraMode::FirstPerson);
        let layout = ui::layout(1280.0, 720.0);
        let vp = layout.viewport;
        let player = &viewer.player;
        let main_vp = player.projection_matrix(vp.w / vp.h) * player.view_matrix();
        // Own eyes sit in the eye plane: no dot by design (screen-up IS
        // the heading there).
        assert_eq!(world_to_pixels(main_vp, player.position(), vp), None);
        // Looking exactly along the facing: centered horizontally.
        let ahead = world_to_pixels(
            main_vp,
            player.eye() + player.facing() * viewer.radius * 0.5,
            vp,
        )
        .expect("view direction projects");
        assert!(
            (ahead.0 - (vp.x + vp.w / 2.0)).abs() < 2.0,
            "view must center on heading, got {ahead:?}"
        );
    }

    #[test]
    fn viewer_ui_shows_player_readout_when_active() {
        use game_debug::player_view::MoveKeys;

        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut viewer = PlanetViewerState::with_values(1, 1.0);
        viewer.player.toggle();
        viewer.player.set_keys(MoveKeys {
            east: true,
            ..MoveKeys::default()
        });
        let desired = visible_hemisphere(&viewer.mesh, viewer.player.position().to_array());
        viewer.player.update(0.05, &desired);
        let mut app = DebugApp::with_viewer(viewer);
        app.select_screen(Screen::Dimensions(WaypointId::Earth));
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let items = build_planet_ui(&mut atlas, &app, &SkySummary::default(), layout);
        let joined = items
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in ["SELECTION", "lon:", "lat:", "cam: follow"] {
            assert!(joined.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn unified_chrome_shows_topbar_dropdown_widget_and_settings() {
        let atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        for _ in 0..120 {
            app.fps.record(1.0 / 60.0);
        }
        app.select_screen(Screen::Dimensions(WaypointId::MilkyWay));
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let lh = atlas.line_height();
        let joined = |items: &UiItems| {
            items
                .texts
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Top bar: three items with the live breadcrumb.
        let mut top = UiItems::default();
        build_topbar(&mut top, lh, &app, layout);
        let top = joined(&top);
        for needle in ["GAME DEMO", "DIMENSIONS: milky-way", "SETTINGS"] {
            assert!(top.contains(needle), "top bar missing {needle}");
        }
        // Dropdown: ten waypoints with digits + active marker. Built by
        // `draw_main` as the last chrome layer (topmost surface).
        app.toggle_dropdown();
        let mut drop_atlas = GlyphAtlas::new(UI_PX);
        let mut drop = UiItems::default();
        build_dropdown(&mut drop, &mut drop_atlas, &app, layout.nav, None);
        // Panel + shadow + 4 border slabs + 9 separators + 1 selected row.
        assert_eq!(drop.solids.len(), 1 + 1 + 4 + 9 + 1);
        let drop_text = joined(&drop);
        for needle in [
            "cosmic-web",
            "solar-system",
            "interior",
            "active frame",
            "inactive · last state",
        ] {
            assert!(drop_text.contains(needle), "dropdown missing {needle}");
        }
        // Hovering a row adds exactly one highlight slab.
        let panel = ui::dropdown_panel(layout.nav);
        let row4 = ui::dropdown_row(panel, 4);
        let before = drop.solids.len();
        build_dropdown(
            &mut drop,
            &mut drop_atlas,
            &app,
            layout.nav,
            Some((row4.x + 5.0, row4.y + 5.0)),
        );
        assert_eq!(drop.solids.len() - before, 1 + 1 + 4 + 9 + 1 + 1);
        app.close_dropdown();
        // Widget FPS body: live numbers + sparkline label.
        app.select_widget_tab(WidgetTab::Fps);
        let mut widget = UiItems::default();
        build_widget(&mut widget, lh, &app, 1280.0, 720.0);
        let widget = joined(&widget);
        for needle in [
            "FPS",
            "Console",
            "Inspector",
            "FRAME HEALTH",
            "fps:",
            "60.0",
            "samples:",
            "120",
            "last 120 frames",
        ] {
            assert!(widget.contains(needle), "widget fps missing {needle}");
        }
        // Widget Console body: the real fly-to event feed.
        use game_debug::cosmic_player::CosmicEvent;

        app.cosmic
            .player
            .events
            .push(CosmicEvent::TargetSelected { node: 5 });
        app.sync_console();
        app.select_widget_tab(WidgetTab::Console);
        let mut widget = UiItems::default();
        build_widget(&mut widget, lh, &app, 1280.0, 720.0);
        let console = joined(&widget);
        assert!(console.contains("web/seed:1234/node:5"));
        // Widget Inspector body: journey summary.
        app.select_widget_tab(WidgetTab::Inspector);
        let mut widget = UiItems::default();
        build_widget(&mut widget, lh, &app, 1280.0, 720.0);
        let inspector = joined(&widget);
        for needle in ["layer:", "waypoint:", "fly-to:"] {
            assert!(inspector.contains(needle), "inspector missing {needle}");
        }
        // Corner strip lives inside the top bar: three toggles with
        // keys + the FPS value on the widget one.
        let mut strip = UiItems::default();
        build_corner_strip_in_bar(&mut strip, lh, &app, 1280.0);
        let strip = joined(&strip);
        for needle in ["DOCK-L [F9]", "DOCK-R [F10]", "DEV 60 [`]"] {
            assert!(strip.contains(needle), "corner strip missing {needle}");
        }
        // Transition pill: silent with no leg in flight…
        let mut items = UiItems::default();
        build_transition_strip(&mut items, lh, &app, 1280.0, 720.0);
        assert!(items.texts.is_empty());
        // …and showing live easing progress once fly-to commits.
        use game_engine::flight::{FlyToExec, Target, plan_fly_to};
        use game_engine::frames::FrameId;

        let home = DVec3::from(app.cosmic.web.home().position_mpc);
        let target = Target::new(FrameId::Cosmological, home).expect("finite target");
        let plan = plan_fly_to(
            FrameId::Cosmological,
            app.cosmic.player.position_mpc(),
            &target,
            0.0,
        )
        .expect("plannable leg");
        let mut exec = FlyToExec::new(plan);
        exec.commit().expect("commits once");
        app.cosmic.player.exec = Some(exec);
        app.cosmic.player.target_node = Some(app.cosmic.web.home_node);
        let mut items = UiItems::default();
        build_transition_strip(&mut items, lh, &app, 1280.0, 720.0);
        let pill = joined(&items);
        assert!(pill.contains("FLY-TO"), "pill missing fly-to: {pill}");
        assert!(pill.contains('%'), "pill missing progress: {pill}");
        // Settings Controls: every registry action as a labeled row.
        let mut settings = UiItems::default();
        build_settings_ui(&mut settings, lh, &app, layout);
        let settings = joined(&settings);
        for needle in [
            "Chrome",
            "Navigation",
            "Camera & walk",
            "Travel",
            "Toggle left dock [F9]",
            "Toggle right dock [F10]",
            "Game Demo tab [F1]",
            "Cycle twilight stage [F5]",
            "Arm/withdraw travel [T]",
        ] {
            assert!(settings.contains(needle), "settings missing {needle}");
        }
        // Placeholder tab: badge, never blank.
        let mut ph = UiItems::default();
        build_placeholder_ui(&mut ph, lh, &app, WaypointId::CosmicWeb, layout.viewport);
        let ph = joined(&ph);
        assert!(ph.contains("cosmic-web"));
        assert!(ph.contains("INACTIVE"));
        // Demo tab: HUD lines, no debug data.
        let mut demo = UiItems::default();
        build_demo_ui(&mut demo, lh, &mut app, layout.viewport);
        let demo = joined(&demo);
        for needle in ["GAME DEMO", "frame:", "time:", "soi:", "target:"] {
            assert!(demo.contains(needle), "demo missing {needle}");
        }
    }

    #[test]
    fn overlay_compose_isolates_dropdown_in_own_buffer() {
        // Regression test for the Settings-screen bleed-through: the
        // dropdown composes into its own `UiItems` (uploaded + drawn
        // after all other UI), so no content solid or text can share
        // its draw or cover it.
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        app.select_screen(Screen::Settings);
        app.toggle_dropdown();
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let lh = atlas.line_height();
        let mut items = UiItems::default();
        let mut drop = UiItems::default();
        build_topbar(&mut items, lh, &app, layout);
        build_settings_ui(&mut items, lh, &app, layout);
        compose_overlay_ui(
            &mut items,
            &mut drop,
            &mut atlas,
            &app,
            layout,
            (1280.0, 720.0),
            None,
        );
        // Main buffer keeps the Settings body and zero menu content.
        assert!(
            items.texts.iter().any(|run| run.text == "Chrome"),
            "settings header stays in the main buffer"
        );
        assert!(
            !items.texts.iter().any(|run| run.text == "cosmic-web"),
            "menu rows must not leak into the main buffer"
        );
        assert!(
            !items
                .solids
                .iter()
                .any(|(rect, _)| *rect == ui::dropdown_panel(layout.nav)),
            "menu panel must not leak into the main buffer"
        );
        // Menu buffer: shadow + panel + 4 border + 9 separators (no row
        // selected on the Settings screen, no hover without a cursor).
        let panel = ui::dropdown_panel(layout.nav);
        assert_eq!(drop.solids.len(), 1 + 1 + 4 + 9);
        assert_eq!(drop.solids[1].0, panel);
        // Menu texts: 10 rows x digit/name/status, starting at row 0
        // (`DROPDOWN_ORDER[0]` = cosmic-web with digit 1).
        assert_eq!(drop.texts.len(), 10 * 3);
        assert_eq!(drop.texts[0].text, "1");
        assert_eq!(drop.texts[1].text, "cosmic-web");
        assert!(drop.texts.iter().any(|run| run.text == "active frame ●"));
    }

    #[test]
    fn ui_vertices_cover_all_items() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        app.select_screen(Screen::Dimensions(WaypointId::Earth));
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let items = build_planet_ui(&mut atlas, &app, &SkySummary::default(), layout);
        let verts = ui_items_to_vertices(&items, &mut atlas);
        let text_quads: usize = items.texts.iter().map(|t| t.text.chars().count()).sum();
        assert_eq!(verts.len(), items.solids.len() * 6 + text_quads * 6);
        assert!((verts.len() as u64) < MAX_UI_VERTS);
    }

    #[test]
    fn settings_seed_editor_lives_in_left_dock() {
        let layout = ui::layout(1280.0, 720.0);
        let dock = settings_left_plan(layout.left, 19.0);
        // Full-width widgets inside the dock, flowing top-down.
        for rect in [dock.field, dock.load, dock.hint] {
            assert!(rect.x >= layout.left.x, "{rect:?}");
            assert!(
                rect.x + rect.w <= layout.left.x + layout.left.w + 1e-3,
                "{rect:?}"
            );
        }
        assert!(dock.header.y < dock.field.y);
        assert!(dock.field.y < dock.load.y);
        assert!(dock.load.y < dock.hint.y);
        // The built screen carries the editor plus the registry.
        let atlas = GlyphAtlas::new(UI_PX);
        let lh = atlas.line_height();
        let app = DebugApp::new();
        let mut items = UiItems::default();
        build_settings_ui(&mut items, lh, &app, layout);
        let texts = items
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "UNIVERSE",
            "Load [Enter]",
            "universe 1234",
            "Toggle left dock [F9]",
        ] {
            assert!(texts.contains(needle), "settings missing {needle}");
        }
        // The Milky Way dock keeps the seed read-only (no editor).
        let mut galaxy = UiItems::default();
        let galaxy_ui = build_galaxy_ui(&mut GlyphAtlas::new(UI_PX), &app, layout);
        galaxy.texts = galaxy_ui.texts;
        let dock_text = galaxy
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(dock_text.contains("seed 1234"), "dock keeps read-only seed");
        assert!(
            !dock_text.contains("Load"),
            "dock editor is gone: {dock_text}"
        );
    }

    #[test]
    fn settings_load_button_dims_on_invalid_seed() {
        let layout = ui::layout(1280.0, 720.0);
        let atlas = GlyphAtlas::new(UI_PX);
        let lh = atlas.line_height();
        let mut app = DebugApp::new();
        app.settings.seed_field.text = "abc".to_owned();
        let dock = settings_left_plan(layout.left, lh);
        let mut items = UiItems::default();
        build_settings_ui(&mut items, lh, &app, layout);
        assert!(
            items
                .solids
                .iter()
                .any(|(rect, color)| *rect == dock.load && *color == C_BTN_OFF),
            "invalid seed dims the Load button"
        );
    }

    #[test]
    fn loader_modal_overlays_seed_bar_and_step() {
        let mut app = DebugApp::new();
        app.loading = Some(LoadPlan::new(7, LoadSource::Panel));
        // One step ran: progress is 1/8 with the first step's label.
        let step = app
            .loading
            .as_mut()
            .expect("plan")
            .advance()
            .expect("first step");
        assert_eq!(step, LoadStep::Galaxy);
        let layout = app_layout(app.chrome, 1280.0, 720.0);
        let mut items = UiItems::default();
        let mut drop = UiItems::default();
        compose_overlay_ui(
            &mut items,
            &mut drop,
            &mut GlyphAtlas::new(UI_PX),
            &app,
            layout,
            (1280.0, 720.0),
            None,
        );
        // Topmost isolation (same rule as the dropdown): the modal
        // lives in the drop buffer, never in the main buffer.
        let main_texts = items
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !main_texts.contains("LOADING"),
            "modal must not leak into the main buffer"
        );
        let drop_texts = drop
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            drop_texts.contains("LOADING UNIVERSE · seed 7"),
            "modal titles the seed: {drop_texts}"
        );
        assert!(
            drop_texts.contains("13%") && drop_texts.contains(LoadStep::Galaxy.label()),
            "modal shows progress + step: {drop_texts}"
        );
        // Full-window dim + panel + track + fill ride the drop buffer.
        assert!(!drop.solids.is_empty());
        assert!(
            drop.solids.iter().any(|(rect, _)| *rect
                == Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1280.0,
                    h: 720.0,
                }),
            "dim covers the full window"
        );
    }
}
