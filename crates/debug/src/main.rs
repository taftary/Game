//! `game_debug` sphere viewer binary (`plans/debug-sphere-viewer`).
//!
//! - `--headless`: GPU-free path (CI-safe — never loads the Vulkan
//!   loader): builds the default N=6/R=1.0 viewer mesh through the
//!   `game_debug` lib and prints stats.
//! - Windowed (default): `winit` window + `vulkano` boot mirroring
//!   `game_tools` (Instance → Surface → Device → Swapchain, Vulkan 1.1
//!   cap), Sphere Viewer screen (orbit camera, filled dual-cell mesh,
//!   wireframe overlay, pentagon highlight, inputs panel, read-only
//!   stats) plus FPS/Console/Inspector placeholder screens, F1–F4/click
//!   nav with preserved viewer state.
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
use game_debug::sphere_viewer::SphereViewerState;
use game_debug::text::GlyphAtlas;
use game_debug::ui::{self, Layout, Rect};
use game_engine::render::{
    OrbitCamera, PlanetVertex, ShaderKind, compile_glsl_to_spirv, create_instance, device_score,
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
layout(push_constant) uniform PushConstants {
    mat4 mvp;
    float highlight;
} pc;
layout(location = 0) out vec3 v_normal;
// `flat`: tint is a per-cell flag. Fan triangles are (center, corner_i,
// corner_i+1), and Vulkan's default provoking vertex is the first one —
// so every triangle takes its cell center's tint and pentagon sites read
// as crisp tinted faces instead of gradient blobs bleeding into
// neighboring hexagons.
layout(location = 1) flat out float v_tint;
void main() {
    gl_Position = pc.mvp * vec4(position, 1.0);
    // The normal attribute is radial outward (position / radius) —
    // pass it through. An earlier `-normal` hack lit the far side's
    // inner faces, which were wrongly visible until the fill
    // pipeline's front face matched the Y-down projection
    // (issue-2026-09-14-2113).
    v_normal = normal;
    v_tint = tint * pc.highlight;
}"##;

const FILL_FRAG: &str = r"#version 450
layout(location = 0) in vec3 v_normal;
layout(location = 1) flat in float v_tint;
layout(location = 0) out vec4 f_color;
void main() {
    vec3 n = normalize(v_normal);
    vec3 sun = normalize(vec3(0.5, 0.8, 0.6));
    float diffuse = max(dot(n, sun), 0.0);
    vec3 hex_color = vec3(0.25, 0.45, 0.75);
    vec3 pent_color = vec3(1.0, 0.8, 0.2);
    vec3 base = mix(hex_color, pent_color, v_tint);
    f_color = vec4(base * (0.25 + 0.75 * diffuse), 1.0);
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

/// Fill push constants: MVP + pentagon-highlight flag (68 B < 128 B floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct FillPush {
    mvp: [[f32; 4]; 4],
    highlight: f32,
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
/// stats, exit 0. Never touches `VulkanLibrary` or `EventLoop`.
fn run_headless() -> i32 {
    let viewer = SphereViewerState::new();
    let stats = &viewer.stats;
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

/// Widget rects inside the inputs panel.
struct PanelRects {
    subdiv_field: Rect,
    subdiv_track: Rect,
    radius_field: Rect,
    regen_button: Rect,
    wire_box: Rect,
    pent_box: Rect,
}

/// Full panel row plan: widget rects + label/text rows in draw order.
/// Built with a single cursor, so hit-testing and drawing always agree.
/// `warn` reserves the extra above-N=6 warning row.
struct PanelPlan {
    rects: PanelRects,
    inputs_header: Rect,
    subdiv_label: Rect,
    subdiv_hint: Rect,
    warn_line: Option<Rect>,
    radius_label: Rect,
    radius_hint: Rect,
    wire_label: Rect,
    pent_label: Rect,
    stats_header: Rect,
    stat_lines: [Rect; 5],
}

fn panel_plan(panel: Rect, lh: f32, warn: bool) -> PanelPlan {
    let mut rows = ui::PanelRows::new(panel, 8.0);
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
    let stats_header = rows.next(lh, 4.0);
    let stat_lines = [
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
            subdiv_field,
            subdiv_track,
            radius_field,
            regen_button,
            wire_box: check_box(wire_label),
            pent_box: check_box(pent_label),
        },
        inputs_header,
        subdiv_label,
        subdiv_hint,
        warn_line,
        radius_label,
        radius_hint,
        wire_label,
        pent_label,
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

    // Read-only stats.
    text_row(&mut items, plan.stats_header, "STATS".to_owned(), C_DIM);
    let stats = &viewer.stats;
    for (row, line) in plan.stat_lines.iter().zip([
        format!("cells:     {}", fmt_int(stats.cells)),
        format!("corners:    {}", fmt_int(stats.corners)),
        format!("pentagons:  {}", stats.pentagons),
        format!("hash:       {}", stats.hash8),
        format!("gen:        {:.1} ms", stats.gen_ms),
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
    let vertex_input_state = PlanetVertex::per_vertex()
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
) -> (Subbuffer<[PlanetVertex]>, Subbuffer<[u32]>) {
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
        viewer.fill_vertices.iter().copied(),
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
    fill_vertices: Subbuffer<[PlanetVertex]>,
    fill_indices: Subbuffer<[u32]>,
    line_vertices: Subbuffer<[LineVertex]>,
    atlas_image: Option<Arc<Image>>,
    atlas_set: Option<Arc<DescriptorSet>>,
    dragging_orbit: bool,
    dragging_slider: bool,
    last_cursor: Option<(f32, f32)>,
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
            atlas_image: None,
            atlas_set: None,
            dragging_orbit: false,
            dragging_slider: false,
            last_cursor: None,
            rcx: None,
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
            "sphere viewer mesh regenerated",
        );
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
            ui: build_ui_pipeline(&self.device, &render_pass),
        };
        (render_pass, pipelines)
    }
}

struct Pipelines {
    fill: Arc<GraphicsPipeline>,
    line: Arc<GraphicsPipeline>,
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
                } else if self.dragging_orbit
                    && let Some(last) = self.last_cursor
                {
                    self.camera.rotate(cursor.0 - last.0, cursor.1 - last.1);
                }
                self.last_cursor = Some(cursor);
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                let pressed = state == ElementState::Pressed;
                if !pressed {
                    self.dragging_orbit = false;
                    self.dragging_slider = false;
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
                    viewer.subdiv_field.click(rects.subdiv_field, cx, cy);
                    viewer.radius_field.click(rects.radius_field, cx, cy);
                    if rects.subdiv_track.contains(cx, cy) {
                        self.dragging_slider = true;
                        viewer.subdiv_slider.drag_to(rects.subdiv_track, cx);
                        viewer.sync_field_from_slider();
                    }
                    let regenerated = viewer.can_regenerate()
                        && rects.regen_button.contains(cx, cy)
                        && viewer.regenerate().is_ok();
                    viewer.wire_cb.click(rects.wire_box, cx, cy);
                    viewer.pent_cb.click(rects.pent_box, cx, cy);
                    viewer.sync_toggles();
                    regenerated
                };
                if regenerated {
                    self.refresh_mesh();
                }
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
            let vp = layout.viewport;
            if vp.w >= 1.0 && vp.h >= 1.0 {
                let aspect = vp.w / vp.h;
                let mvp = (self.camera.projection_matrix(aspect) * self.camera.view_matrix())
                    .to_cols_array_2d();
                builder
                    .set_viewport(
                        0,
                        [Viewport {
                            offset: [vp.x, vp.y],
                            extent: [vp.w, vp.h],
                            depth_range: 0.0..=1.0,
                        }]
                        .into_iter()
                        .collect(),
                    )
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
                            highlight: if self.debug.viewer.pentagons {
                                1.0
                            } else {
                                0.0
                            },
                        },
                    )
                    .expect("fill push constants must upload");
                unsafe { builder.draw_indexed(self.fill_indices.len() as u32, 1, 0, 0, 0) }
                    .expect("fill draw must record");
                if self.debug.viewer.wireframe {
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
                    if !self.debug.viewer.lines.is_empty() {
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
    fn panel_plan_stays_inside_and_ordered() {
        for warn in [false, true] {
            let layout = ui::layout(1280.0, 720.0);
            let plan = panel_plan(layout.panel, 19.0, warn);
            assert_eq!(plan.warn_line.is_some(), warn);
            let rects = &plan.rects;
            for rect in [
                rects.subdiv_field,
                rects.subdiv_track,
                rects.radius_field,
                rects.regen_button,
                rects.wire_box,
                rects.pent_box,
            ] {
                assert!(rect.x >= layout.panel.x, "{rect:?}");
                assert!(
                    rect.x + rect.w <= layout.panel.x + layout.panel.w + 1e-3,
                    "{rect:?}"
                );
            }
            assert!(rects.subdiv_field.y < rects.subdiv_track.y);
            assert!(rects.subdiv_track.y < rects.radius_field.y);
            assert!(rects.radius_field.y < rects.regen_button.y);
            assert!(rects.regen_button.y < rects.wire_box.y);
            assert!(rects.wire_box.y < rects.pent_box.y);
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
            "STATS",
            "cells:",
            "hash:",
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
