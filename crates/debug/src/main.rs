//! `game_debug` sphere viewer binary (`plans/debug-sphere-viewer`).
//!
//! - `--headless`: GPU-free path (CI-safe — never loads the Vulkan
//!   loader): builds the default N=6/R=1.0 viewer mesh through the
//!   `game_debug` lib and prints stats.
//! - Windowed (default): `winit` window + `vulkano` boot mirroring
//!   `game_tools` (Instance → Surface → Device → Swapchain, Vulkan 1.1
//!   cap), Sphere Viewer screen (orbit camera, filled dual-cell mesh,
//!   wireframe overlay, pentagon highlight, cell-chunk hover highlight +
//!   click-to-pin with panel readout, inputs panel, read-only stats) plus
//!   FPS/Console/Inspector placeholder screens, F1–F4/click nav with
//!   preserved viewer state.
//!
//! All screen logic lives in the `game_debug` lib (window- and GPU-free);
//! this binary owns the winit event loop, the three graphics pipelines
//! (fill, wireframe lines, UI quads), the depth buffer and the font-atlas
//! texture. `game_engine` and `game` are untouched.
//!
//! Usage: `game_debug [--headless]`.

use std::sync::Arc;

use game_debug::app::{App as DebugApp, Screen};
use game_debug::params::{cell_count_hint, parse_radius, parse_subdivisions, subdiv_warning};
use game_debug::picking::{
    Ray, flat_point_from_cursor, intersect_sphere, pick_cell, pick_flat_visible, ray_from_cursor,
};
use game_debug::sphere_viewer::{DebugMode, SphereViewerState, ViewFocus};
use game_debug::text::GlyphAtlas;
use game_debug::ui::{self, Layout, Rect};
use game_engine::render::{
    OrbitCamera, ShaderKind, compile_glsl_to_spirv, create_instance, device_score,
    log_physical_device, required_device_extensions,
};
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
layout(push_constant) uniform PushConstants {
    mat4 mvp;
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

const FLAT_VERT: &str = r##"#version 450
layout(location = 0) in vec2 uv_pos;
layout(location = 1) in vec3 normal;
layout(location = 2) in float tint;
layout(location = 3) in vec2 dbg;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float highlight;
    float mode;
    float density;
    float seams_on;
    float hover_cell;
    float pin_cell;
} pc;
layout(location = 0) out vec3 v_normal;
layout(location = 1) flat out float v_tint;
layout(location = 2) out vec2 v_uv;
layout(location = 3) flat out float v_seam;
layout(location = 4) flat out float v_island;
layout(location = 5) flat out float v_hover;
layout(location = 6) flat out float v_pin;
void main() {
    gl_Position = pc.mvp * vec4(uv_pos, 0.0, 1.0);
    v_normal = normal;
    v_tint = tint * pc.highlight;
    v_uv = uv_pos;
    v_seam = dbg.x;
    v_island = dbg.y;
    // No chunk hover in the flat UV net (v1): the shared fragment shader
    // still declares these inputs, so they must be written.
    v_hover = 0.0;
    v_pin = 0.0;
}"##;

const CHUNK_FLAT_VERT: &str = r##"#version 450
layout(location = 0) in vec2 pos;
layout(location = 1) in float chunk_id;
layout(location = 2) in float island;
layout(location = 3) in float tint;
layout(location = 4) in float seam;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float highlight;
    float mode;
    float density;
    float seams_on;
    float hover_cell;
    float pin_cell;
} pc;
layout(location = 0) out vec3 v_normal;
layout(location = 1) flat out float v_tint;
layout(location = 2) out vec2 v_uv;
layout(location = 3) flat out float v_seam;
layout(location = 4) flat out float v_island;
layout(location = 5) flat out float v_hover;
layout(location = 6) flat out float v_pin;
void main() {
    gl_Position = pc.mvp * vec4(pos, 0.0, 1.0);
    // The map faces the viewer: constant forward normal, so Lit shades
    // every chunk evenly and the debug modes read like the UV net.
    v_normal = vec3(0.0, 0.0, 1.0);
    v_tint = tint * pc.highlight;
    v_uv = pos;
    v_seam = seam;
    v_island = island;
    // Per-chunk hover/pin: every fan vertex carries its cell id (float
    // comparison — ids stay exactly representable in f32; -1.0 = none).
    v_hover = (abs(chunk_id - pc.hover_cell) < 0.5) ? 1.0 : 0.0;
    v_pin = (abs(chunk_id - pc.pin_cell) < 0.5) ? 1.0 : 0.0;
}"##;

const FLAT_LINE_VERT: &str = r##"#version 450
layout(location = 0) in vec2 uv_pos;
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float highlight;
    float mode;
    float density;
    float seams_on;
    // Unused here (no chunk hover on UV wireframe, v1) — declared so the
    // shared FillPush block stays one size across all pipelines.
    float hover_cell;
    float pin_cell;
} pc;
void main() {
    gl_Position = pc.mvp * vec4(uv_pos, 0.0, 1.0);
}"##;

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
/// island id). Built at upload from `SphereViewerState`.
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

/// Flat UV-view vertex: icosa-net position plus the same debug varyings.
/// Shares the fill index buffer (identical topology).
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct FlatVertex {
    #[format(R32G32_SFLOAT)]
    uv_pos: [f32; 2],
    #[format(R32G32B32_SFLOAT)]
    normal: [f32; 3],
    #[format(R32_SFLOAT)]
    tint: f32,
    #[format(R32G32_SFLOAT)]
    dbg: [f32; 2],
}

/// Flat-view wireframe vertex: icosa-net position only.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct FlatLineVertex {
    #[format(R32G32_SFLOAT)]
    uv_pos: [f32; 2],
}

/// Flat chunk-map fill vertex: 2D position plus per-chunk flags. Every
/// vertex carries its cell id, so the whole polygon highlights.
#[derive(BufferContents, Vertex, Clone, Copy, Debug)]
#[repr(C)]
struct ChunkFlatVertex {
    #[format(R32G32_SFLOAT)]
    pos: [f32; 2],
    #[format(R32_SFLOAT)]
    chunk_id: f32,
    #[format(R32_SFLOAT)]
    island: f32,
    #[format(R32_SFLOAT)]
    tint: f32,
    #[format(R32_SFLOAT)]
    seam: f32,
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

/// Fill push constants: MVP + pentagon-highlight flag + debug-mode id +
/// checker density + seam overlay flag + hovered/pinned chunk ids
/// (−1.0 = none; 88 B < 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct FillPush {
    mvp: [[f32; 4]; 4],
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

// ---------------------------------------------------------------------------
// Args + headless.
// ---------------------------------------------------------------------------

fn usage() -> &'static str {
    "usage: game_debug [--headless]"
}

fn parse_args(argv: &[String]) -> Result<bool, String> {
    let mut headless = false;
    for arg in argv.iter().skip(1) {
        match arg.as_str() {
            "--headless" => headless = true,
            "--help" | "-h" => return Err(usage().to_owned()),
            other => {
                return Err(format!(
                    "unknown argument {other:?}\n{usage}",
                    usage = usage()
                ));
            }
        }
    }
    Ok(headless)
}

/// GPU-free viewer check: build the default mesh through the lib, print
/// stats, run the pick self-test, exit 0. Never touches
/// `VulkanLibrary` or `EventLoop`.
fn run_headless() -> i32 {
    let viewer = SphereViewerState::new();
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
        "uv_islands={} uv_seam_verts={} uv_flat_tris={} uv_flat_verts={} focus={:?} mode={:?}",
        islands.len(),
        seams,
        viewer.flat.indices.len() / 3,
        viewer.flat.uv.len(),
        viewer.focus,
        viewer.debug_mode,
    );
    println!("pick_selftest=chunk{} ok", picked.index());
    // Chunk-flat self-test: the north-pole view loads the fully-inside
    // chunks in unit range (strict subset — partial rim cells are
    // dropped); flat picking resolves a visible center.
    assert!(!viewer.chunk_flat_cells.is_empty(), "hemisphere non-empty");
    assert!(
        viewer.chunk_flat_cells.len() < stats.cells,
        "hemisphere is a strict subset"
    );
    assert!(
        viewer
            .chunk_flat_vertices
            .iter()
            .all(|v| (0.0..=1.0).contains(&v.position[0]) && (0.0..=1.0).contains(&v.position[1])),
        "flat verts in unit range"
    );
    let first = viewer.chunk_flat_cells[0];
    let flat_picked = pick_flat_visible(
        &viewer.mesh,
        &viewer.chunk_flat_cells,
        &viewer.chunk_flat_centers,
        viewer.chunk_flat_centers[0],
    );
    assert_eq!(
        flat_picked.index(),
        first,
        "flat pick at a visible center must resolve it"
    );
    println!(
        "chunk_flat_cells={} chunk_flat_verts={} chunk_flat_tris={} chunk_flat_pick=chunk{} ok",
        viewer.chunk_flat_cells.len(),
        viewer.chunk_flat_vertices.len(),
        viewer.chunk_flat_indices.len() / 3,
        flat_picked.index(),
    );
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

/// Aspect-fit square MVP for flat UV views: maps `[0,1]²` (v=0 top,
/// y-down like the UI) into the largest centered square of `vp`,
/// outputting Vulkan NDC. Stretching would lie about distortion, so the
/// long axis letterboxes instead.
fn flat_mvp(vp: Rect) -> [[f32; 4]; 4] {
    let aspect = vp.w / vp.h;
    // NDC: x = sx*u + tx, y = sy*v + ty.
    let (sx, tx, sy, ty) = if aspect >= 1.0 {
        (2.0 / aspect, -1.0 / aspect, -2.0, 1.0)
    } else {
        (2.0, -1.0, -2.0 * aspect, aspect)
    };
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [tx, ty, 0.0, 1.0],
    ]
}

/// Widget rects inside the inputs panel.
struct PanelRects {
    thumb: Rect,
    thumb_caption: Rect,
    swap_button: Rect,
    shader_button: Rect,
    density_track: Rect,
    subdiv_field: Rect,
    subdiv_track: Rect,
    radius_field: Rect,
    regen_button: Rect,
    wire_box: Rect,
    pent_box: Rect,
    seam_box: Rect,
    uvwire_box: Rect,
}

/// Full panel row plan: widget rects + label/text rows in draw order.
/// Built with a single cursor, so hit-testing and drawing always agree.
/// The UV thumb comes first so it matches [`ui::uv_thumb_rect`]
/// (rows start at `panel.y + pad`); `warn` reserves the extra above-N=6
/// warning row.
struct PanelPlan {
    rects: PanelRects,
    uv_header: Rect,
    density_label: Rect,
    inputs_header: Rect,
    subdiv_label: Rect,
    subdiv_hint: Rect,
    warn_line: Option<Rect>,
    radius_label: Rect,
    radius_hint: Rect,
    wire_label: Rect,
    pent_label: Rect,
    seam_label: Rect,
    uvwire_label: Rect,
    chunk_header: Rect,
    chunk_lines: [Rect; 4],
    stats_header: Rect,
    stat_lines: [Rect; 6],
}

fn panel_plan(panel: Rect, lh: f32, warn: bool) -> PanelPlan {
    let mut rows = ui::PanelRows::new(panel, 8.0);
    let thumb = rows.next(ui::UV_THUMB_H, 4.0);
    debug_assert_eq!(
        thumb,
        ui::uv_thumb_rect(panel, 8.0),
        "thumb must match the shared hit-test rect"
    );
    let thumb_caption = rows.next(lh, 4.0);
    let swap_button = rows.next(28.0, 6.0);
    let uv_header = rows.next(lh, 4.0);
    let shader_button = rows.next(28.0, 4.0);
    let density_label = rows.next(lh, 4.0);
    let density_track = rows.next(20.0, 6.0);
    let inputs_header = rows.next(lh, 4.0);
    let subdiv_label = rows.next(lh, 4.0);
    let subdiv_field = rows.next(24.0, 4.0);
    let subdiv_track = rows.next(20.0, 4.0);
    let subdiv_hint = rows.next(lh, 4.0);
    let warn_line = warn.then(|| rows.next(lh, 4.0));
    let radius_label = rows.next(lh, 4.0);
    let radius_field = rows.next(24.0, 4.0);
    let radius_hint = rows.next(lh, 4.0);
    let regen_button = rows.next(28.0, 6.0);
    let wire_label = rows.next(lh, 4.0);
    let pent_label = rows.next(lh, 4.0);
    let seam_label = rows.next(lh, 4.0);
    let uvwire_label = rows.next(lh, 4.0);
    let chunk_header = rows.next(lh, 4.0);
    let chunk_lines = [
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
    ];
    let stats_header = rows.next(lh, 4.0);
    let stat_lines = [
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
        rows.next(lh, 4.0),
    ];
    let check_box = |row: Rect| Rect {
        x: row.x,
        y: row.y + (lh - 16.0) / 2.0,
        w: 16.0,
        h: 16.0,
    };
    PanelPlan {
        rects: PanelRects {
            thumb,
            thumb_caption,
            swap_button,
            shader_button,
            density_track,
            subdiv_field,
            subdiv_track,
            radius_field,
            regen_button,
            wire_box: check_box(wire_label),
            pent_box: check_box(pent_label),
            seam_box: check_box(seam_label),
            uvwire_box: check_box(uvwire_label),
        },
        uv_header,
        density_label,
        inputs_header,
        subdiv_label,
        subdiv_hint,
        warn_line,
        radius_label,
        radius_hint,
        wire_label,
        pent_label,
        seam_label,
        uvwire_label,
        chunk_header,
        chunk_lines,
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

/// Frame UI: solid rects + text runs (converted to vertices later).
#[derive(Default)]
struct UiItems {
    solids: Vec<(Rect, Color)>,
    texts: Vec<UiText>,
}

impl UiItems {
    fn solid(&mut self, rect: Rect, color: Color) {
        self.solids.push((rect, color));
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

/// Nav bar shared by every screen.
fn build_nav(items: &mut UiItems, layout: Layout, active: Screen, lh: f32) {
    items.solid(layout.nav, C_NAV_BG);
    for (i, screen) in Screen::ALL.iter().enumerate() {
        let rect = ui::nav_button(layout.nav, i);
        if *screen == active {
            items.solid(rect, C_TAB_ACTIVE);
        }
        items.text(
            screen.title().to_owned(),
            rect.x + 12.0,
            rect.y + (rect.h + lh) / 2.0 - 3.0,
            C_TEXT,
        );
    }
}

/// Sphere Viewer panel (3D draws separately).
fn build_viewer_ui(atlas: &mut GlyphAtlas, viewer: &SphereViewerState, layout: Layout) -> UiItems {
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_nav(&mut items, layout, Screen::SphereViewer, lh);

    let warn = parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
    let plan = panel_plan(layout.panel, lh, warn);
    let rects = &plan.rects;
    items.solid(layout.panel, C_PANEL_BG);

    let text_row = |items: &mut UiItems, row: Rect, text: String, color: Color| {
        items.text(text, row.x, row.y + lh - 4.0, color);
    };

    // UV preview thumb: the flat/3D view lives in the GPU pass (drawn
    // into this rect); the UI only frames it and captions the swap.
    items.solid(rects.thumb, C_FIELD_BG);
    items.solid(
        Rect {
            x: rects.thumb.x - 1.0,
            y: rects.thumb.y - 1.0,
            w: rects.thumb.w + 2.0,
            h: 1.0,
        },
        C_TRACK,
    );
    text_row(
        &mut items,
        rects.thumb_caption,
        viewer.thumb_label().to_owned(),
        C_DIM,
    );
    items.solid(rects.swap_button, C_BTN);
    items.text(
        "Swap view (U)".to_owned(),
        rects.swap_button.x + 12.0,
        rects.swap_button.y + 19.0,
        C_TEXT,
    );
    text_row(&mut items, plan.uv_header, "UV DEBUG".to_owned(), C_DIM);
    items.solid(rects.shader_button, C_BTN);
    items.text(
        format!(
            "Shader: {} ({}/{})",
            viewer.debug_mode.title(),
            viewer.debug_mode.index() + 1,
            DebugMode::ALL.len(),
        ),
        rects.shader_button.x + 12.0,
        rects.shader_button.y + 19.0,
        C_TEXT,
    );
    text_row(
        &mut items,
        plan.density_label,
        format!("Checker density: {}", viewer.checker_density),
        C_DIM,
    );
    items.solid(rects.density_track, C_TRACK);
    items.solid(
        Rect {
            x: viewer.density_slider.knob_x(rects.density_track) - 5.0,
            y: rects.density_track.y + 1.0,
            w: 10.0,
            h: rects.density_track.h - 2.0,
        },
        C_KNOB,
    );
    text_row(&mut items, plan.inputs_header, "INPUTS".to_owned(), C_DIM);
    text_row(
        &mut items,
        plan.subdiv_label,
        "Subdivisions".to_owned(),
        C_DIM,
    );

    // Subdivisions field + slider + live cost hint.
    items.solid(rects.subdiv_field, C_FIELD_BG);
    items.text(
        viewer.subdiv_field.text.clone(),
        rects.subdiv_field.x + 6.0,
        rects.subdiv_field.y + 17.0,
        C_TEXT,
    );
    items.solid(rects.subdiv_track, C_TRACK);
    items.solid(
        Rect {
            x: viewer.subdiv_slider.knob_x(rects.subdiv_track) - 5.0,
            y: rects.subdiv_track.y + 1.0,
            w: 10.0,
            h: rects.subdiv_track.h - 2.0,
        },
        C_KNOB,
    );
    match parse_subdivisions(&viewer.subdiv_field.text) {
        Ok(n) => text_row(
            &mut items,
            plan.subdiv_hint,
            format!("→ {} cells", fmt_int(cell_count_hint(n))),
            C_DIM,
        ),
        Err(error) => text_row(&mut items, plan.subdiv_hint, error.hint().to_owned(), C_ERR),
    }
    if let Some(warn_row) = plan.warn_line {
        text_row(
            &mut items,
            warn_row,
            "above N=6: seconds per regen".to_owned(),
            C_WARN,
        );
    }
    text_row(&mut items, plan.radius_label, "Radius".to_owned(), C_DIM);

    // Radius field + validation hint.
    items.solid(rects.radius_field, C_FIELD_BG);
    items.text(
        viewer.radius_field.text.clone(),
        rects.radius_field.x + 6.0,
        rects.radius_field.y + 17.0,
        C_TEXT,
    );
    if let Err(error) = parse_radius(&viewer.radius_field.text) {
        text_row(&mut items, plan.radius_hint, error.hint().to_owned(), C_ERR);
    }

    // Regenerate (dimmed while invalid; clicks ignored then).
    let ok = viewer.can_regenerate();
    items.solid(rects.regen_button, if ok { C_BTN } else { C_BTN_OFF });
    items.text(
        "Regenerate".to_owned(),
        rects.regen_button.x + 12.0,
        rects.regen_button.y + 19.0,
        if ok { C_TEXT } else { C_DIM },
    );

    // Display toggles.
    for (box_rect, label_row, label, checked) in [
        (
            rects.wire_box,
            plan.wire_label,
            "Wireframe",
            viewer.wireframe,
        ),
        (
            rects.pent_box,
            plan.pent_label,
            "Pentagons",
            viewer.pentagons,
        ),
        (rects.seam_box, plan.seam_label, "Seams", viewer.seams),
        (
            rects.uvwire_box,
            plan.uvwire_label,
            "UV wire",
            viewer.wire_on_uv,
        ),
    ] {
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

    // Chunk hover/pin readout (cell-chunks): the pin wins over a
    // fleeting hover; nothing selected shows em-dashes.
    text_row(&mut items, plan.chunk_header, "CHUNK".to_owned(), C_DIM);
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
    for (row, line) in plan
        .chunk_lines
        .iter()
        .zip([chunk_line, type_line, neigh_line, state_line])
    {
        text_row(&mut items, *row, line, C_TEXT);
    }

    // Read-only stats.
    text_row(&mut items, plan.stats_header, "STATS".to_owned(), C_DIM);
    let stats = &viewer.stats;
    for (row, line) in plan.stat_lines.iter().zip([
        format!("cells:     {}", fmt_int(stats.cells)),
        format!("corners:    {}", fmt_int(stats.corners)),
        format!("pentagons:  {}", stats.pentagons),
        format!("hash:       {}", stats.hash8),
        format!("gen:        {:.1} ms", stats.gen_ms),
        format!("view:       {}", viewer.debug_mode.title()),
    ]) {
        text_row(&mut items, *row, line, C_TEXT);
    }
    items
}

/// Placeholder screen: nav bar + centered title + body.
fn build_placeholder_ui(atlas: &mut GlyphAtlas, screen: Screen, layout: Layout) -> UiItems {
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_nav(&mut items, layout, screen, lh);
    let area = layout.viewport;
    for (text, color, dy) in [
        (screen.title().to_owned(), C_TEXT, -lh),
        (
            screen.placeholder_body().unwrap_or("").to_owned(),
            C_DIM,
            lh,
        ),
    ] {
        let (w, _) = atlas.measure(&text);
        items.text(
            text,
            area.x + (area.w - w) / 2.0,
            area.y + area.h / 2.0 + dy,
            color,
        );
    }
    items
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

fn build_fill_pipeline(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(device, ShaderKind::Vertex, FILL_VERT, "fill vertex");
    let fs_module = compile_shader(device, ShaderKind::Fragment, FILL_FRAG, "fill fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
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
                // `OrbitCamera::projection_matrix` outputs Y-down NDC
                // (glam `vulkan::perspective`): the baked-in Y-flip
                // mirrors triangle winding in framebuffer space, where
                // Vulkan classifies front faces — so the mesh's
                // CCW-outward fans (`fill_faces_point_outward`) land as
                // CW. Clockwise front keeps the near-side outward faces
                // and culls the far side; the CCW default culled the
                // near side and rendered the sphere inside-out
                // (issue-2026-09-14-2113).
                front_face: FrontFace::Clockwise,
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
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(device, ShaderKind::Vertex, LINE_VERT, "line vertex");
    let fs_module = compile_shader(device, ShaderKind::Fragment, LINE_FRAG, "line fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
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

fn build_flat_pipeline(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(device, ShaderKind::Vertex, FLAT_VERT, "flat vertex");
    let fs_module = compile_shader(device, ShaderKind::Fragment, FILL_FRAG, "flat fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
    let vertex_input_state = FlatVertex::per_vertex()
        .definition(&vs)
        .expect("flat vertex layout must match shader");
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
                // UV-space winding is arbitrary per island: never cull.
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            depth_stencil_state: Some(DepthStencilState {
                // Flat islands are coplanar by construction and seam
                // triangles legitimately cross over other islands: never
                // write depth here or cuts z-fight (issue-2026-09-15-0648).
                // Draw order decides overdraw; the UI pass draws after.
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
    .expect("flat graphics pipeline must create")
}

fn build_chunk_flat_pipeline(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(
        device,
        ShaderKind::Vertex,
        CHUNK_FLAT_VERT,
        "chunk flat vertex",
    );
    let fs_module = compile_shader(device, ShaderKind::Fragment, FILL_FRAG, "flat fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
    let vertex_input_state = ChunkFlatVertex::per_vertex()
        .definition(&vs)
        .expect("chunk flat vertex layout must match shader");
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
                // Polygon winding is CCW in layout space but the y-down
                // flat projection mirrors it: never cull.
                cull_mode: CullMode::None,
                ..Default::default()
            }),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            depth_stencil_state: Some(DepthStencilState {
                // Coplanar polygons by construction: never write depth
                // here or neighbors z-fight (same rule as the UV net).
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
    .expect("chunk flat graphics pipeline must create")
}

fn build_flat_line_pipeline(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(
        device,
        ShaderKind::Vertex,
        FLAT_LINE_VERT,
        "flat line vertex",
    );
    let fs_module = compile_shader(device, ShaderKind::Fragment, LINE_FRAG, "line fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
    let vertex_input_state = FlatLineVertex::per_vertex()
        .definition(&vs)
        .expect("flat line vertex layout must match shader");
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
                // Same no-write rule as the flat fill: coplanar UV lines
                // must not fight the islands underneath.
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
    .expect("flat line graphics pipeline must create")
}

fn build_ui_pipeline(device: &Arc<Device>, render_pass: &Arc<RenderPass>) -> Arc<GraphicsPipeline> {
    let vs_module = compile_shader(device, ShaderKind::Vertex, UI_VERT, "ui vertex");
    let fs_module = compile_shader(device, ShaderKind::Fragment, UI_FRAG, "ui fragment");
    let vs = vs_module.entry_point("main").expect("vertex entry point");
    let fs = fs_module.entry_point("main").expect("fragment entry point");
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

fn upload_fill(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
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

fn upload_flat(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
) -> (Subbuffer<[FlatVertex]>, Subbuffer<[u32]>) {
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
        viewer.flat.uv.iter().enumerate().map(|(i, uv)| {
            let src = viewer.fill_vertices[viewer.flat.source[i] as usize];
            FlatVertex {
                uv_pos: *uv,
                normal: src.normal,
                tint: src.tint,
                dbg: [
                    if viewer.flat.seam[i] { 1.0 } else { 0.0 },
                    viewer.flat.island[i] as f32,
                ],
            }
        }),
    )
    .expect("flat vertex buffer upload must succeed");
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
        viewer.flat.indices.iter().copied(),
    )
    .expect("flat index buffer upload must succeed");
    (vertices, indices)
}

fn upload_flat_lines(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
) -> Subbuffer<[FlatLineVertex]> {
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
        viewer
            .uv_lines
            .iter()
            .map(|uv| FlatLineVertex { uv_pos: *uv }),
    )
    .expect("flat wireframe vertex buffer upload must succeed")
}

fn upload_chunk_flat(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
) -> (Subbuffer<[ChunkFlatVertex]>, Subbuffer<[u32]>) {
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
        viewer.chunk_flat_vertices.iter().map(|v| ChunkFlatVertex {
            pos: v.position,
            chunk_id: v.chunk_id,
            island: v.island,
            tint: v.tint,
            seam: v.seam,
        }),
    )
    .expect("chunk flat vertex buffer upload must succeed");
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
        viewer.chunk_flat_indices.iter().copied(),
    )
    .expect("chunk flat index buffer upload must succeed");
    (vertices, indices)
}

fn upload_chunk_flat_lines(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
) -> Subbuffer<[FlatLineVertex]> {
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
        viewer
            .chunk_flat_wire
            .iter()
            .map(|pos| FlatLineVertex { uv_pos: *pos }),
    )
    .expect("chunk flat wireframe vertex buffer upload must succeed")
}

fn upload_lines(
    allocator: &Arc<StandardMemoryAllocator>,
    viewer: &SphereViewerState,
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
    fill_vertices: Subbuffer<[FillVertex]>,
    fill_indices: Subbuffer<[u32]>,
    line_vertices: Subbuffer<[LineVertex]>,
    flat_vertices: Subbuffer<[FlatVertex]>,
    flat_indices: Subbuffer<[u32]>,
    flat_lines: Subbuffer<[FlatLineVertex]>,
    chunk_flat_vertices: Subbuffer<[ChunkFlatVertex]>,
    chunk_flat_indices: Subbuffer<[u32]>,
    chunk_flat_lines: Subbuffer<[FlatLineVertex]>,
    atlas_image: Option<Arc<Image>>,
    atlas_set: Option<Arc<DescriptorSet>>,
    dragging_orbit: bool,
    dragging_slider: bool,
    dragging_density: bool,
    last_cursor: Option<(f32, f32)>,
    /// Cursor position at left-button press (main-viewport sphere presses
    /// only): release within [`CLICK_MAX_DRAG_PX`] of it counts as a click
    /// (chunk pin) rather than an orbit drag.
    press_cursor: Option<(f32, f32)>,
    rcx: Option<RenderContext>,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    depth_view: Arc<ImageView>,
    fill_pipeline: Arc<GraphicsPipeline>,
    line_pipeline: Arc<GraphicsPipeline>,
    flat_pipeline: Arc<GraphicsPipeline>,
    flat_line_pipeline: Arc<GraphicsPipeline>,
    chunk_flat_pipeline: Arc<GraphicsPipeline>,
    ui_pipeline: Arc<GraphicsPipeline>,
    recreate_swapchain: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
}

impl ViewerApp {
    fn new(event_loop: &EventLoop<()>) -> Self {
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
        let debug = DebugApp::with_viewer(SphereViewerState::with_values(
            WINDOWED_SUBDIV,
            WINDOWED_RADIUS,
        ));
        let viewer = &debug.viewer;
        tracing::info!(
            subdivisions = viewer.subdiv,
            radius = viewer.radius,
            cells = viewer.stats.cells,
            hash8 = viewer.stats.hash8.as_str(),
            "sphere viewer mesh generated",
        );
        let (fill_vertices, fill_indices) = upload_fill(&memory_allocator, viewer);
        let line_vertices = upload_lines(&memory_allocator, viewer);
        let (flat_vertices, flat_indices) = upload_flat(&memory_allocator, viewer);
        let flat_lines = upload_flat_lines(&memory_allocator, viewer);
        let (chunk_flat_vertices, chunk_flat_indices) =
            upload_chunk_flat(&memory_allocator, viewer);
        let chunk_flat_lines = upload_chunk_flat_lines(&memory_allocator, viewer);
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
            fill_vertices,
            fill_indices,
            line_vertices,
            flat_vertices,
            flat_indices,
            flat_lines,
            chunk_flat_vertices,
            chunk_flat_indices,
            chunk_flat_lines,
            atlas_image: None,
            atlas_set: None,
            dragging_orbit: false,
            dragging_slider: false,
            dragging_density: false,
            last_cursor: None,
            press_cursor: None,
            rcx: None,
        }
    }

    /// Rebuild GPU mesh buffers + reframe the camera after Regenerate.
    fn refresh_mesh(&mut self) {
        let viewer = &self.debug.viewer;
        (self.fill_vertices, self.fill_indices) = upload_fill(&self.memory_allocator, viewer);
        self.line_vertices = upload_lines(&self.memory_allocator, viewer);
        (self.flat_vertices, self.flat_indices) = upload_flat(&self.memory_allocator, viewer);
        self.flat_lines = upload_flat_lines(&self.memory_allocator, viewer);
        (self.chunk_flat_vertices, self.chunk_flat_indices) =
            upload_chunk_flat(&self.memory_allocator, viewer);
        self.chunk_flat_lines = upload_chunk_flat_lines(&self.memory_allocator, viewer);
        self.camera = OrbitCamera::framing_planet(viewer.radius);
        tracing::info!(
            subdivisions = viewer.subdiv,
            radius = viewer.radius,
            cells = viewer.stats.cells,
            hash8 = viewer.stats.hash8.as_str(),
            gen_ms = format!("{:.1}", viewer.stats.gen_ms),
            "sphere viewer mesh regenerated",
        );
    }

    /// Re-upload the flat hemisphere GPU buffers after an arrow-key orbit
    /// step (the lib state already reloaded the new half). Camera framing
    /// is untouched — only the visible chunk set changes.
    fn refresh_chunk_flat(&mut self) {
        let viewer = &self.debug.viewer;
        (self.chunk_flat_vertices, self.chunk_flat_indices) =
            upload_chunk_flat(&self.memory_allocator, viewer);
        self.chunk_flat_lines = upload_chunk_flat_lines(&self.memory_allocator, viewer);
        tracing::info!(
            cells = viewer.chunk_flat_cells.len(),
            viewpoint = ?viewer.chunk_flat_viewpoint,
            "chunk flat hemisphere reloaded",
        );
    }

    /// Recompute the hovered chunk from the current cursor: only when the
    /// Sphere Viewer is active and the cursor sits inside a chunk-rendered
    /// rect (main viewport with sphere/chunk-flat focus, panel thumb with
    /// UV focus). A miss (cursor over empty space, panel, or nav) clears
    /// the hover. Hover feeds the fill highlight + panel CHUNK readout;
    /// it never touches the mesh. Callers refresh after every cursor or
    /// camera move so the highlight tracks within one frame.
    fn update_hover(&mut self) {
        let hovered = self
            .last_cursor
            .zip(self.rcx_window_size())
            .filter(|_| self.debug.screen == Screen::SphereViewer)
            .and_then(|((cx, cy), (w, h))| {
                let layout = ui::layout(w, h);
                match self.debug.viewer.focus {
                    ViewFocus::ChunkFlat => {
                        let rect = layout.viewport;
                        if !rect.contains(cx, cy) || rect.w < 1.0 || rect.h < 1.0 {
                            return None;
                        }
                        let point = flat_point_from_cursor((cx, cy), rect)?;
                        let viewer = &self.debug.viewer;
                        Some(pick_flat_visible(
                            &viewer.mesh,
                            &viewer.chunk_flat_cells,
                            &viewer.chunk_flat_centers,
                            point,
                        ))
                    }
                    _ => {
                        let rect = match self.debug.viewer.focus {
                            ViewFocus::SphereMain => layout.viewport,
                            ViewFocus::UvMain => ui::uv_thumb_rect(layout.panel, 8.0),
                            ViewFocus::ChunkFlat => unreachable!("flat branch above"),
                        };
                        if !rect.contains(cx, cy) || rect.w < 1.0 || rect.h < 1.0 {
                            return None;
                        }
                        let aspect = rect.w / rect.h;
                        let view_proj =
                            self.camera.projection_matrix(aspect) * self.camera.view_matrix();
                        let ray = ray_from_cursor((cx, cy), rect, view_proj);
                        let hit = intersect_sphere(ray, self.debug.viewer.radius)?;
                        let viewer = &self.debug.viewer;
                        Some(pick_cell(&viewer.mesh, hit, viewer.hovered))
                    }
                }
            });
        self.debug.viewer.hovered = hovered;
    }

    /// (Re)build the atlas image + descriptor set when the atlas grew;
    /// upload texels when the version changed.
    fn sync_atlas(&mut self) {
        let extent = self.atlas.extent();
        let grown = self
            .atlas_image
            .as_ref()
            .is_none_or(|image| image.extent()[0] != extent);
        if grown {
            let image = create_atlas_image(&self.memory_allocator, extent);
            let rcx = self.rcx.as_ref().expect("render context must exist");
            let set_layout = rcx.ui_pipeline.layout().set_layouts()[0].clone();
            let view = ImageView::new_default(image.clone()).expect("atlas view must create");
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
            self.atlas_image = Some(image);
            self.atlas_set = Some(set);
            self.uploaded_atlas_version = None;
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
            fill: build_fill_pipeline(&self.device, &render_pass),
            line: build_line_pipeline(&self.device, &render_pass),
            flat: build_flat_pipeline(&self.device, &render_pass),
            flat_line: build_flat_line_pipeline(&self.device, &render_pass),
            chunk_flat: build_chunk_flat_pipeline(&self.device, &render_pass),
            ui: build_ui_pipeline(&self.device, &render_pass),
        };
        (render_pass, pipelines)
    }
}

struct Pipelines {
    fill: Arc<GraphicsPipeline>,
    line: Arc<GraphicsPipeline>,
    flat: Arc<GraphicsPipeline>,
    flat_line: Arc<GraphicsPipeline>,
    chunk_flat: Arc<GraphicsPipeline>,
    ui: Arc<GraphicsPipeline>,
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

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes().with_title("PlanetCrafter — sphere viewer"),
                )
                .expect("viewer window must create"),
        );
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
        self.rcx = Some(RenderContext {
            window,
            swapchain,
            render_pass,
            framebuffers,
            depth_view,
            fill_pipeline: pipelines.fill,
            line_pipeline: pipelines.line,
            flat_pipeline: pipelines.flat,
            flat_line_pipeline: pipelines.flat_line,
            chunk_flat_pipeline: pipelines.chunk_flat,
            ui_pipeline: pipelines.ui,
            recreate_swapchain: false,
            previous_frame_end: Some(sync::now(self.device.clone()).boxed()),
        });
        // Atlas image + descriptor set need the UI pipeline: build lazily
        // on the first frame via `sync_atlas`.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                if let Some(rcx) = self.rcx.as_mut() {
                    rcx.recreate_swapchain = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = (position.x as f32, position.y as f32);
                if self.dragging_slider {
                    let size = self.rcx_window_size();
                    if let Some((w, h)) = size {
                        let viewer = &mut self.debug.viewer;
                        let lh = self.atlas.line_height();
                        let warn =
                            parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                        let track = panel_plan(ui::layout(w, h).panel, lh, warn)
                            .rects
                            .subdiv_track;
                        viewer.subdiv_slider.drag_to(track, cursor.0);
                        viewer.sync_field_from_slider();
                    }
                } else if self.dragging_density {
                    let size = self.rcx_window_size();
                    if let Some((w, h)) = size {
                        let viewer = &mut self.debug.viewer;
                        let lh = self.atlas.line_height();
                        let warn =
                            parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                        let track = panel_plan(ui::layout(w, h).panel, lh, warn)
                            .rects
                            .density_track;
                        viewer.density_slider.drag_to(track, cursor.0);
                        viewer.sync_density_from_slider();
                    }
                } else if self.dragging_orbit
                    && let Some(last) = self.last_cursor
                {
                    self.camera.rotate(cursor.0 - last.0, cursor.1 - last.1);
                }
                self.last_cursor = Some(cursor);
                // Hover tracks cursor AND camera moves (orbit drags change
                // the cells under a static cursor).
                self.update_hover();
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                let pressed = state == ElementState::Pressed;
                if !pressed {
                    // Release: a press that barely traveled counts as a
                    // click — pin the hovered chunk. The release must still
                    // land in the main sphere viewport, and pinning only
                    // exists there (thumb clicks keep swap-view).
                    let click = self.press_cursor.zip(self.last_cursor).is_some_and(
                        |((px, py), (cx, cy))| (cx - px).hypot(cy - py) <= CLICK_MAX_DRAG_PX,
                    );
                    self.dragging_orbit = false;
                    self.dragging_slider = false;
                    self.dragging_density = false;
                    self.press_cursor = None;
                    if click
                        && self.debug.screen == Screen::SphereViewer
                        && matches!(
                            self.debug.viewer.focus,
                            ViewFocus::SphereMain | ViewFocus::ChunkFlat
                        )
                        && let Some(chunk) = self.debug.viewer.hovered
                        && self.last_cursor.zip(self.rcx_window_size()).is_some_and(
                            |((cx, cy), (w, h))| ui::layout(w, h).viewport.contains(cx, cy),
                        )
                    {
                        self.debug.viewer.toggle_pin(chunk);
                    }
                    return;
                }
                let Some((cx, cy)) = self.last_cursor else {
                    return;
                };
                let Some((w, h)) = self.rcx_window_size() else {
                    return;
                };
                let layout = ui::layout(w, h);
                // Nav bar first.
                for (i, screen) in Screen::ALL.iter().enumerate() {
                    if ui::nav_button(layout.nav, i).contains(cx, cy) {
                        self.debug.select(*screen);
                        return;
                    }
                }
                // Viewport drag starts an orbit.
                if layout.viewport.contains(cx, cy) {
                    self.dragging_orbit = true;
                    // A sphere-main or chunk-flat press may end as a
                    // chunk-pin click (decided on release by travel
                    // distance).
                    if self.debug.screen == Screen::SphereViewer
                        && matches!(
                            self.debug.viewer.focus,
                            ViewFocus::SphereMain | ViewFocus::ChunkFlat
                        )
                    {
                        self.press_cursor = Some((cx, cy));
                    }
                    return;
                }
                // Panel widgets (viewer screen only).
                if self.debug.screen != Screen::SphereViewer {
                    return;
                }
                let regenerated = {
                    let viewer = &mut self.debug.viewer;
                    let lh = self.atlas.line_height();
                    let warn =
                        parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                    let rects = panel_plan(layout.panel, lh, warn).rects;
                    // UV thumb + caption + swap button all flip main ↔ thumb.
                    if rects.thumb.contains(cx, cy)
                        || rects.thumb_caption.contains(cx, cy)
                        || rects.swap_button.contains(cx, cy)
                    {
                        viewer.toggle_focus();
                    }
                    if rects.shader_button.contains(cx, cy) {
                        viewer.cycle_debug_mode();
                    }
                    viewer.subdiv_field.click(rects.subdiv_field, cx, cy);
                    viewer.radius_field.click(rects.radius_field, cx, cy);
                    if rects.subdiv_track.contains(cx, cy) {
                        self.dragging_slider = true;
                        viewer.subdiv_slider.drag_to(rects.subdiv_track, cx);
                        viewer.sync_field_from_slider();
                    }
                    if rects.density_track.contains(cx, cy) {
                        self.dragging_density = true;
                        viewer.density_slider.drag_to(rects.density_track, cx);
                        viewer.sync_density_from_slider();
                    }
                    let regenerated = viewer.can_regenerate()
                        && rects.regen_button.contains(cx, cy)
                        && viewer.regenerate().is_ok();
                    viewer.wire_cb.click(rects.wire_box, cx, cy);
                    viewer.pent_cb.click(rects.pent_box, cx, cy);
                    viewer.seam_cb.click(rects.seam_box, cx, cy);
                    viewer.uvwire_cb.click(rects.uvwire_box, cx, cy);
                    viewer.sync_toggles();
                    regenerated
                };
                if regenerated {
                    self.refresh_mesh();
                }
                // Panel clicks, focus swaps and rebuilds all change what
                // sits under the cursor — refresh the hover.
                self.update_hover();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let in_viewport = self
                    .last_cursor
                    .zip(self.rcx_window_size())
                    .is_some_and(|((cx, cy), (w, h))| ui::layout(w, h).viewport.contains(cx, cy));
                if in_viewport {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                    };
                    self.camera.zoom(scroll);
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
                if state != ElementState::Pressed {
                    return;
                }
                match physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::Enter) => {
                        let viewer = &mut self.debug.viewer;
                        viewer.subdiv_field.focused = false;
                        viewer.radius_field.focused = false;
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        let viewer = &mut self.debug.viewer;
                        viewer.subdiv_field.backspace();
                        viewer.radius_field.backspace();
                        viewer.sync_slider_from_field();
                    }
                    PhysicalKey::Code(KeyCode::F1) => {
                        self.debug.select_by_fkey(1);
                    }
                    PhysicalKey::Code(KeyCode::F2) => {
                        self.debug.select_by_fkey(2);
                    }
                    PhysicalKey::Code(KeyCode::F3) => {
                        self.debug.select_by_fkey(3);
                    }
                    PhysicalKey::Code(KeyCode::F4) => {
                        self.debug.select_by_fkey(4);
                    }
                    PhysicalKey::Code(
                        KeyCode::Digit1
                        | KeyCode::Digit2
                        | KeyCode::Digit3
                        | KeyCode::Digit4
                        | KeyCode::Digit5
                        | KeyCode::Digit6,
                    ) => {
                        // Direct debug-mode select (fields unfocused only).
                        if self.debug.screen == Screen::SphereViewer {
                            let viewer = &self.debug.viewer;
                            if !viewer.subdiv_field.focused && !viewer.radius_field.focused {
                                let i = match physical_key {
                                    PhysicalKey::Code(KeyCode::Digit1) => 0,
                                    PhysicalKey::Code(KeyCode::Digit2) => 1,
                                    PhysicalKey::Code(KeyCode::Digit3) => 2,
                                    PhysicalKey::Code(KeyCode::Digit4) => 3,
                                    PhysicalKey::Code(KeyCode::Digit5) => 4,
                                    _ => 5,
                                };
                                if let Some(&mode) = DebugMode::ALL.get(i) {
                                    self.debug.viewer.debug_mode = mode;
                                }
                            } else if let Some(text) = text {
                                let viewer = &mut self.debug.viewer;
                                for ch in text.chars() {
                                    viewer.subdiv_field.insert_char(ch);
                                    viewer.radius_field.insert_char(ch);
                                }
                                viewer.sync_slider_from_field();
                            }
                        }
                    }
                    PhysicalKey::Code(
                        KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::ArrowUp
                        | KeyCode::ArrowDown,
                    ) => {
                        // Flat-map orbit: only when the chunk-flat view is
                        // focused and no text field owns the keystrokes.
                        // Each step yaws/pitches the viewpoint 5° and
                        // reloads the hemisphere GPU buffers (unload +
                        // load in one rebuild).
                        if self.debug.screen == Screen::SphereViewer
                            && self.debug.viewer.focus == ViewFocus::ChunkFlat
                        {
                            let viewer = &self.debug.viewer;
                            if !viewer.subdiv_field.focused && !viewer.radius_field.focused {
                                const STEP: f32 = std::f32::consts::PI / 36.0;
                                let (yaw, pitch) = match physical_key {
                                    PhysicalKey::Code(KeyCode::ArrowLeft) => (STEP, 0.0),
                                    PhysicalKey::Code(KeyCode::ArrowRight) => (-STEP, 0.0),
                                    PhysicalKey::Code(KeyCode::ArrowUp) => (0.0, STEP),
                                    _ => (0.0, -STEP),
                                };
                                self.debug.viewer.orbit_chunk_flat(yaw, pitch);
                                self.refresh_chunk_flat();
                                self.update_hover();
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyU) => {
                        // Swap toggle — but never steal keystrokes from
                        // focused fields (`u` is printable input there).
                        if self.debug.screen == Screen::SphereViewer {
                            let viewer = &self.debug.viewer;
                            if !viewer.subdiv_field.focused && !viewer.radius_field.focused {
                                self.debug.viewer.toggle_focus();
                                self.update_hover();
                            } else if let Some(text) = text {
                                let viewer = &mut self.debug.viewer;
                                for ch in text.chars() {
                                    viewer.subdiv_field.insert_char(ch);
                                    viewer.radius_field.insert_char(ch);
                                }
                                viewer.sync_slider_from_field();
                            }
                        }
                    }
                    _ => {
                        if let Some(text) = text {
                            let viewer = &mut self.debug.viewer;
                            for ch in text.chars() {
                                viewer.subdiv_field.insert_char(ch);
                                viewer.radius_field.insert_char(ch);
                            }
                            viewer.sync_slider_from_field();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(rcx) = self.rcx.as_ref() {
            rcx.window.request_redraw();
        }
    }
}

impl ViewerApp {
    fn rcx_window_size(&self) -> Option<(f32, f32)> {
        self.rcx.as_ref().map(|rcx| {
            let size = rcx.window.inner_size();
            (size.width as f32, size.height as f32)
        })
    }

    fn draw(&mut self) {
        let (win_w, win_h) = self.rcx_window_size().expect("render context must exist");
        if win_w < 1.0 || win_h < 1.0 {
            return;
        }
        {
            let rcx = self.rcx.as_mut().expect("render context must exist");
            rcx.previous_frame_end
                .as_mut()
                .expect("frame future")
                .cleanup_finished();
            if rcx.recreate_swapchain {
                let window_size = rcx.window.inner_size();
                let (new_swapchain, new_images) = rcx
                    .swapchain
                    .recreate(SwapchainCreateInfo {
                        image_extent: window_size.into(),
                        ..rcx.swapchain.create_info()
                    })
                    .expect("swapchain recreation must succeed");
                rcx.swapchain = new_swapchain;
                rcx.depth_view =
                    create_depth_view(&self.memory_allocator, rcx.swapchain.image_extent());
                rcx.framebuffers =
                    window_size_dependent_setup(&new_images, &rcx.render_pass, &rcx.depth_view);
                rcx.recreate_swapchain = false;
            }
        }

        let layout = ui::layout(win_w, win_h);
        let viewer_screen = self.debug.screen == Screen::SphereViewer;

        // Build frame UI (atlas insertions happen here) and sync the GPU
        // atlas before recording.
        let items = if viewer_screen {
            build_viewer_ui(&mut self.atlas, &self.debug.viewer, layout)
        } else {
            build_placeholder_ui(&mut self.atlas, self.debug.screen, layout)
        };
        self.sync_atlas();
        let ui_verts = ui_items_to_vertices(&items, &mut self.atlas);
        assert!(
            ui_verts.len() as u64 <= MAX_UI_VERTS,
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

        let rcx = self.rcx.as_mut().expect("render context must exist");
        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    return;
                }
                Err(error) => panic!("swapchain acquire failed: {error}"),
            };
        if suboptimal {
            rcx.recreate_swapchain = true;
        }

        let mut builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .expect("command buffer builder must create");
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![
                        Some([0.02, 0.03, 0.08, 1.0].into()),
                        Some(ClearValue::Depth(1.0)),
                    ],
                    ..RenderPassBeginInfo::framebuffer(
                        rcx.framebuffers[image_index as usize].clone(),
                    )
                },
                SubpassBeginInfo {
                    contents: SubpassContents::Inline,
                    ..Default::default()
                },
            )
            .expect("render pass must begin");

        if viewer_screen {
            // Triple views: the focused view fills the main viewport, a
            // secondary view renders into the panel-top thumb (same rect
            // the UI frames and hit-tests). SphereMain pairs with the UV
            // net thumb; both flat focuses pair with the 3D sphere thumb.
            // The viewport transform clips output to each rect, so no
            // scissor state is needed.
            let focus = self.debug.viewer.focus;
            let thumb_focus = match focus {
                ViewFocus::SphereMain => ViewFocus::UvMain,
                ViewFocus::UvMain | ViewFocus::ChunkFlat => ViewFocus::SphereMain,
            };
            let views = [
                (layout.viewport, focus),
                (ui::uv_thumb_rect(layout.panel, 8.0), thumb_focus),
            ];
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
                let viewport = Viewport {
                    offset: [vp.x, vp.y],
                    extent: [vp.w, vp.h],
                    depth_range: 0.0..=1.0,
                };
                if view == ViewFocus::SphereMain {
                    let aspect = vp.w / vp.h;
                    let mvp = (self.camera.projection_matrix(aspect) * self.camera.view_matrix())
                        .to_cols_array_2d();
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(rcx.fill_pipeline.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.fill_vertices.clone())
                        .expect("vertex buffer must bind")
                        .bind_index_buffer(self.fill_indices.clone())
                        .expect("index buffer must bind")
                        .push_constants(
                            rcx.fill_pipeline.layout().clone(),
                            0,
                            FillPush {
                                mvp,
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
                            .bind_pipeline_graphics(rcx.line_pipeline.clone())
                            .expect("pipeline must bind")
                            .bind_vertex_buffers(0, self.line_vertices.clone())
                            .expect("vertex buffer must bind")
                            .push_constants(
                                rcx.line_pipeline.layout().clone(),
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
                } else if view == ViewFocus::ChunkFlat {
                    let mvp = flat_mvp(vp);
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(rcx.chunk_flat_pipeline.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.chunk_flat_vertices.clone())
                        .expect("vertex buffer must bind")
                        .bind_index_buffer(self.chunk_flat_indices.clone())
                        .expect("index buffer must bind")
                        .push_constants(
                            rcx.chunk_flat_pipeline.layout().clone(),
                            0,
                            FillPush {
                                mvp,
                                highlight,
                                mode,
                                density,
                                seams_on,
                                hover_cell,
                                pin_cell,
                            },
                        )
                        .expect("chunk flat push constants must upload");
                    unsafe {
                        builder.draw_indexed(self.chunk_flat_indices.len() as u32, 1, 0, 0, 0)
                    }
                    .expect("chunk flat draw must record");
                    if self.debug.viewer.wire_on_uv && !self.debug.viewer.chunk_flat_wire.is_empty()
                    {
                        builder
                            .bind_pipeline_graphics(rcx.flat_line_pipeline.clone())
                            .expect("pipeline must bind")
                            .bind_vertex_buffers(0, self.chunk_flat_lines.clone())
                            .expect("vertex buffer must bind")
                            .push_constants(
                                rcx.flat_line_pipeline.layout().clone(),
                                0,
                                FillPush {
                                    mvp,
                                    highlight,
                                    mode,
                                    density,
                                    seams_on,
                                    hover_cell,
                                    pin_cell,
                                },
                            )
                            .expect("chunk flat line push constants must upload");
                        // SAFETY: same contract as the 3D wireframe draw.
                        unsafe {
                            builder.draw(self.debug.viewer.chunk_flat_wire.len() as u32, 1, 0, 0)
                        }
                        .expect("chunk flat wireframe draw must record");
                    }
                } else {
                    let mvp = flat_mvp(vp);
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(rcx.flat_pipeline.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.flat_vertices.clone())
                        .expect("vertex buffer must bind")
                        .bind_index_buffer(self.flat_indices.clone())
                        .expect("index buffer must bind")
                        .push_constants(
                            rcx.flat_pipeline.layout().clone(),
                            0,
                            FillPush {
                                mvp,
                                highlight,
                                mode,
                                density,
                                seams_on,
                                hover_cell,
                                pin_cell,
                            },
                        )
                        .expect("flat push constants must upload");
                    unsafe { builder.draw_indexed(self.flat_indices.len() as u32, 1, 0, 0, 0) }
                        .expect("flat draw must record");
                    if self.debug.viewer.wire_on_uv && !self.debug.viewer.uv_lines.is_empty() {
                        builder
                            .bind_pipeline_graphics(rcx.flat_line_pipeline.clone())
                            .expect("pipeline must bind")
                            .bind_vertex_buffers(0, self.flat_lines.clone())
                            .expect("vertex buffer must bind")
                            .push_constants(
                                rcx.flat_line_pipeline.layout().clone(),
                                0,
                                FillPush {
                                    mvp,
                                    highlight,
                                    mode,
                                    density,
                                    seams_on,
                                    hover_cell,
                                    pin_cell,
                                },
                            )
                            .expect("flat line push constants must upload");
                        // SAFETY: same contract as the 3D wireframe draw.
                        unsafe { builder.draw(self.debug.viewer.uv_lines.len() as u32, 1, 0, 0) }
                            .expect("flat wireframe draw must record");
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
            .bind_pipeline_graphics(rcx.ui_pipeline.clone())
            .expect("pipeline must bind")
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                rcx.ui_pipeline.layout().clone(),
                0,
                self.atlas_set.clone().expect("atlas set must exist"),
            )
            .expect("descriptor set must bind")
            .bind_vertex_buffers(0, ui_buffer.clone())
            .expect("vertex buffer must bind")
            .push_constants(
                rcx.ui_pipeline.layout().clone(),
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
                rcx.ui_pipeline.layout().clone(),
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
        builder
            .end_render_pass(Default::default())
            .expect("render pass must end");
        let command_buffer = builder.build().expect("command buffer must build");

        let rcx = self.rcx.as_mut().expect("render context must exist");
        let future = rcx
            .previous_frame_end
            .take()
            .expect("frame future")
            .join(acquire_future)
            .then_execute(self.queue.clone(), command_buffer)
            .expect("command buffer must submit")
            .then_swapchain_present(
                self.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush();
        match future.map_err(Validated::unwrap) {
            Ok(future) => rcx.previous_frame_end = Some(future.boxed()),
            Err(VulkanError::OutOfDate) => {
                rcx.recreate_swapchain = true;
                rcx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
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
    let headless = match parse_args(&argv) {
        Ok(headless) => headless,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if headless {
        return run_headless();
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("event loop failed: {error}");
            return 1;
        }
    };
    let mut app = ViewerApp::new(&event_loop);
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
            (ShaderKind::Vertex, FLAT_VERT, "flat vert"),
            (ShaderKind::Vertex, CHUNK_FLAT_VERT, "chunk flat vert"),
            (ShaderKind::Vertex, FLAT_LINE_VERT, "flat line vert"),
            (ShaderKind::Vertex, LINE_VERT, "line vert"),
            (ShaderKind::Fragment, LINE_FRAG, "line frag"),
            (ShaderKind::Vertex, UI_VERT, "ui vert"),
            (ShaderKind::Fragment, UI_FRAG, "ui frag"),
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
    fn flat_mvp_centers_unit_square() {
        // Wide viewport: square fit by height, u centered.
        let m = flat_mvp(Rect {
            x: 0.0,
            y: 0.0,
            w: 1020.0,
            h: 692.0,
        });
        let project = |u: f32, v: f32| (m[0][0] * u + m[3][0], m[1][1] * v + m[3][1]);
        let (cx0, cy0) = project(0.0, 0.0);
        let (cx1, cy1) = project(1.0, 1.0);
        // Centered: x symmetric, y spans full NDC (v=0 top → +1).
        assert!((cx0 + cx1).abs() < 1e-5, "{cx0} {cx1}");
        assert!((cy0 - 1.0).abs() < 1e-5, "{cy0}");
        assert!((cy1 + 1.0).abs() < 1e-5, "{cy1}");
        let (mx, my) = project(0.5, 0.5);
        assert!(mx.abs() < 1e-5 && my.abs() < 1e-5, "{mx} {my}");
        // Tall viewport: fit by width instead.
        let t = flat_mvp(Rect {
            x: 0.0,
            y: 0.0,
            w: 244.0,
            h: 500.0,
        });
        let tx = |u: f32| t[0][0] * u + t[3][0];
        let ty = |v: f32| t[1][1] * v + t[3][1];
        assert!((tx(0.0) + 1.0).abs() < 1e-5);
        assert!((tx(1.0) - 1.0).abs() < 1e-5);
        assert!((ty(0.0) + ty(1.0)).abs() < 1e-5, "v centered");
    }

    #[test]
    fn panel_plan_stays_inside_and_ordered() {
        for warn in [false, true] {
            let layout = ui::layout(1280.0, 720.0);
            let plan = panel_plan(layout.panel, 19.0, warn);
            assert_eq!(plan.warn_line.is_some(), warn);
            let rects = &plan.rects;
            // Thumb is the shared hit-test rect by construction.
            assert_eq!(rects.thumb, ui::uv_thumb_rect(layout.panel, 8.0));
            for rect in [
                rects.thumb,
                rects.thumb_caption,
                rects.swap_button,
                rects.shader_button,
                rects.density_track,
                rects.subdiv_field,
                rects.subdiv_track,
                rects.radius_field,
                rects.regen_button,
                rects.wire_box,
                rects.pent_box,
                rects.seam_box,
                rects.uvwire_box,
            ] {
                assert!(rect.x >= layout.panel.x, "{rect:?}");
                assert!(
                    rect.x + rect.w <= layout.panel.x + layout.panel.w + 1e-3,
                    "{rect:?}"
                );
            }
            assert!(rects.thumb.y < rects.thumb_caption.y);
            assert!(rects.thumb_caption.y < rects.swap_button.y);
            assert!(rects.swap_button.y < rects.shader_button.y);
            assert!(rects.shader_button.y < rects.density_track.y);
            assert!(rects.density_track.y < rects.subdiv_field.y);
            assert!(rects.subdiv_field.y < rects.subdiv_track.y);
            assert!(rects.subdiv_track.y < rects.radius_field.y);
            assert!(rects.radius_field.y < rects.regen_button.y);
            assert!(rects.regen_button.y < rects.wire_box.y);
            assert!(rects.wire_box.y < rects.pent_box.y);
            assert!(rects.pent_box.y < rects.seam_box.y);
            assert!(rects.seam_box.y < rects.uvwire_box.y);
            assert!(rects.uvwire_box.y < plan.chunk_header.y);
            assert!(plan.chunk_header.y < plan.chunk_lines[0].y);
            assert!(plan.chunk_lines.windows(2).all(|w| w[0].y < w[1].y));
            assert!(plan.chunk_lines[3].y < plan.stats_header.y);
            assert!(plan.uv_header.y < plan.density_label.y);
            assert!(plan.inputs_header.y < plan.subdiv_label.y);
            assert!(plan.subdiv_hint.y < plan.radius_label.y);
            assert!(plan.radius_hint.y < plan.stats_header.y);
            assert!(plan.stat_lines.windows(2).all(|w| w[0].y < w[1].y));
        }
    }

    #[test]
    fn viewer_ui_contains_panel_content() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let viewer = SphereViewerState::new();
        let layout = ui::layout(1280.0, 720.0);
        let items = build_viewer_ui(&mut atlas, &viewer, layout);
        assert!(!items.solids.is_empty() && !items.texts.is_empty());
        let joined = items
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [
            "Sphere Viewer",
            "INPUTS",
            "Subdivisions",
            "Radius",
            "Regenerate",
            "Wireframe",
            "Pentagons",
            "Seams",
            "UV wire",
            "UV DEBUG",
            "CHUNK",
            "chunk:",
            "state:",
            "Swap view (U)",
            "Shader: Lit",
            "Checker density:",
            "click/U",
            "STATS",
            "cells:",
            "hash:",
            "view:",
        ] {
            assert!(joined.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn placeholder_ui_centers_title_and_body() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let layout = ui::layout(1280.0, 720.0);
        let items = build_placeholder_ui(&mut atlas, Screen::Console, layout);
        let joined = items
            .texts
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("Console"));
        assert!(joined.contains("not implemented yet"));
    }

    #[test]
    fn ui_vertices_cover_all_items() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let viewer = SphereViewerState::new();
        let layout = ui::layout(1280.0, 720.0);
        let items = build_viewer_ui(&mut atlas, &viewer, layout);
        let verts = ui_items_to_vertices(&items, &mut atlas);
        let text_quads: usize = items.texts.iter().map(|t| t.text.chars().count()).sum();
        assert_eq!(verts.len(), items.solids.len() * 6 + text_quads * 6);
        assert!((verts.len() as u64) < MAX_UI_VERTS);
    }
}
