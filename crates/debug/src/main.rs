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
//! read-only stats). A transition strip shows in-flight waypoint
//! transitions; the dev widget (`` ` ``, `F6`–`F8` sub-tabs
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
use game::journey::{Journey, JourneyEvent, Layer};
use game::transit::{SIM_DT_SECS, Transit, plan_cost};
use game_debug::actions::{Action, ActionGroup, DROPDOWN_ORDER, digit_for_dimension_index};
use game_debug::app::{App as DebugApp, ChromeState, Screen, ViewContent, WidgetTab};
use game_debug::fps::{FPS_SPARKLINE, FpsOverlay};
use game_debug::galaxy_map::{DEFAULT_GALAXY_SEED, GalaxyMapView, spectral_color, star_world};
use game_debug::params::{cell_count_hint, parse_radius, parse_subdivisions, subdiv_warning};
use game_debug::picking::{Ray, intersect_sphere, pick_cell, project_to_screen, ray_from_cursor};
use game_debug::planet_viewer::{DebugMode, PlanetViewerState};
use game_debug::player_view::{MoveKeys, PlayerViewState};
use game_debug::sky::{CatalogSky, SKY_ORDER, SkySummary};
use game_debug::system_map::{SystemMapView, arrival_for, orbit_ring_points, planet_slot};
use game_debug::text::GlyphAtlas;
use game_debug::ui::{self, Layout, Rect};
use game_engine::catalog::scheduler::SkyView;
use game_engine::render::{
    ExposureParams, FOV_Y, MAX_PITCH, OrbitCamera, ShaderKind, WORLD_TO_EQUATORIAL,
    compile_glsl_to_spirv, create_instance, device_score, log_physical_device,
    required_device_extensions, star_visibility, visible_hemisphere,
};
use game_engine::waypoints::WaypointId;
use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferToImageInfo, RenderPassBeginInfo,
    SubpassBeginInfo, SubpassContents,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags};
use vulkano::format::{ClearValue, Format};
use vulkano::image::sampler::{Filter, Sampler, SamplerCreateInfo};
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::instance::Instance;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::color_blend::{
    AttachmentBlend, ColorBlendAttachmentState, ColorBlendState,
};
use vulkano::pipeline::graphics::depth_stencil::{CompareOp, DepthState, DepthStencilState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::{CullMode, FrontFace, RasterizationState};
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
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
    "usage: game_debug [--headless] [--seed N]"
}

/// Parsed CLI: `--headless` runs the GPU-free checks; `--seed N`
/// opens the viewer (and seeds the headless map checks) on universe N.
#[derive(Debug)]
struct CliArgs {
    headless: bool,
    seed: Option<u64>,
}

fn parse_args(argv: &[String]) -> Result<CliArgs, String> {
    let mut headless = false;
    let mut seed = None;
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
            other => {
                return Err(format!(
                    "unknown argument {other:?}\n{usage}",
                    usage = usage()
                ));
            }
        }
    }
    Ok(CliArgs { headless, seed })
}

/// GPU-free viewer check: build the default mesh through the lib, print
/// stats, run the pick self-test, exit 0. Never touches
/// `VulkanLibrary` or `EventLoop`. `seed` overrides the universe the
/// map checks run on (`--seed N`).
fn run_headless(seed: Option<u64>) -> i32 {
    let viewer = PlanetViewerState::new();
    let mut debug_app = DebugApp::new();
    // Unified shell smoke: F2 opens Dimensions on the active
    // waypoint, digits pick dropdown entries, F-keys jump the
    // widget to a sub-tab.
    assert!(debug_app.select_top_by_fkey(2));
    assert_eq!(debug_app.screen, Screen::Dimensions(WaypointId::MilkyWay));
    assert!(debug_app.dropdown_open);
    assert!(debug_app.select_dimension_by_digit(1));
    assert_eq!(debug_app.screen, Screen::Dimensions(WaypointId::CosmicWeb));
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

// ---------------------------------------------------------------------------
// Pure UI builders (unit-tested below; the frame loop only converts).
// ---------------------------------------------------------------------------

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

/// Game Demo tab body: presentation-accurate player HUD lines for
/// the current journey layer — frame/waypoint, time state
/// (real-time: the debug shell owns no compression clock), and the
/// SOI/target readouts, hidden until travel features feed them.
/// Zero debug data by construction (the chrome builders run
/// separately and only when their toggles are on).
fn build_demo_ui(items: &mut UiItems, lh: f32, app: &DebugApp, area: Rect) {
    let mut rows = ui::PanelRows::new(area, 12.0);
    section_bar(items, rows.next(lh + 6.0, 4.0), "GAME DEMO");
    for (line, color) in [
        (
            format!(
                "frame:  {:?} · {} (L{})",
                app.journey.active_layer(),
                app.active_waypoint().name(),
                app.active_waypoint().number()
            ),
            C_TEXT,
        ),
        ("time:   real-time".to_owned(), C_TEXT),
        ("soi:    —".to_owned(), C_DIM),
        ("target: —".to_owned(), C_DIM),
    ] {
        text_row(items, lh, rows.next(lh, 6.0), line, color);
    }
    text_row(
        items,
        lh,
        rows.next(lh, 6.0),
        "WASD walk · P camera · E/T/Q travel".to_owned(),
        C_DIM,
    );
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

/// Settings tab body: Controls section rendering the registry.
fn build_settings_ui(items: &mut UiItems, lh: f32, area: Rect) {
    let plan = controls_plan(area, lh);
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
struct GalaxyLeftRects {
    field: Rect,
    load: Rect,
}

struct GalaxyLeftPlan {
    header: Rect,
    seed_row: Rect,
    stats_row: Rect,
    cam_row: Rect,
    cam_header: Rect,
    hints: [Rect; 6],
    seed_header: Rect,
    rects: GalaxyLeftRects,
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
    y += 4.0;
    let seed_header = bar(&mut y);
    let field_w = ((left.w - 2.0 * pad) * 0.62).max(0.0);
    let field = Rect {
        x: left.x + pad,
        y,
        w: field_w,
        h: lh + 6.0,
    };
    let load = Rect {
        x: field.x + field.w + 6.0,
        y,
        w: (left.x + left.w - pad - (field.x + field.w + 6.0)).max(0.0),
        h: lh + 6.0,
    };
    GalaxyLeftPlan {
        header,
        seed_row,
        stats_row,
        cam_row,
        cam_header,
        hints,
        seed_header,
        rects: GalaxyLeftRects { field, load },
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
        // ---- Left dock: SEED (runtime plumbing) ----
        section_bar(&mut items, plan.seed_header, "SEED");
        items.solid(plan.rects.field, C_FIELD_BG);
        text_row(
            &mut items,
            lh,
            plan.rects.field,
            galaxy.seed_field.text.clone(),
            C_TEXT,
        );
        let load_ok = galaxy.seed_field.text.parse::<u64>().is_ok();
        items.solid(plan.rects.load, if load_ok { C_BTN } else { C_BTN_OFF });
        items.text(
            "Load".to_owned(),
            plan.rects.load.x + 8.0,
            plan.rects.load.y + lh - 2.0,
            if load_ok { C_TEXT } else { C_DIM },
        );
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
}

/// Transition pill (S2): floating bottom-center overlay, drawn only
/// while a transition is in flight. Overlays content — layout never
/// shifts.
fn build_transition_strip(items: &mut UiItems, lh: f32, app: &DebugApp, win_w: f32, win_h: f32) {
    let Some(descriptor) = app.transitions.current else {
        return;
    };
    let pill = ui::transition_strip(win_w, win_h);
    items.solid(pill, C_PILL_BG);
    items.text(
        format!(
            "▶ {} → {} · {:.0}%",
            descriptor.leg.from.name(),
            descriptor.leg.to.name(),
            descriptor.progress * 100.0
        ),
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
    let body = Rect {
        x: widget.x + ui::DOCK_PAD,
        y: widget.y + ui::WIDGET_TAB_H + 4.0,
        w: (widget.w - 2.0 * ui::DOCK_PAD).max(0.0),
        h: (widget.h - ui::WIDGET_TAB_H - 12.0).max(0.0),
    };
    match app.widget_tab {
        WidgetTab::Fps => build_widget_fps(items, lh, app, body),
        WidgetTab::Console => build_widget_console(items, lh, app, body),
        WidgetTab::Inspector => build_widget_inspector(items, lh, app, body),
    }
}

/// Widget FPS body: the frame-health tab, drawn into the widget
/// body rect (it already lays out into any area).
fn build_widget_fps(items: &mut UiItems, lh: f32, app: &DebugApp, body: Rect) {
    build_fps_tab(items, lh, &app.fps, body);
}

/// Widget Console body: the transition-event log feed (newest last,
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
    for line in [
        format!("layer:      {:?}", app.journey.active_layer()),
        format!("waypoint:   {}", app.active_waypoint().name()),
        format!("transitions: {}", app.transitions.history_len()),
        format!("console:     {} lines", app.console.len()),
    ] {
        text_row(items, lh, rows.next(lh, 4.0), line, C_TEXT);
    }
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
    shaders: ShaderSet,
    fill_vertices: Subbuffer<[FillVertex]>,
    fill_indices: Subbuffer<[u32]>,
    line_vertices: Subbuffer<[LineVertex]>,
    map_vertices: Subbuffer<[MapVertex]>,
    system_points: Subbuffer<[MapVertex]>,
    system_lines: Subbuffer<[LineVertex]>,
    /// Catalog sky runtime (scheduler + loader + cache + fallback) and
    /// its Backdrop point buffer, drawn first in the Planet View.
    sky: CatalogSky,
    sky_vertices: Subbuffer<[MapVertex]>,
    /// Twilight demo stage (F5 cycles Day → Civil → Nautical →
    /// Astronomical): keys the sky-luminance input of the star
    /// fade-in (exposure-tone-mapping DoD-2 captures).
    twilight_stage: u8,
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
    render_pass: Arc<RenderPass>,
    pipelines: Pipelines,
    framebuffers: Vec<Arc<Framebuffer>>,
    depth_view: Arc<ImageView>,
    atlas_set: Option<Arc<DescriptorSet>>,
    atlas_set_extent: u32,
    last_cursor: Option<(f32, f32)>,
    recreate_swapchain: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
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

        // The windowed viewer opens at N=4: at the N=6 headless default
        // cells are subpixel (faces and pentagon sites unreadable), which
        // defeats the inspection goal of the default view. The `--headless`
        // path stays N=6 to cross-check the committed engine mesh hash
        // (update-2026-09-14-2008).
        let mut debug = DebugApp::with_viewer(PlanetViewerState::with_values(
            WINDOWED_SUBDIV,
            WINDOWED_RADIUS,
        ));
        // `--seed N` opens the viewer on universe N (galaxy + journey +
        // system star 0, Milky Way dimension tab) instead of the
        // default seed.
        if let Some(seed) = seed {
            debug.galaxy.regenerate(seed);
            debug.journey = Journey::new(seed);
            let star0 = debug.galaxy.galaxy.stars[0].clone();
            debug.system.load(seed, &star0);
            debug.select_screen(Screen::Dimensions(WaypointId::MilkyWay));
            debug.fx.notify(format!("Seed {seed} · --seed flag"));
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
        // Catalog sky over the cooker layout (`assets/catalog`); missing
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
            shaders,
            fill_vertices,
            fill_indices,
            line_vertices,
            map_vertices,
            system_points,
            system_lines,
            sky,
            sky_vertices,
            twilight_stage: 0,
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

    /// Route typed text into whichever field holds focus (seed field on
    /// the galaxy screen, subdiv/radius on the planet screen). Every
    /// `TextField::insert_char` guards on its own `focused` flag, so
    /// pushing to all three is safe — only the focused one accepts.
    /// Without this, the shortcut arms below (digits, U, R, E, Q, F,
    /// T, G/B) swallow keystrokes meant for the seed field: digits are
    /// the whole u64 seed alphabet, so typing a seed appears to do
    /// nothing.
    fn type_into_focused_fields(&mut self, text: &str) {
        for ch in text.chars() {
            self.debug.galaxy.seed_field.insert_char(ch);
            self.debug.viewer.subdiv_field.insert_char(ch);
            self.debug.viewer.radius_field.insert_char(ch);
        }
        self.debug.viewer.sync_slider_from_field();
    }

    /// Load a full universe for `seed` (seed plumbing, shared by the
    /// `--seed` flag, the panel Load button, Enter, and `R`): galaxy +
    /// buffers, journey reset, system back to star 0, screen back to
    /// the Milky Way dimension tab.
    fn load_galaxy_seed(&mut self, seed: u64) {
        self.debug.galaxy.regenerate(seed);
        self.debug.journey = Journey::new(seed);
        let star0 = self.debug.galaxy.galaxy.stars[0].clone();
        self.debug.system.load(seed, &star0);
        self.transit_acc = 0.0;
        self.refresh_map();
        self.refresh_system();
        self.debug
            .select_screen(Screen::Dimensions(WaypointId::MilkyWay));
        self.debug.fx.trigger_fade();
        self.debug.fx.notify(format!(
            "Seed {seed} · {} stars",
            self.debug.galaxy.galaxy.stars.len()
        ));
        tracing::info!(seed, "universe seed loaded");
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

    fn build_pipelines(&self, swapchain: &Arc<Swapchain>) -> (Arc<RenderPass>, Pipelines) {
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
        let pipelines = Pipelines {
            fill: build_fill_pipeline(&self.device, &self.shaders, &render_pass),
            line: build_line_pipeline(&self.device, &self.shaders, &render_pass),
            ui: build_ui_pipeline(&self.device, &self.shaders, &render_pass),
            map: build_map_pipeline(&self.device, &self.shaders, &render_pass),
        };
        (render_pass, pipelines)
    }
}

struct Pipelines {
    fill: Arc<GraphicsPipeline>,
    line: Arc<GraphicsPipeline>,
    ui: Arc<GraphicsPipeline>,
    map: Arc<GraphicsPipeline>,
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
                    image_usage: ImageUsage::COLOR_ATTACHMENT,
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
        let (render_pass, pipelines) = self.build_pipelines(&swapchain);
        let depth_view = create_depth_view(&self.memory_allocator, swapchain.image_extent());
        let framebuffers = window_size_dependent_setup(&images, &render_pass, &depth_view);
        WindowContext {
            window,
            swapchain,
            render_pass,
            pipelines,
            framebuffers,
            depth_view,
            atlas_set: None,
            atlas_set_extent: 0,
            last_cursor: None,
            recreate_swapchain: false,
            previous_frame_end: Some(sync::now(self.device.clone()).boxed()),
        }
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
            WindowEvent::Resized(_) => {
                if let Some(ctx) = self.main.as_mut() {
                    ctx.recreate_swapchain = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = (position.x as f32, position.y as f32);
                let content = self.debug.screen_content();
                if self.dragging_slider {
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
                    // (Settings Controls hold-to-press parity).
                    if self.mouse_walk.take().is_some() {
                        let viewer = &mut self.debug.viewer;
                        let mut keys = viewer.player.keys();
                        keys.north = false;
                        keys.south = false;
                        keys.west = false;
                        keys.east = false;
                        viewer.player.set_keys(keys);
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
                        return;
                    }
                }
                if self.debug.screen == Screen::Settings {
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
                // A staged seed load (galaxy Load button) applies after
                // this block: the block holds the viewer borrow, and the
                // loader needs `&mut self`.
                let mut pending_seed: Option<u64> = None;
                let regenerated = {
                    let viewer = &mut self.debug.viewer;
                    let lh = self.atlas.line_height();
                    let warn =
                        parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                    let checker = viewer.debug_mode == DebugMode::Checker;
                    // Content-specific left dock first.
                    match content {
                        Some(ViewContent::PlanetView) => {
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
                        // Seed widgets with galaxy content: click focuses
                        // the field, Load stages a seed (applied after
                        // the viewer borrow ends, below).
                        Some(ViewContent::GalaxyMap) => {
                            let plan = galaxy_left_plan(layout.left, lh);
                            self.debug.galaxy.seed_field.click(plan.rects.field, cx, cy);
                            if plan.rects.load.contains(cx, cy)
                                && let Ok(seed) = self.debug.galaxy.seed_field.text.parse::<u64>()
                            {
                                pending_seed = Some(seed);
                            }
                        }
                        _ => {}
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
                if let Some(seed) = pending_seed {
                    self.load_galaxy_seed(seed);
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
                    if content == Some(ViewContent::GalaxyMap) {
                        // Log zoom on the map: wheel-up (positive scroll)
                        // shrinks the camera distance.
                        let factor = (1.0 - 0.12 * scroll).max(0.05);
                        self.debug.galaxy.camera.zoom_by(factor);
                    } else if content == Some(ViewContent::SystemMap) {
                        let factor = (1.0 - 0.12 * scroll).max(0.05);
                        self.debug.system.camera.zoom_by(factor);
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
                        && !self.debug.galaxy.seed_field.focused;
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
                    // quits — closing the window exits.
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.debug.esc_unwind();
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        // Enter confirms the focused field: seed field
                        // loads the typed universe (or surfaces the miss),
                        // planet fields just unfocus.
                        if self.debug.galaxy.seed_field.focused {
                            match self.debug.galaxy.seed_field.text.parse::<u64>() {
                                Ok(seed) => self.load_galaxy_seed(seed),
                                Err(_) => self.debug.fx.notify(format!(
                                    "Invalid seed '{}'",
                                    self.debug.galaxy.seed_field.text
                                )),
                            }
                            self.debug.galaxy.seed_field.focused = false;
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
                        self.debug.galaxy.seed_field.backspace();
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
                            && !self.debug.galaxy.seed_field.focused
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
                            && !self.debug.galaxy.seed_field.focused
                        {
                            self.debug.viewer.player.toggle();
                            self.update_hover();
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyP) => {
                        // Player camera cycle (active player only).
                        // A focused field keeps the keystroke instead.
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && !self.debug.galaxy.seed_field.focused
                            && viewer.player.active
                        {
                            self.debug.viewer.player.cycle_camera();
                        } else if let Some(text) = text {
                            self.type_into_focused_fields(&text);
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        // R re-rolls the galaxy seed with galaxy content;
                        // planet content keeps R = Right preset.
                        let fields_free = {
                            let viewer = &self.debug.viewer;
                            !viewer.subdiv_field.focused
                                && !viewer.radius_field.focused
                                && !self.debug.galaxy.seed_field.focused
                        };
                        if fields_free {
                            let content = self.debug.screen_content();
                            if content == Some(ViewContent::GalaxyMap) {
                                self.load_galaxy_seed(self.debug.galaxy.seed + 1);
                            } else if content == Some(ViewContent::PlanetView) {
                                let radius = self.debug.viewer.radius;
                                snap_global_camera(&mut self.camera, GlobalPreset::Right, radius);
                                self.update_hover();
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
                                && !self.debug.galaxy.seed_field.focused
                        };
                        if fields_free
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
                                && !self.debug.galaxy.seed_field.focused
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
                                && !self.debug.galaxy.seed_field.focused
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
                                && !self.debug.galaxy.seed_field.focused
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
                                && !self.debug.galaxy.seed_field.focused
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
                            && !self.debug.galaxy.seed_field.focused
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
                        if self.debug.galaxy.seed_field.focused {
                            if let Some(text) = text {
                                for ch in text.chars() {
                                    self.debug.galaxy.seed_field.insert_char(ch);
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
                if self.debug.viewer.player.active {
                    self.debug.viewer.player.cycle_camera();
                }
            }
            Action::PresetPerspective => {
                self.snap_preset(GlobalPreset::Perspective);
            }
            Action::PresetTop => self.snap_preset(GlobalPreset::Top),
            Action::PresetBottom => self.snap_preset(GlobalPreset::Bottom),
            Action::PresetRight => self.snap_preset(GlobalPreset::Right),
            Action::RerollSeed => {
                if self.debug.screen_content() == Some(ViewContent::GalaxyMap) {
                    self.load_galaxy_seed(self.debug.galaxy.seed + 1);
                }
            }
            Action::TopDownSnap => {
                let content = self.debug.screen_content();
                if content == Some(ViewContent::GalaxyMap) {
                    self.debug.galaxy.camera.toggle_top_down();
                } else if content == Some(ViewContent::SystemMap) {
                    self.debug.system.camera.toggle_top_down();
                }
            }
            Action::TwilightCycle => {
                self.twilight_stage = (self.twilight_stage + 1) % 4;
            }
            Action::ShaderMode(i) => {
                if let Some(&mode) = DebugMode::ALL.get(i) {
                    self.debug.viewer.debug_mode = mode;
                }
            }
            Action::TravelOffer => self.travel_offer_toggle(),
            Action::TravelBegin => self.travel_begin(),
            Action::AscendLayer => self.ascend_layer(),
            Action::FocusToggle => self.focus_toggle(),
            Action::ConfirmField => self.confirm_focused_field(),
        }
    }

    /// Arm a mouse-held walk direction (player must be active; the
    /// release path clears it).
    fn hold_walk(&mut self, dir: WalkDir) {
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

    /// Confirm the focused field (Enter parity): the seed field loads
    /// the typed universe, planet fields just unfocus.
    fn confirm_focused_field(&mut self) {
        if self.debug.galaxy.seed_field.focused {
            match self.debug.galaxy.seed_field.text.parse::<u64>() {
                Ok(seed) => self.load_galaxy_seed(seed),
                Err(_) => self.debug.fx.notify(format!(
                    "Invalid seed '{}'",
                    self.debug.galaxy.seed_field.text
                )),
            }
            self.debug.galaxy.seed_field.focused = false;
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
                ctx.depth_view =
                    create_depth_view(&self.memory_allocator, ctx.swapchain.image_extent());
                ctx.framebuffers =
                    window_size_dependent_setup(&new_images, &ctx.render_pass, &ctx.depth_view);
                ctx.recreate_swapchain = false;
            }
        }

        let layout = app_layout(self.debug.chrome, win_w, win_h);
        let player_active = self.debug.viewer.player.active;
        let content = self.debug.screen_content();

        // Build frame UI (atlas insertions happen here) and sync the GPU
        // atlas before recording. The top bar + docks render inside the
        // content builders; overlay chrome (strip, corner, widget)
        // composes on top so it floats over every tab.
        let mut items = match self.debug.screen {
            Screen::GameDemo => {
                let mut demo = UiItems::default();
                build_topbar(&mut demo, self.atlas.line_height(), &self.debug, layout);
                match content {
                    Some(ViewContent::PlanetView) => {
                        let sky_summary = self.sky.summary();
                        let mut inner =
                            build_planet_ui(&mut self.atlas, &self.debug, &sky_summary, layout);
                        demo.solids.append(&mut inner.solids);
                        demo.tris.append(&mut inner.tris);
                        demo.texts.append(&mut inner.texts);
                    }
                    Some(ViewContent::GalaxyMap) => {
                        let mut inner = build_galaxy_ui(&mut self.atlas, &self.debug, layout);
                        demo.solids.append(&mut inner.solids);
                        demo.tris.append(&mut inner.tris);
                        demo.texts.append(&mut inner.texts);
                    }
                    Some(ViewContent::SystemMap) => {
                        let mut inner = build_system_ui(&mut self.atlas, &self.debug, layout);
                        demo.solids.append(&mut inner.solids);
                        demo.tris.append(&mut inner.tris);
                        demo.texts.append(&mut inner.texts);
                    }
                    None => {}
                }
                build_demo_ui(
                    &mut demo,
                    self.atlas.line_height(),
                    &self.debug,
                    layout.viewport,
                );
                demo
            }
            Screen::Dimensions(WaypointId::MilkyWay) => {
                build_galaxy_ui(&mut self.atlas, &self.debug, layout)
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
                build_settings_ui(&mut items, self.atlas.line_height(), layout.viewport);
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
        // Orbit backdrop (UMAP-020): the arrival target's atmosphere
        // color, scaled to a near-black space read; the default tint
        // otherwise. Descriptor palette straight to the frame.
        let backdrop: [f32; 4] = self
            .debug
            .viewer
            .arrival
            .as_ref()
            .map(|arrival| {
                let c = arrival.atmosphere.color;
                [c[0] * 0.07, c[1] * 0.07, c[2] * 0.07 + 0.02, 1.0]
            })
            .unwrap_or([0.02, 0.03, 0.08, 1.0]);
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
            Ok(future) => ctx.previous_frame_end = Some(future.boxed()),
            Err(VulkanError::OutOfDate) => {
                ctx.recreate_swapchain = true;
                ctx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
            }
            Err(error) => panic!("frame flush failed: {error}"),
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
        ] {
            if let Err(error) = compile_glsl_to_spirv(kind, source) {
                panic!("{what} must compile: {error}");
            }
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
        // Widget Console body: the transition-event log feed.
        app.sync_console();
        app.select_widget_tab(WidgetTab::Console);
        let mut widget = UiItems::default();
        build_widget(&mut widget, lh, &app, 1280.0, 720.0);
        let console = joined(&widget);
        assert!(console.contains("solar-system -> neighborhood"));
        // Widget Inspector body: journey summary.
        app.select_widget_tab(WidgetTab::Inspector);
        let mut widget = UiItems::default();
        build_widget(&mut widget, lh, &app, 1280.0, 720.0);
        let inspector = joined(&widget);
        for needle in ["layer:", "waypoint:", "transitions:"] {
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
        // Transition pill: shows the in-flight preview leg…
        let mut items = UiItems::default();
        build_transition_strip(&mut items, lh, &app, 1280.0, 720.0);
        let pill = joined(&items);
        assert!(pill.contains("solar-system → neighborhood"));
        // …and stays silent with no transition in flight.
        app.transitions.current = None;
        let mut items = UiItems::default();
        build_transition_strip(&mut items, lh, &app, 1280.0, 720.0);
        assert!(items.texts.is_empty());
        // Settings Controls: every registry action as a labeled row.
        let mut settings = UiItems::default();
        build_settings_ui(&mut settings, lh, layout.viewport);
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
        build_demo_ui(&mut demo, lh, &app, layout.viewport);
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
        build_settings_ui(&mut items, lh, layout.viewport);
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
}
