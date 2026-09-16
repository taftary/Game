//! `game_debug` sphere viewer binary (`plans/debug-ui-reorganize`).
//!
//! - `--headless`: GPU-free path (CI-safe — never loads the Vulkan
//!   loader): builds the default N=6/R=1.0 viewer mesh through the
//!   `game_debug` lib and prints stats.
//! - Windowed (default): two `winit` windows sharing one `vulkano`
//!   device. The viewer window hosts the Sphere Viewer screen (F1:
//!   orbit camera, filled dual-cell mesh, wireframe overlay, pentagon
//!   highlight, cell-chunk hover highlight + click-to-pin with panel
//!   readout, inputs panel, read-only stats) and the UV Net screen (F2:
//!   full-viewport icosa-net unwrap with its own dock). The tools window
//!   hosts the FPS / Console / Inspector tabs (window-local `1/2/3`;
//!   Console/Inspector are placeholders). Closing the tools window hides
//!   it (`F3` on the viewer window reopens); closing the viewer window
//!   (or `Esc`) exits.
//!
//! All screen logic lives in the `game_debug` lib (window- and GPU-free);
//! this binary owns the winit event loop, the graphics pipelines (fill,
//! wireframe lines, flat UV, UI quads), the depth buffers and the
//! font-atlas texture. `game_engine` and `game` are untouched.
//!
//! Usage: `game_debug [--headless]`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use game::camera::CameraMode;
use game_debug::app::{App as DebugApp, MainScreen, ToolsScreen};
use game_debug::fps::{FPS_SPARKLINE, FpsOverlay};
use game_debug::params::{cell_count_hint, parse_radius, parse_subdivisions, subdiv_warning};
use game_debug::picking::{Ray, intersect_sphere, pick_cell, ray_from_cursor};
use game_debug::player_view::{MoveKeys, PlayerViewState};
use game_debug::sphere_viewer::{DebugMode, SphereViewerState};
use game_debug::text::GlyphAtlas;
use game_debug::ui::{self, Layout, Rect};
use game_engine::render::{
    MAX_PITCH, OrbitCamera, ShaderKind, compile_glsl_to_spirv, create_instance, device_score,
    log_physical_device, required_device_extensions, visible_hemisphere,
};
use glam::{Mat4, Vec3};
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
/// Section header bar behind VIEW / INPUTS / SELECTION / STATS titles.
const C_SECTION_BG: Color = [0.11, 0.13, 0.19, 1.0];
/// FPS sparkline bars: <20 ms ok, <34 ms warm, above = hitch.
const C_FPS_OK: Color = [0.30, 0.85, 0.45, 1.0];
const C_FPS_WARN: Color = [1.00, 0.75, 0.25, 1.0];
const C_FPS_HITCH: Color = [1.00, 0.35, 0.30, 1.0];
/// Screen-aware layout: both viewer-window screens (sphere + UV net)
/// get the left dock + center viewport + right data dock; the tools
/// window reclaims the full width so viewer inputs stay attached to
/// the viewer window.
fn app_layout(screen: MainScreen, win_w: f32, win_h: f32) -> Layout {
    match screen {
        MainScreen::SphereViewer | MainScreen::UvNet => ui::layout_viewer(win_w, win_h),
    }
}
/// Player marker dot (sphere view).
const C_PLAYER: Color = [0.30, 1.00, 0.45, 1.0];
/// Player marker size, pixels.
const PLAYER_DOT: f32 = 6.0;
/// Streaming desired-set refresh throttle while player mode is active.
const STREAM_SYNC_MS: u64 = 100;

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
        "uv_islands={} uv_seam_verts={} uv_flat_tris={} uv_flat_verts={} mode={:?}",
        islands.len(),
        seams,
        viewer.flat.indices.len() / 3,
        viewer.flat.uv.len(),
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

/// World-space arrow tip for the sphere marker: along the facing at
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
/// Same shape in the sphere and flat views.
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

/// Widget rects inside the sphere screen's left dock (presets + shader
/// + sphere overlays). Only built for the Sphere Viewer screen.
struct SphereLeftRects {
    shader_button: Rect,
    density_track: Option<Rect>,
    wire_box: Rect,
    pent_box: Rect,
    seam_box: Rect,
    /// Global camera presets as a 2×2 grid: [Top, Bottom] / [Right, Persp].
    preset_grid: [[Rect; 2]; 2],
}

/// Sphere left-dock row plan: widget rects + label rows in draw order.
/// Built with a single cursor so hit-testing and drawing always agree.
/// The density rows only exist in Checker mode (inputs appear only when
/// needed — the cursor flows up when they don't).
struct SphereLeftPlan {
    rects: SphereLeftRects,
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

fn sphere_left_plan(left: Rect, lh: f32, checker: bool) -> SphereLeftPlan {
    let mut rows = ui::PanelRows::new(left, 8.0);
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
    SphereLeftPlan {
        rects: SphereLeftRects {
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

/// Widget rects inside the UV screen's left dock (shader + UV
/// overlays). Only built for the UV Net screen.
struct UvLeftRects {
    shader_button: Rect,
    density_track: Option<Rect>,
    uvwire_box: Rect,
    seam_box: Rect,
}

/// UV left-dock row plan: same cursor contract as the sphere plan; the
/// density rows only exist in Checker mode.
struct UvLeftPlan {
    rects: UvLeftRects,
    uv_header: Rect,
    uv_info: Rect,
    shader_header: Rect,
    shader_hint: Rect,
    density_label: Option<Rect>,
    overlay_header: Rect,
    uvwire_label: Rect,
    seam_label: Rect,
}

fn uv_left_plan(left: Rect, lh: f32, checker: bool) -> UvLeftPlan {
    let mut rows = ui::PanelRows::new(left, 8.0);
    let uv_header = rows.next(lh + 6.0, 4.0);
    let uv_info = rows.next(lh, 4.0);
    let shader_header = rows.next(lh + 6.0, 4.0);
    let shader_button = rows.next(28.0, 4.0);
    let shader_hint = rows.next(lh, 4.0);
    let (density_label, density_track) = if checker {
        (Some(rows.next(lh, 4.0)), Some(rows.next(20.0, 6.0)))
    } else {
        (None, None)
    };
    let overlay_header = rows.next(lh + 6.0, 4.0);
    let uvwire_label = rows.next(lh, 4.0);
    let seam_label = rows.next(lh, 4.0);
    UvLeftPlan {
        rects: UvLeftRects {
            shader_button,
            density_track,
            uvwire_box: check_box(uvwire_label, lh),
            seam_box: check_box(seam_label, lh),
        },
        uv_header,
        uv_info,
        shader_header,
        shader_hint,
        density_label,
        overlay_header,
        uvwire_label,
        seam_label,
    }
}

/// Widget rects inside the right data dock (params + selection + stats).
/// Only built for the Sphere Viewer screen.
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
    stat_lines: [Rect; 6],
}

fn right_panel_plan(panel: Rect, lh: f32, warn: bool, player_active: bool) -> RightPlan {
    let mut rows = ui::PanelRows::new(panel, 8.0);
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

/// Nav bar shared by every screen of a window: `titles` in order,
/// `active` highlighted.
fn build_nav(items: &mut UiItems, layout: Layout, titles: &[&str], active: usize, lh: f32) {
    items.solid(layout.nav, C_NAV_BG);
    for (i, title) in titles.iter().enumerate() {
        let rect = ui::nav_button(layout.nav, i);
        if i == active {
            items.solid(rect, C_TAB_ACTIVE);
        }
        items.text(
            (*title).to_owned(),
            rect.x + 12.0,
            rect.y + (rect.h + lh) / 2.0 - 3.0,
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

/// Shader selector rows shared by both viewer screens: cycle button +
/// usage hint + checker-density slider (only in Checker mode — inputs
/// appear only when needed).
fn shader_rows(items: &mut UiItems, lh: f32, viewer: &SphereViewerState, rects: &ShaderRowRects) {
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

/// Sphere Viewer UI: left dock (camera presets + shader + sphere
/// overlays) + shared right data dock (the 3D draws separately). Each
/// dock groups its widgets under section bars so controls, params,
/// selection and stats stay visually separate instead of one long
/// undifferentiated column.
fn build_sphere_ui(atlas: &mut GlyphAtlas, viewer: &SphereViewerState, layout: Layout) -> UiItems {
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_nav(
        &mut items,
        layout,
        &MainScreen::ALL.map(|s| s.title()),
        MainScreen::SphereViewer.index(),
        lh,
    );

    // ---- Left dock: VIEW (camera presets — sphere screen only) ----
    let checker = viewer.debug_mode == DebugMode::Checker;
    let left = sphere_left_plan(layout.left, lh, checker);
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

    // ---- Left dock: SHADER + OVERLAYS (sphere-relevant only) ----
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

    build_right_dock(&mut items, lh, viewer, layout.panel);
    items
}

/// UV Net UI: left dock (net info + shader + UV overlays) + shared
/// right data dock (the icosa-net unwrap draws separately).
fn build_uv_ui(atlas: &mut GlyphAtlas, viewer: &SphereViewerState, layout: Layout) -> UiItems {
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_nav(
        &mut items,
        layout,
        &MainScreen::ALL.map(|s| s.title()),
        MainScreen::UvNet.index(),
        lh,
    );

    let checker = viewer.debug_mode == DebugMode::Checker;
    let left = uv_left_plan(layout.left, lh, checker);
    items.solid(layout.left, C_PANEL_BG);
    section_bar(&mut items, left.uv_header, "UV NET");
    text_row(
        &mut items,
        lh,
        left.uv_info,
        "icosa net · 20 islands".to_owned(),
        C_DIM,
    );
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
            left.rects.uvwire_box,
            left.uvwire_label,
            "UV wire",
            viewer.wire_on_uv,
        ),
        (left.rects.seam_box, left.seam_label, "Seams", viewer.seams),
    ] {
        checkbox_row(&mut items, lh, box_rect, label_row, label, checked);
    }

    build_right_dock(&mut items, lh, viewer, layout.panel);
    items
}

/// Right data dock shared by both viewer screens: INPUTS holds mesh
/// params, SELECTION holds chunk + player, STATS is read-only. The
/// player block shrinks to a single "off" line when the player is off,
/// so walk/cam key hints only exist when the player is on.
fn build_right_dock(items: &mut UiItems, lh: f32, viewer: &SphereViewerState, panel: Rect) {
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
    for (row, line) in plan.stat_lines.iter().zip([
        format!("cells:     {}", fmt_int(stats.cells)),
        format!("corners:    {}", fmt_int(stats.corners)),
        format!("pentagons:  {}", stats.pentagons),
        format!("hash:       {}", stats.hash8),
        format!("gen:        {:.1} ms", stats.gen_ms),
        format!("view:       {}", viewer.debug_mode.title()),
    ]) {
        text_row(items, lh, *row, line, C_TEXT);
    }
}

/// Tools window UI: tab nav + per-tab content. FPS shows the live
/// recorder; Console/Inspector are placeholders.
fn build_tools_ui(atlas: &mut GlyphAtlas, app: &DebugApp, layout: Layout) -> UiItems {
    let lh = atlas.line_height();
    let mut items = UiItems::default();
    build_nav(
        &mut items,
        layout,
        &ToolsScreen::ALL.map(|s| s.title()),
        app.tools_screen.index(),
        lh,
    );
    let area = layout.viewport;
    match app.tools_screen {
        ToolsScreen::Fps => build_fps_tab(&mut items, lh, &app.fps, area),
        ToolsScreen::Console | ToolsScreen::Inspector => {
            let title = app.tools_screen.title();
            for (text, color, dy) in [
                (title.to_owned(), C_TEXT, -lh),
                ("not implemented yet".to_owned(), C_DIM, lh),
            ] {
                let (w, _) = atlas.measure(&text);
                items.text(
                    text,
                    area.x + (area.w - w) / 2.0,
                    area.y + area.h / 2.0 + dy,
                    color,
                );
            }
        }
    }
    items
}

/// FPS tab: live numbers + a sparkline of the newest
/// [`FPS_SPARKLINE`] samples (right = newest, 0–50 ms full height).
fn build_fps_tab(items: &mut UiItems, lh: f32, fps: &FpsOverlay, area: Rect) {
    let mut rows = ui::PanelRows::new(area, 8.0);
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
    flat_vert: Arc<ShaderModule>,
    flat_line_vert: Arc<ShaderModule>,
    ui_vert: Arc<ShaderModule>,
    ui_frag: Arc<ShaderModule>,
}

impl ShaderSet {
    fn compile(device: &Arc<Device>) -> Self {
        ShaderSet {
            fill_vert: compile_shader(device, ShaderKind::Vertex, FILL_VERT, "fill vertex"),
            fill_frag: compile_shader(device, ShaderKind::Fragment, FILL_FRAG, "fill fragment"),
            line_vert: compile_shader(device, ShaderKind::Vertex, LINE_VERT, "line vertex"),
            line_frag: compile_shader(device, ShaderKind::Fragment, LINE_FRAG, "line fragment"),
            flat_vert: compile_shader(device, ShaderKind::Vertex, FLAT_VERT, "flat vertex"),
            flat_line_vert: compile_shader(
                device,
                ShaderKind::Vertex,
                FLAT_LINE_VERT,
                "flat line vertex",
            ),
            ui_vert: compile_shader(device, ShaderKind::Vertex, UI_VERT, "ui vertex"),
            ui_frag: compile_shader(device, ShaderKind::Fragment, UI_FRAG, "ui fragment"),
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

fn build_flat_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .flat_vert
        .entry_point("main")
        .expect("vertex entry point");
    // The UV net shares the fill fragment shader.
    let fs = shaders
        .fill_frag
        .entry_point("main")
        .expect("fragment entry point");
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

fn build_flat_line_pipeline(
    device: &Arc<Device>,
    shaders: &ShaderSet,
    render_pass: &Arc<RenderPass>,
) -> Arc<GraphicsPipeline> {
    let vs = shaders
        .flat_line_vert
        .entry_point("main")
        .expect("vertex entry point");
    let fs = shaders
        .line_frag
        .entry_point("main")
        .expect("fragment entry point");
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
    shaders: ShaderSet,
    fill_vertices: Subbuffer<[FillVertex]>,
    fill_indices: Subbuffer<[u32]>,
    line_vertices: Subbuffer<[LineVertex]>,
    flat_vertices: Subbuffer<[FlatVertex]>,
    flat_indices: Subbuffer<[u32]>,
    flat_lines: Subbuffer<[FlatLineVertex]>,
    atlas_image: Option<Arc<Image>>,
    dragging_orbit: bool,
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
    /// Cursor position at left-button press (sphere-viewport presses
    /// only): release within [`CLICK_MAX_DRAG_PX`] of it counts as a click
    /// (chunk pin) rather than an orbit drag.
    press_cursor: Option<(f32, f32)>,
    /// Last FPS sample: the tools-window frame-rate clock (once per
    /// event-loop iteration — both windows present once per iteration,
    /// so this is the true per-window rate).
    last_fps_tick: Option<Instant>,
    main: Option<WindowContext>,
    tools: Option<WindowContext>,
}

/// Which OS window a context belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowKind {
    Main,
    Tools,
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
            flat_vertices,
            flat_indices,
            flat_lines,
            atlas_image: None,
            dragging_orbit: false,
            dragging_slider: false,
            dragging_density: false,
            last_frame: None,
            last_stream_sync: None,
            stream_desired: Vec::new(),
            press_cursor: None,
            last_fps_tick: None,
            main: None,
            tools: None,
        }
    }

    /// Rebuild GPU mesh buffers + reframe the camera after Regenerate.
    fn refresh_mesh(&mut self) {
        let viewer = &self.debug.viewer;
        (self.fill_vertices, self.fill_indices) = upload_fill(&self.memory_allocator, viewer);
        self.line_vertices = upload_lines(&self.memory_allocator, viewer);
        (self.flat_vertices, self.flat_indices) = upload_flat(&self.memory_allocator, viewer);
        self.flat_lines = upload_flat_lines(&self.memory_allocator, viewer);
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

    /// Recompute the hovered chunk from the current cursor: only on the
    /// Sphere Viewer screen with the cursor inside the main viewport. A
    /// miss (cursor over empty space, panel, or nav) clears the hover.
    /// Hover feeds the fill highlight + panel CHUNK readout; it never
    /// touches the mesh. Callers refresh after every cursor or camera
    /// move so the highlight tracks within one frame.
    fn update_hover(&mut self) {
        let hovered = self
            .main
            .as_ref()
            .and_then(|ctx| ctx.last_cursor.map(|cursor| (cursor, ctx.size())))
            .filter(|_| self.debug.main_screen == MainScreen::SphereViewer)
            .and_then(|((cx, cy), (w, h))| {
                let layout = app_layout(self.debug.main_screen, w, h);
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

    /// (Re)build the atlas image + this window's descriptor set when the
    /// atlas grew; upload texels when the version changed. The image is
    /// shared, but each window owns its descriptor set (allocated from
    /// its own UI pipeline layout, so binding is always compatible).
    fn sync_atlas(&mut self, kind: WindowKind) {
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
        let ctx = match kind {
            WindowKind::Main => self.main.as_mut().expect("main window must exist"),
            WindowKind::Tools => self.tools.as_mut().expect("tools window must exist"),
        };
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
            flat: build_flat_pipeline(&self.device, &self.shaders, &render_pass),
            flat_line: build_flat_line_pipeline(&self.device, &self.shaders, &render_pass),
            ui: build_ui_pipeline(&self.device, &self.shaders, &render_pass),
        };
        (render_pass, pipelines)
    }
}

struct Pipelines {
    fill: Arc<GraphicsPipeline>,
    line: Arc<GraphicsPipeline>,
    flat: Arc<GraphicsPipeline>,
    flat_line: Arc<GraphicsPipeline>,
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

impl ViewerApp {
    /// Create one OS window + swapchain + per-window GPU state.
    /// `position` offsets the window (the tools window opens beside the
    /// viewer instead of on top of it).
    fn create_window(
        &self,
        event_loop: &ActiveEventLoop,
        title: &str,
        position: Option<winit::dpi::PhysicalPosition<i32>>,
    ) -> WindowContext {
        let mut attrs = Window::default_attributes().with_title(title);
        if let Some(pos) = position {
            attrs = attrs.with_position(pos);
        }
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

    /// Look up which window an event belongs to (`None` for stale ids
    /// after a window closed).
    fn window_kind(&self, window_id: WindowId) -> Option<WindowKind> {
        if self
            .main
            .as_ref()
            .is_some_and(|ctx| ctx.window.id() == window_id)
        {
            Some(WindowKind::Main)
        } else if self
            .tools
            .as_ref()
            .is_some_and(|ctx| ctx.window.id() == window_id)
        {
            Some(WindowKind::Tools)
        } else {
            None
        }
    }
}

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once: only create windows that
        // don't have a live context yet.
        if self.main.is_none() {
            self.main = Some(self.create_window(event_loop, "PlanetCrafter — sphere viewer", None));
        }
        if self.tools.is_none() {
            self.tools = Some(self.create_window(
                event_loop,
                "PlanetCrafter — debug tools",
                Some(winit::dpi::PhysicalPosition::new(60, 60)),
            ));
        }
        // Atlas image + descriptor sets need the UI pipelines: built
        // lazily on the first frame via `sync_atlas`.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match self.window_kind(window_id) {
            Some(WindowKind::Main) => self.main_window_event(event_loop, event),
            Some(WindowKind::Tools) => self.tools_window_event(event_loop, event),
            None => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // One FPS sample per event-loop iteration (both windows present
        // once per iteration, so this is the true per-window rate).
        let now = Instant::now();
        if let Some(last) = self.last_fps_tick {
            self.debug.fps.record((now - last).as_secs_f32());
        }
        self.last_fps_tick = Some(now);
        if let Some(ctx) = self.main.as_ref() {
            ctx.window.request_redraw();
        }
        if let Some(ctx) = self.tools.as_ref() {
            ctx.window.request_redraw();
        }
    }
}
impl ViewerApp {
    /// Viewer-window events (both viewer screens).
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
                let screen = self.debug.main_screen;
                if self.dragging_slider {
                    if let Some(ctx) = self.main.as_ref() {
                        let (w, h) = ctx.size();
                        let viewer = &mut self.debug.viewer;
                        let lh = self.atlas.line_height();
                        let warn =
                            parse_subdivisions(&viewer.subdiv_field.text).is_ok_and(subdiv_warning);
                        let layout = app_layout(screen, w, h);
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
                        let layout = app_layout(screen, w, h);
                        let checker = viewer.debug_mode == DebugMode::Checker;
                        let track = match screen {
                            MainScreen::SphereViewer => {
                                sphere_left_plan(layout.left, lh, checker)
                                    .rects
                                    .density_track
                            }
                            MainScreen::UvNet => {
                                uv_left_plan(layout.left, lh, checker).rects.density_track
                            }
                        };
                        if let Some(track) = track {
                            viewer.density_slider.drag_to(track, cursor.0);
                            viewer.sync_density_from_slider();
                        }
                    }
                } else if self.dragging_orbit {
                    let last = self.main.as_ref().and_then(|ctx| ctx.last_cursor);
                    if let Some(last) = last {
                        // Player mode orbits the follow camera instead of
                        // the free global one (first/third person ignore
                        // rotation).
                        let (dx, dy) = (cursor.0 - last.0, cursor.1 - last.1);
                        if self.debug.viewer.player.active {
                            self.debug.viewer.player.rotate_camera(dx, dy);
                        } else {
                            self.camera.rotate(dx, dy);
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
                if button != MouseButton::Left {
                    return;
                }
                let pressed = state == ElementState::Pressed;
                let screen = self.debug.main_screen;
                if !pressed {
                    // Release: a press that barely traveled counts as a
                    // click — pin the hovered chunk. The release must
                    // still land in the sphere viewport (pinning only
                    // exists on the sphere screen).
                    let (cursor, in_viewport) = match self.main.as_ref() {
                        Some(ctx) => {
                            let (w, h) = ctx.size();
                            let layout = app_layout(screen, w, h);
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
                        && screen == MainScreen::SphereViewer
                        && in_viewport
                        && let Some(chunk) = self.debug.viewer.hovered
                    {
                        self.debug.viewer.toggle_pin(chunk);
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
                let layout = app_layout(screen, w, h);
                // Nav bar first.
                for (i, screen) in MainScreen::ALL.iter().enumerate() {
                    if ui::nav_button(layout.nav, i).contains(cx, cy) {
                        self.debug.select_main(*screen);
                        return;
                    }
                }
                // Viewport drag starts an orbit.
                if layout.viewport.contains(cx, cy) {
                    self.dragging_orbit = true;
                    // A sphere-screen press may end as a chunk-pin click
                    // (decided on release by travel distance).
                    if screen == MainScreen::SphereViewer {
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
                    // Screen-specific left dock first.
                    match screen {
                        MainScreen::SphereViewer => {
                            let lrects = &sphere_left_plan(layout.left, lh, checker).rects;
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
                        MainScreen::UvNet => {
                            let lrects = &uv_left_plan(layout.left, lh, checker).rects;
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
                            viewer.uvwire_cb.click(lrects.uvwire_box, cx, cy);
                            viewer.seam_cb.click(lrects.seam_box, cx, cy);
                            viewer.sync_toggles();
                        }
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
                // orbit camera (same as G/T/B/R) — sphere screen only.
                // Grid order is [[Top, Bot], [Right, Persp]] matching
                // `GlobalPreset::ALL`.
                let preset = if screen == MainScreen::SphereViewer {
                    let lh = self.atlas.line_height();
                    let checker = self.debug.viewer.debug_mode == DebugMode::Checker;
                    sphere_left_plan(layout.left, lh, checker)
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
                        app_layout(self.debug.main_screen, w, h)
                            .viewport
                            .contains(cx, cy)
                    })
                });
                if in_viewport {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                    };
                    if self.debug.viewer.player.active {
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
                    let fields_free = !viewer.subdiv_field.focused && !viewer.radius_field.focused;
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
                        self.debug.select_main_by_fkey(1);
                    }
                    PhysicalKey::Code(KeyCode::F2) => {
                        self.debug.select_main_by_fkey(2);
                    }
                    PhysicalKey::Code(KeyCode::F3) => {
                        // Reopen the tools window if the user closed it.
                        if self.tools.is_none() {
                            self.tools = Some(self.create_window(
                                event_loop,
                                "PlanetCrafter — debug tools",
                                Some(winit::dpi::PhysicalPosition::new(60, 60)),
                            ));
                        }
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
                    PhysicalKey::Code(KeyCode::KeyU) => {
                        // Player-mode toggle — but never steal keystrokes
                        // from focused fields (`u` is printable input
                        // there).
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused && !viewer.radius_field.focused {
                            self.debug.viewer.player.toggle();
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
                    PhysicalKey::Code(KeyCode::KeyP) => {
                        // Player camera cycle (active player only).
                        let viewer = &mut self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && viewer.player.active
                        {
                            self.debug.viewer.player.cycle_camera();
                        }
                    }
                    PhysicalKey::Code(
                        KeyCode::KeyG | KeyCode::KeyT | KeyCode::KeyB | KeyCode::KeyR,
                    ) => {
                        // Global camera presets (same as the VIEW buttons).
                        let viewer = &self.debug.viewer;
                        if !viewer.subdiv_field.focused
                            && !viewer.radius_field.focused
                            && self.debug.main_screen == MainScreen::SphereViewer
                        {
                            let preset = match physical_key {
                                PhysicalKey::Code(KeyCode::KeyT) => GlobalPreset::Top,
                                PhysicalKey::Code(KeyCode::KeyB) => GlobalPreset::Bottom,
                                PhysicalKey::Code(KeyCode::KeyR) => GlobalPreset::Right,
                                _ => GlobalPreset::Perspective,
                            };
                            let radius = self.debug.viewer.radius;
                            snap_global_camera(&mut self.camera, preset, radius);
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
            WindowEvent::RedrawRequested => {
                self.tick_player();
                self.draw_main();
            }
            _ => {}
        }
    }

    /// Tools-window events (FPS / Console / Inspector tabs).
    fn tools_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                // Hide the tools window; F3 on the viewer window reopens it.
                self.tools = None;
            }
            WindowEvent::Resized(_) => {
                if let Some(ctx) = self.tools.as_mut() {
                    ctx.recreate_swapchain = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ctx) = self.tools.as_mut() {
                    ctx.last_cursor = Some((position.x as f32, position.y as f32));
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if button != MouseButton::Left || state != ElementState::Pressed {
                    return;
                }
                let cursor = self.tools.as_ref().and_then(|ctx| ctx.last_cursor);
                let Some((cx, cy)) = cursor else {
                    return;
                };
                let ctx = self.tools.as_ref().expect("tools window must exist");
                let (w, h) = ctx.size();
                let layout = ui::layout_full(w, h);
                for (i, screen) in ToolsScreen::ALL.iter().enumerate() {
                    if ui::nav_button(layout.nav, i).contains(cx, cy) {
                        self.debug.select_tools(*screen);
                        return;
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state,
                        ..
                    },
                ..
            } => {
                if state != ElementState::Pressed {
                    return;
                }
                match physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::Digit1) => {
                        self.debug.select_tools_by_digit(1);
                    }
                    PhysicalKey::Code(KeyCode::Digit2) => {
                        self.debug.select_tools_by_digit(2);
                    }
                    PhysicalKey::Code(KeyCode::Digit3) => {
                        self.debug.select_tools_by_digit(3);
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.draw_tools(),
            _ => {}
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

        let layout = app_layout(self.debug.main_screen, win_w, win_h);
        let player_active = self.debug.viewer.player.active;

        // Build frame UI (atlas insertions happen here) and sync the GPU
        // atlas before recording.
        let mut items = match self.debug.main_screen {
            MainScreen::SphereViewer => {
                build_sphere_ui(&mut self.atlas, &self.debug.viewer, layout)
            }
            MainScreen::UvNet => build_uv_ui(&mut self.atlas, &self.debug.viewer, layout),
        };
        // Player dot: the walker projected through the main-view matrices
        // (sphere screen only).
        if player_active && self.debug.main_screen == MainScreen::SphereViewer {
            let player = &self.debug.viewer.player;
            let vp = layout.viewport;
            let main_vp = player.projection_matrix(vp.w / vp.h) * player.view_matrix();
            // Facing tip sized to a readable pixel length (eye-distance
            // proportional, then clamped).
            if let Some(origin) = world_to_pixels(main_vp, player.position(), vp) {
                let tip = world_to_pixels(main_vp, sphere_tip_world(player, player.position()), vp);
                draw_player_marker(&mut items, origin, tip);
            }
        }
        self.sync_atlas(WindowKind::Main);
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
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![
                        Some([0.02, 0.03, 0.08, 1.0].into()),
                        Some(ClearValue::Depth(1.0)),
                    ],
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
            // Single view: the active screen fills the main viewport.
            // The viewport transform clips output to the rect, so no
            // scissor state is needed.
            let views = [(layout.viewport, self.debug.main_screen)];
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
                if view == MainScreen::SphereViewer {
                    let aspect = vp.w / vp.h;
                    // Player mode renders the sphere through the player
                    // camera.
                    let main_vp = if player_active {
                        let player = &self.debug.viewer.player;
                        player.projection_matrix(aspect) * player.view_matrix()
                    } else {
                        self.camera.projection_matrix(aspect) * self.camera.view_matrix()
                    };
                    let mvp = main_vp.to_cols_array_2d();
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
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
                } else {
                    let mvp = flat_mvp(vp);
                    builder
                        .set_viewport(0, [viewport].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(ctx.pipelines.flat.clone())
                        .expect("pipeline must bind")
                        .bind_vertex_buffers(0, self.flat_vertices.clone())
                        .expect("vertex buffer must bind")
                        .bind_index_buffer(self.flat_indices.clone())
                        .expect("index buffer must bind")
                        .push_constants(
                            ctx.pipelines.flat.layout().clone(),
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
                            .bind_pipeline_graphics(ctx.pipelines.flat_line.clone())
                            .expect("pipeline must bind")
                            .bind_vertex_buffers(0, self.flat_lines.clone())
                            .expect("vertex buffer must bind")
                            .push_constants(
                                ctx.pipelines.flat_line.layout().clone(),
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

    /// Draw the tools window: tab UI only (no 3D scene).
    fn draw_tools(&mut self) {
        let (win_w, win_h) = match self.tools.as_ref() {
            Some(ctx) => ctx.size(),
            None => return,
        };
        if win_w < 1.0 || win_h < 1.0 {
            return;
        }
        {
            let ctx = self.tools.as_mut().expect("tools window must exist");
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

        let layout = ui::layout_full(win_w, win_h);
        // Build frame UI (atlas insertions happen here) and sync the GPU
        // atlas before recording.
        let items = build_tools_ui(&mut self.atlas, &self.debug, layout);
        self.sync_atlas(WindowKind::Tools);
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

        let ctx = self.tools.as_mut().expect("tools window must exist");
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
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![
                        Some([0.02, 0.03, 0.08, 1.0].into()),
                        Some(ClearValue::Depth(1.0)),
                    ],
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
        builder
            .end_render_pass(Default::default())
            .expect("render pass must end");
        let command_buffer = builder.build().expect("command buffer must build");

        let ctx = self.tools.as_mut().expect("tools window must exist");
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
    fn panel_plans_stay_inside_and_ordered() {
        for warn in [false, true] {
            for checker in [false, true] {
                for player_active in [false, true] {
                    let layout = ui::layout(1280.0, 720.0);
                    let sphere = sphere_left_plan(layout.left, 19.0, checker);
                    let uv = uv_left_plan(layout.left, 19.0, checker);
                    let right = right_panel_plan(layout.panel, 19.0, warn, player_active);
                    assert_eq!(right.warn_line.is_some(), warn);
                    // Player block shrinks to one "off" line when the player is off.
                    assert_eq!(right.player_lines.len(), if player_active { 4 } else { 1 });
                    // Density rows only exist in Checker mode.
                    assert_eq!(sphere.rects.density_track.is_some(), checker);
                    assert_eq!(sphere.density_label.is_some(), checker);
                    assert_eq!(uv.rects.density_track.is_some(), checker);
                    assert_eq!(uv.density_label.is_some(), checker);
                    // Every widget rect stays inside its dock.
                    for rect in [
                        sphere.rects.shader_button,
                        sphere.rects.wire_box,
                        sphere.rects.pent_box,
                        sphere.rects.seam_box,
                        uv.rects.shader_button,
                        uv.rects.uvwire_box,
                        uv.rects.seam_box,
                    ] {
                        assert!(rect.x >= layout.left.x, "{rect:?}");
                        assert!(
                            rect.x + rect.w <= layout.left.x + layout.left.w + 1e-3,
                            "{rect:?}"
                        );
                    }
                    for rect in sphere
                        .rects
                        .density_track
                        .into_iter()
                        .chain(uv.rects.density_track)
                    {
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
                    for row in sphere.rects.preset_grid {
                        for button in row {
                            assert!(button.x >= layout.left.x, "{button:?}");
                            assert!(
                                button.x + button.w <= layout.left.x + layout.left.w + 1e-3,
                                "{button:?}"
                            );
                        }
                    }
                    // Sphere dock flows top-down: view, presets, shader, overlays.
                    assert!(sphere.view_header.y < sphere.preset_hint.y);
                    assert!(sphere.rects.shader_button.y > sphere.shader_header.y);
                    assert!(sphere.shader_hint.y > sphere.rects.shader_button.y);
                    assert!(sphere.overlay_header.y < sphere.rects.wire_box.y);
                    assert!(sphere.rects.wire_box.y < sphere.rects.pent_box.y);
                    assert!(sphere.rects.pent_box.y < sphere.rects.seam_box.y);
                    // Preset grid: top row above bottom row, left column left of right.
                    assert!(sphere.rects.preset_grid[0][0].y < sphere.rects.preset_grid[1][0].y);
                    assert!(sphere.rects.preset_grid[0][0].x < sphere.rects.preset_grid[0][1].x);
                    // UV dock flows top-down: info, shader, overlays.
                    assert!(uv.uv_header.y < uv.uv_info.y);
                    assert!(uv.uv_info.y < uv.shader_header.y);
                    assert!(uv.shader_header.y < uv.rects.shader_button.y);
                    assert!(uv.overlay_header.y < uv.rects.uvwire_box.y);
                    assert!(uv.rects.uvwire_box.y < uv.rects.seam_box.y);
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
        let viewer = SphereViewerState::new();
        let layout = ui::layout(1280.0, 720.0);
        let joined = |items: &UiItems| {
            items
                .texts
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Sphere screen: presets + sphere overlays, no UV-only controls.
        let sphere = joined(&build_sphere_ui(&mut atlas, &viewer, layout));
        assert!(!sphere.is_empty());
        for needle in [
            "Sphere Viewer",
            "UV Net",
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
            assert!(sphere.contains(needle), "sphere ui missing {needle}");
        }
        for absent in [
            "Swap view (V)",
            "click/V",
            "UV wire",
            "UV DEBUG",
            "Checker density:",
            "move:     WASD",
            "cam:      P cycles",
        ] {
            assert!(!sphere.contains(absent), "sphere ui leaks {absent}");
        }
        // UV screen: net info + UV overlays, no sphere-only controls.
        let uv = joined(&build_uv_ui(&mut atlas, &viewer, layout));
        for needle in [
            "Sphere Viewer",
            "UV Net",
            "UV NET",
            "icosa net",
            "SHADER",
            "Shader: Lit",
            "UV wire",
            "Seams",
            "OVERLAYS",
            "INPUTS",
            "SELECTION",
            "STATS",
            "click cycles",
        ] {
            assert!(uv.contains(needle), "uv ui missing {needle}");
        }
        for absent in [
            "Swap view (V)",
            "Wireframe",
            "Pentagons",
            "Camera presets",
            "UV DEBUG",
            "Checker density:",
        ] {
            assert!(!uv.contains(absent), "uv ui leaks {absent}");
        }
        // Checker mode reveals the density slider on both screens.
        let mut checker_viewer = SphereViewerState::with_values(1, 1.0);
        checker_viewer.debug_mode = DebugMode::Checker;
        let sphere_checker = joined(&build_sphere_ui(&mut atlas, &checker_viewer, layout));
        let uv_checker = joined(&build_uv_ui(&mut atlas, &checker_viewer, layout));
        assert!(sphere_checker.contains("Checker density:"));
        assert!(uv_checker.contains("Checker density:"));
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

        let mut viewer = SphereViewerState::with_values(1, 1.0);
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
        let mut viewer = SphereViewerState::with_values(1, 1.0);
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
        let mut viewer = SphereViewerState::with_values(1, 1.0);
        viewer.player.toggle();
        viewer.player.set_keys(MoveKeys {
            east: true,
            ..MoveKeys::default()
        });
        let desired = visible_hemisphere(&viewer.mesh, viewer.player.position().to_array());
        viewer.player.update(0.05, &desired);
        let layout = ui::layout(1280.0, 720.0);
        let items = build_sphere_ui(&mut atlas, &viewer, layout);
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
    fn tools_ui_shows_fps_numbers_and_placeholders() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let mut app = DebugApp::new();
        for _ in 0..120 {
            app.fps.record(1.0 / 60.0);
        }
        let layout = ui::layout_full(1280.0, 720.0);
        let joined = |items: &UiItems| {
            items
                .texts
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        // FPS tab: live numbers + sparkline label; nav shows all tabs.
        app.select_tools(ToolsScreen::Fps);
        let fps_tab = joined(&build_tools_ui(&mut atlas, &app, layout));
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
            assert!(fps_tab.contains(needle), "fps tab missing {needle}");
        }
        // Console / Inspector tabs are placeholders for now.
        app.select_tools(ToolsScreen::Console);
        let console = joined(&build_tools_ui(&mut atlas, &app, layout));
        assert!(console.contains("Console"));
        assert!(console.contains("not implemented yet"));
        app.select_tools(ToolsScreen::Inspector);
        let inspector = joined(&build_tools_ui(&mut atlas, &app, layout));
        assert!(inspector.contains("Inspector"));
        assert!(inspector.contains("not implemented yet"));
    }

    #[test]
    fn ui_vertices_cover_all_items() {
        let mut atlas = GlyphAtlas::new(UI_PX);
        let viewer = SphereViewerState::new();
        let layout = ui::layout(1280.0, 720.0);
        let items = build_sphere_ui(&mut atlas, &viewer, layout);
        let verts = ui_items_to_vertices(&items, &mut atlas);
        let text_quads: usize = items.texts.iter().map(|t| t.text.chars().count()).sum();
        assert_eq!(verts.len(), items.solids.len() * 6 + text_quads * 6);
        assert!((verts.len() as u64) < MAX_UI_VERTS);
    }
}
