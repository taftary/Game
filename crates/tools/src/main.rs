//! `game_tools` renderer smoke (M1).
//!
//! - `--headless`: GPU-free path (CI-safe — never loads the Vulkan
//!   loader): generates the tier planet and prints
//!   `tier subdiv cells tris hash gen_ms`.
//! - Windowed (default): `winit` window + `vulkano` boot via
//!   `engine::render::boot` (Instance → Surface → Device → Swapchain),
//!   tier planet uploaded to GPU buffers, naga-compiled shaded pipeline,
//!   drag-orbit / wheel-zoom camera, `1/2/3` to force Low/Med/High tiers,
//!   explicit swapchain recreation on resize.
//!
//! Usage: `game_tools [--tier low|medium|high] [--seed N] [--radius R]
//!   [--headless] [--log-depth]`.
//!
//! `game_tools catalog --out <dir> ...`: catalog cooker (see the
//! `catalog_cook` module): deterministic Gaia-schema tile writer,
//! GPU-free.
//!
//! `--log-depth` (windowed only) switches the planet pipeline to the
//! log-depth vertex variant (`engine::render::depth`, ADR-016) with a
//! `D32_SFLOAT` depth attachment. Default pixels are unchanged without
//! the flag.

use std::sync::Arc;
use std::time::Instant;

mod catalog_cook;

use game_engine::render::{
    FOV_Y, IndexedMesh, OrbitCamera, PLANET_FRAG, PLANET_VERT, PlanetVertex, QualityTier,
    SeededPlanet, ShaderKind, compile_glsl_to_spirv, create_instance, device_score,
    log_physical_device, planet_vert_logdepth, required_device_extensions,
};
use glam::Vec3;
use glam::camera::rh::proj::directx::perspective;
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo, SubpassBeginInfo,
    SubpassContents,
};
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags};
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::instance::Instance;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::depth_stencil::{CompareOp, DepthState, DepthStencilState};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::{CullMode, FrontFace, RasterizationState};
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{
    DynamicState, GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo,
};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader::{ShaderModule, ShaderModuleCreateInfo};
use vulkano::swapchain::SwapchainPresentInfo;
use vulkano::swapchain::{Surface, Swapchain, SwapchainCreateInfo, acquire_next_image};
use vulkano::sync::{self, GpuFuture};
use vulkano::{Validated, VulkanError, VulkanLibrary};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// Note: the global allocator switch lives in `game_engine` (feature
// `game_engine/mimalloc`, ADR-009) — exactly one `#[global_allocator]`
// may exist per binary, so this crate only passes the feature through
// (see `Cargo.toml`) and never defines its own.

const DEFAULT_SEED: u64 = 1337;
const DEFAULT_RADIUS: f32 = 1.0;
/// Depth format for the log-depth variant. `D32_SFLOAT` is a mandatory
/// Vulkan 1.1 format (no extension, inside the ADR-007 floor); the float
/// buffer is required because D16 quanta are coarser than the log slope
/// at decade range (`depth::oracle_log_depth_needs_float_buffer_at_decade_range`).
const LOG_DEPTH_FORMAT: Format = Format::D32_SFLOAT;
/// Far plane fed to the log-depth shader. Must match
/// `OrbitCamera::projection_matrix` (near 0.05, far 1000.0) so the
/// encoding normalizes against the same range the matrix maps.
const SMOKE_LOG_FAR: f32 = 1000.0;
/// Tight far plane of the near-band demo pass (same projection family
/// as the main camera — `directx::perspective`, un-flipped — just
/// tighter, per the band scheduler's Near contract).
const NEAR_PASS_FAR: f32 = 10.0;
/// Near-band demo quad: world-space half-size. Placed between the
/// camera and the planet on the view axis, so the composite must draw
/// it over the planet disc (near-over-far painter order across bands).
const DEMO_QUAD_HALF: f32 = 0.35;
/// Fraction along eye→target where the demo quad sits (0 = eye, 1 =
/// planet center). 0.45 lands it well in front of the nearest surface.
const DEMO_QUAD_T: f32 = 0.45;

/// MVP push-constant block (matches `PLANET_VERT`).
#[derive(BufferContents)]
#[repr(C)]
struct MvpData {
    mvp: [[f32; 4]; 4],
}

/// MVP + log far-plane push-constant block (matches the composed
/// `planet_vert_logdepth(PLANET_VERT)` source: 64 B matrix + 4 B float =
/// 68 B, under the 128 B Vulkan 1.1 floor).
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
struct LogMvpData {
    mvp: [[f32; 4]; 4],
    log_far: f32,
}

struct Args {
    tier: QualityTier,
    seed: u64,
    radius: f32,
    headless: bool,
    log_depth: bool,
}

fn usage() -> &'static str {
    "usage: game_tools [--tier low|medium|high] [--seed N] [--radius R] [--headless] [--log-depth]"
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut tier = QualityTier::Medium;
    let mut seed = DEFAULT_SEED;
    let mut radius = DEFAULT_RADIUS;
    let mut headless = false;
    let mut log_depth = false;
    let mut iter = argv.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--tier" => {
                let value = iter.next().ok_or("--tier needs a value".to_owned())?;
                tier = value.parse().map_err(|error| format!("{error}"))?;
            }
            "--seed" => {
                let value = iter.next().ok_or("--seed needs a value".to_owned())?;
                seed = value
                    .parse()
                    .map_err(|_| format!("invalid --seed {value:?}"))?;
            }
            "--radius" => {
                let value = iter.next().ok_or("--radius needs a value".to_owned())?;
                radius = value
                    .parse()
                    .map_err(|_| format!("invalid --radius {value:?}"))?;
            }
            "--headless" => headless = true,
            "--log-depth" => log_depth = true,
            "--help" | "-h" => return Err(usage().to_owned()),
            other => {
                return Err(format!(
                    "unknown argument {other:?}\n{usage}",
                    usage = usage()
                ));
            }
        }
    }
    if !(radius.is_finite() && radius > 0.0) {
        return Err(format!(
            "--radius must be positive and finite, got {radius}"
        ));
    }
    if log_depth && headless {
        return Err(
            "--log-depth needs the windowed path (it is a GPU pipeline variant)".to_owned(),
        );
    }
    Ok(Args {
        tier,
        seed,
        radius,
        headless,
        log_depth,
    })
}

/// GPU-free smoke: generate the tier planet, print stats, exit 0.
/// Never touches `VulkanLibrary` or `EventLoop` — CI-safe.
fn run_headless(args: &Args) -> i32 {
    let started = Instant::now();
    let planet = SeededPlanet::generate(args.seed, args.tier, args.radius);
    let gen_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "tier={} subdiv={} seed={} radius={} cells={} corners={} tris={} hash={:016x} gen_ms={:.1}",
        args.tier.name(),
        args.tier.subdivisions(),
        args.seed,
        args.radius,
        planet.mesh().cell_count(),
        planet.mesh().corner_count(),
        planet.triangle_count(),
        planet.descriptor_hash(),
        gen_ms,
    );
    0
}

fn upload_mesh(
    allocator: &Arc<StandardMemoryAllocator>,
    mesh: &IndexedMesh,
) -> (
    Subbuffer<[game_engine::render::PlanetVertex]>,
    Subbuffer<[u32]>,
) {
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
        mesh.vertices.iter().copied(),
    )
    .expect("planet vertex buffer upload must succeed");
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
        mesh.indices.iter().copied(),
    )
    .expect("planet index buffer upload must succeed");
    (vertices, indices)
}

/// Builds the near-band demo quad: a camera-facing gold square on the
/// initial view axis between the eye and the planet. Static in world
/// space (orbiting may leave it behind — it is compositing evidence,
/// not scene content).
fn build_demo_quad(
    allocator: &Arc<StandardMemoryAllocator>,
    camera: &OrbitCamera,
) -> (Subbuffer<[PlanetVertex]>, Subbuffer<[u32]>) {
    let eye = camera.eye();
    let dir = (Vec3::ZERO - eye).normalize();
    let center = eye + dir * (camera.distance() * DEMO_QUAD_T);
    let right = dir.cross(Vec3::Y).normalize();
    let up = right.cross(dir).normalize();
    let normal = -dir;
    let corner = |rx: f32, uy: f32| PlanetVertex {
        position: (center + right * (rx * DEMO_QUAD_HALF) + up * (uy * DEMO_QUAD_HALF)).into(),
        normal: normal.into(),
        tint: 1.0,
        // The engine planet shader ignores `uv`; the debug shader reads
        // it — the demo quad never reaches the debug pipeline.
        uv: [0.0, 0.0],
    };
    // Bottom-left, bottom-right, top-right, top-left as seen from the
    // camera: (0,1,2),(0,2,3) is CCW front-facing under the un-flipped
    // projection (same winding contract as the planet mesh).
    let vertices = [
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
    ];
    let vertex_buffer = Buffer::from_iter(
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
        vertices,
    )
    .expect("demo quad vertex buffer upload must succeed");
    let index_buffer = Buffer::from_iter(
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
        [0u32, 1, 2, 0, 2, 3],
    )
    .expect("demo quad index buffer upload must succeed");
    (vertex_buffer, index_buffer)
}

fn log_tier(tier: QualityTier) {
    tracing::info!(
        tier = tier.name(),
        subdivisions = tier.subdivisions(),
        resolution_scale = tier.resolution_scale(),
        shadows = tier.shadows(),
        "quality tier forced (resolution scale applies from M6 dynamic resolution)",
    );
}

struct App {
    seed: u64,
    radius: f32,
    tier: QualityTier,
    log_depth: bool,
    planet: SeededPlanet,
    camera: OrbitCamera,
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    vertex_buffer: Subbuffer<[game_engine::render::PlanetVertex]>,
    index_buffer: Subbuffer<[u32]>,
    /// Near-band demo quad (`Some` only in `--log-depth` mode).
    quad_vertex_buffer: Option<Subbuffer<[game_engine::render::PlanetVertex]>>,
    quad_index_buffer: Option<Subbuffer<[u32]>>,
    dragging: bool,
    last_cursor: Option<(f32, f32)>,
    rcx: Option<RenderContext>,
}

/// Near-band demo pass (`--log-depth` mode only): its own render pass
/// (color Load = composite over the planet band, depth Clear = fresh
/// tight range) plus the linear-depth quad pipeline.
struct NearPass {
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    pipeline: Arc<GraphicsPipeline>,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    /// Depth view for the log-depth variant (`None` on the default
    /// depth-less path). Shared by all framebuffers; recreated on
    /// resize next to them.
    depth_view: Option<Arc<ImageView>>,
    pipeline: Arc<GraphicsPipeline>,
    /// Near-band composite pass (`Some` only in `--log-depth` mode).
    near: Option<NearPass>,
    viewport: Viewport,
    recreate_swapchain: bool,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
}

impl App {
    fn new(event_loop: &EventLoop<()>, args: &Args) -> Self {
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
        log_tier(args.tier);

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

        let planet = SeededPlanet::generate(args.seed, args.tier, args.radius);
        tracing::info!(
            cells = planet.mesh().cell_count(),
            tris = planet.triangle_count(),
            hash = format!("{:016x}", planet.descriptor_hash()),
            "seeded planet generated",
        );
        let mesh = planet.to_indexed_mesh();
        let (vertex_buffer, index_buffer) = upload_mesh(&memory_allocator, &mesh);
        let camera = OrbitCamera::framing_planet(args.radius);
        // Near-band demo quad rides the initial view axis (log-depth
        // mode only); the default smoke uploads no extra geometry.
        let (quad_vertex_buffer, quad_index_buffer) = if args.log_depth {
            let (vertices, indices) = build_demo_quad(&memory_allocator, &camera);
            (Some(vertices), Some(indices))
        } else {
            (None, None)
        };

        App {
            seed: args.seed,
            radius: args.radius,
            tier: args.tier,
            log_depth: args.log_depth,
            planet,
            camera,
            instance,
            device,
            queue,
            memory_allocator,
            command_buffer_allocator,
            vertex_buffer,
            index_buffer,
            quad_vertex_buffer,
            quad_index_buffer,
            dragging: false,
            last_cursor: None,
            rcx: None,
        }
    }

    /// Regenerate the planet at a new tier (keys 1/2/3 force tiers).
    fn force_tier(&mut self, tier: QualityTier) {
        if tier == self.tier {
            return;
        }
        self.tier = tier;
        self.planet = SeededPlanet::generate(self.seed, tier, self.radius);
        let mesh = self.planet.to_indexed_mesh();
        (self.vertex_buffer, self.index_buffer) = upload_mesh(&self.memory_allocator, &mesh);
        log_tier(tier);
        tracing::info!(
            cells = self.planet.mesh().cell_count(),
            tris = self.planet.triangle_count(),
            "planet regenerated for tier",
        );
    }

    /// Shared planet-pipeline constructor: base or log-depth vertex
    /// words over `render_pass`, optional D32F depth test. Winding is
    /// always `CounterClockwise` + `Back` (rendering invariants).
    fn build_planet_pipeline(
        device: &Arc<Device>,
        render_pass: &Arc<RenderPass>,
        vert_words: &[u32],
        frag_words: &[u32],
        with_depth: bool,
    ) -> Arc<GraphicsPipeline> {
        // SAFETY: SPIR-V words come from `compile_glsl_to_spirv`, which
        // runs naga validation and pins the `main` entry point; the module
        // is used only with the matching vertex input layout below.
        let vert_module =
            unsafe { ShaderModule::new(device.clone(), ShaderModuleCreateInfo::new(vert_words)) }
                .expect("vertex shader module must load");
        let frag_module =
            unsafe { ShaderModule::new(device.clone(), ShaderModuleCreateInfo::new(frag_words)) }
                .expect("fragment shader module must load");
        let vs = vert_module.entry_point("main").expect("vertex entry point");
        let fs = frag_module
            .entry_point("main")
            .expect("fragment entry point");
        let vertex_input_state = game_engine::render::PlanetVertex::per_vertex()
            .definition(&vs)
            .expect("planet vertex layout must match shader");
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
        let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
        // Log-depth changes depth writes only: winding stays
        // `CounterClockwise` + `Back` and the projection stays un-flipped
        // (rendering invariants — the depth variant must render the same
        // image modulo depth resolution).
        let depth_stencil_state = with_depth.then(|| DepthStencilState {
            depth: Some(DepthState {
                write_enable: true,
                compare_op: CompareOp::Less,
            }),
            ..Default::default()
        });
        GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: stages.into_iter().collect(),
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState {
                    // Convex planet: backface culling resolves
                    // visibility, no depth buffer in M1. Front face is
                    // CounterClockwise because
                    // `OrbitCamera::projection_matrix` outputs
                    // framebuffer-true NDC (no Y-flip), so the
                    // CCW-outward fans classify as CCW directly
                    // (same convention as the debug viewer —
                    // issue-2026-09-14-2113).
                    cull_mode: CullMode::Back,
                    front_face: FrontFace::CounterClockwise,
                    ..Default::default()
                }),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    subpass.num_color_attachments(),
                    ColorBlendAttachmentState::default(),
                )),
                depth_stencil_state,
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(subpass.into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )
        .expect("planet graphics pipeline must create")
    }

    fn build_pipeline(
        &self,
        swapchain: &Arc<Swapchain>,
    ) -> (Arc<RenderPass>, Arc<GraphicsPipeline>) {
        // Log-depth variant composes the single-authored `PLANET_VERT`
        // body with the depth epilogue (engine::render::depth) — the
        // default path compiles the base source untouched, so default
        // pixels are bit-identical to the M1 smoke.
        let vert_source = if self.log_depth {
            planet_vert_logdepth(PLANET_VERT)
        } else {
            PLANET_VERT.to_owned()
        };
        let vert_words = compile_glsl_to_spirv(ShaderKind::Vertex, &vert_source)
            .expect("planet vertex shader must compile");
        let frag_words = compile_glsl_to_spirv(ShaderKind::Fragment, PLANET_FRAG)
            .expect("planet fragment shader must compile");

        let render_pass = if self.log_depth {
            vulkano::single_pass_renderpass!(
                self.device.clone(),
                attachments: {
                    color: {
                        format: swapchain.image_format(),
                        samples: 1,
                        load_op: Clear,
                        store_op: Store,
                    },
                    depth: {
                        format: LOG_DEPTH_FORMAT,
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
            .expect("render pass must create")
        } else {
            vulkano::single_pass_renderpass!(
                self.device.clone(),
                attachments: {
                    color: {
                        format: swapchain.image_format(),
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
            .expect("render pass must create")
        };
        let pipeline = Self::build_planet_pipeline(
            &self.device,
            &render_pass,
            &vert_words,
            &frag_words,
            self.log_depth,
        );
        (render_pass, pipeline)
    }

    /// Near-band demo pass (`--log-depth` mode only): color `Load`
    /// (composite over the planet band) + depth `Clear` (fresh tight
    /// range), with the base linear vertex shader — the Near bucket of
    /// the band plan executing on GPU.
    fn build_near_pass(
        &self,
        swapchain: &Arc<Swapchain>,
    ) -> (Arc<RenderPass>, Arc<GraphicsPipeline>) {
        let vert_words = compile_glsl_to_spirv(ShaderKind::Vertex, PLANET_VERT)
            .expect("planet vertex shader must compile");
        let frag_words = compile_glsl_to_spirv(ShaderKind::Fragment, PLANET_FRAG)
            .expect("planet fragment shader must compile");
        let render_pass = vulkano::single_pass_renderpass!(
            self.device.clone(),
            attachments: {
                color: {
                    format: swapchain.image_format(),
                    samples: 1,
                    load_op: Load,
                    store_op: Store,
                },
                depth: {
                    format: LOG_DEPTH_FORMAT,
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
        .expect("near render pass must create");
        let pipeline =
            Self::build_planet_pipeline(&self.device, &render_pass, &vert_words, &frag_words, true);
        (render_pass, pipeline)
    }
}

fn create_depth_view(allocator: &Arc<StandardMemoryAllocator>, extent: [u32; 2]) -> Arc<ImageView> {
    let image = Image::new(
        allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: LOG_DEPTH_FORMAT,
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

fn window_size_dependent_setup(
    images: &[Arc<Image>],
    render_pass: &Arc<RenderPass>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    log_depth: bool,
) -> (Vec<Arc<Framebuffer>>, Option<Arc<ImageView>>) {
    let depth_view = log_depth.then(|| {
        let extent = images
            .first()
            .expect("swapchain must yield an image")
            .extent();
        create_depth_view(memory_allocator, [extent[0], extent[1]])
    });
    let framebuffers = images
        .iter()
        .map(|image| {
            let view = ImageView::new_default(image.clone()).expect("swapchain image view");
            let mut attachments = vec![view];
            if let Some(depth) = depth_view.clone() {
                attachments.push(depth);
            }
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments,
                    ..Default::default()
                },
            )
            .expect("framebuffer must create")
        })
        .collect();
    (framebuffers, depth_view)
}

/// Framebuffers for the near-band composite pass: same swapchain image
/// plus the shared depth view, under the near render pass.
fn build_near_framebuffers(
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
            .expect("near framebuffer must create")
        })
        .collect()
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes().with_title("PlanetCrafter — renderer smoke"),
                )
                .expect("smoke window must create"),
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
        let (render_pass, pipeline) = self.build_pipeline(&swapchain);
        let (framebuffers, depth_view) = window_size_dependent_setup(
            &images,
            &render_pass,
            &self.memory_allocator,
            self.log_depth,
        );
        // Near-band composite pass, log-depth mode only: Backdrop (the
        // clear above) → Mid (log planet) → Near (linear quad) is the
        // band plan executing on GPU.
        let near = if self.log_depth {
            let (near_pass, near_pipeline) = self.build_near_pass(&swapchain);
            let near_framebuffers = build_near_framebuffers(
                &images,
                &near_pass,
                depth_view
                    .as_ref()
                    .expect("log-depth mode must create a depth view"),
            );
            Some(NearPass {
                render_pass: near_pass,
                framebuffers: near_framebuffers,
                pipeline: near_pipeline,
            })
        } else {
            None
        };
        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: window_size.into(),
            depth_range: 0.0..=1.0,
        };
        self.rcx = Some(RenderContext {
            window,
            swapchain,
            render_pass,
            framebuffers,
            depth_view,
            pipeline,
            near,
            viewport,
            recreate_swapchain: false,
            previous_frame_end: Some(sync::now(self.device.clone()).boxed()),
        });
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
            WindowEvent::MouseInput { button, state, .. } => {
                if button == MouseButton::Left {
                    self.dragging = state == ElementState::Pressed;
                    if !self.dragging {
                        self.last_cursor = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.dragging {
                    let current = (position.x as f32, position.y as f32);
                    if let Some(last) = self.last_cursor {
                        self.camera.rotate(current.0 - last.0, current.1 - last.1);
                    }
                    self.last_cursor = Some(current);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                };
                self.camera.zoom(scroll);
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
                if state == ElementState::Pressed {
                    match physical_key {
                        PhysicalKey::Code(KeyCode::Digit1) => self.force_tier(QualityTier::Low),
                        PhysicalKey::Code(KeyCode::Digit2) => self.force_tier(QualityTier::Medium),
                        PhysicalKey::Code(KeyCode::Digit3) => self.force_tier(QualityTier::High),
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let rcx = self.rcx.as_mut().expect("render context must exist");
                let window_size = rcx.window.inner_size();
                if window_size.width == 0 || window_size.height == 0 {
                    return;
                }
                rcx.previous_frame_end
                    .as_mut()
                    .expect("frame future")
                    .cleanup_finished();
                if rcx.recreate_swapchain {
                    let (new_swapchain, new_images) = rcx
                        .swapchain
                        .recreate(SwapchainCreateInfo {
                            image_extent: window_size.into(),
                            ..rcx.swapchain.create_info()
                        })
                        .expect("swapchain recreation must succeed");
                    rcx.swapchain = new_swapchain;
                    let (framebuffers, depth_view) = window_size_dependent_setup(
                        &new_images,
                        &rcx.render_pass,
                        &self.memory_allocator,
                        self.log_depth,
                    );
                    rcx.framebuffers = framebuffers;
                    rcx.depth_view = depth_view;
                    if let Some(near) = rcx.near.as_mut() {
                        near.framebuffers = build_near_framebuffers(
                            &new_images,
                            &near.render_pass,
                            rcx.depth_view
                                .as_ref()
                                .expect("log-depth mode must create a depth view"),
                        );
                    }
                    rcx.viewport.extent = window_size.into();
                    rcx.recreate_swapchain = false;
                }
                let extent = rcx.swapchain.image_extent();
                let aspect = extent[0] as f32 / extent[1].max(1) as f32;
                let mvp_matrix = (self.camera.projection_matrix(aspect)
                    * self.camera.view_matrix())
                .to_cols_array_2d();
                let (image_index, suboptimal, acquire_future) = match acquire_next_image(
                    rcx.swapchain.clone(),
                    None,
                )
                .map_err(Validated::unwrap)
                {
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
                // Depth clear = 1.0 (far): empty pixels read as farthest
                // under both linear and log encodings.
                let mut clear_values = vec![Some([0.02, 0.03, 0.08, 1.0].into())];
                if self.log_depth {
                    clear_values.push(Some(1.0f32.into()));
                }
                builder
                    .begin_render_pass(
                        RenderPassBeginInfo {
                            clear_values,
                            ..RenderPassBeginInfo::framebuffer(
                                rcx.framebuffers[image_index as usize].clone(),
                            )
                        },
                        SubpassBeginInfo {
                            contents: SubpassContents::Inline,
                            ..Default::default()
                        },
                    )
                    .expect("render pass must begin")
                    .set_viewport(0, [rcx.viewport.clone()].into_iter().collect())
                    .expect("viewport must set")
                    .bind_pipeline_graphics(rcx.pipeline.clone())
                    .expect("pipeline must bind")
                    .bind_vertex_buffers(0, self.vertex_buffer.clone())
                    .expect("vertex buffer must bind")
                    .bind_index_buffer(self.index_buffer.clone())
                    .expect("index buffer must bind");
                if self.log_depth {
                    builder
                        .push_constants(
                            rcx.pipeline.layout().clone(),
                            0,
                            LogMvpData {
                                mvp: mvp_matrix,
                                log_far: SMOKE_LOG_FAR,
                            },
                        )
                        .expect("log-depth push constants must upload");
                } else {
                    builder
                        .push_constants(
                            rcx.pipeline.layout().clone(),
                            0,
                            MvpData { mvp: mvp_matrix },
                        )
                        .expect("MVP push constants must upload");
                }
                unsafe { builder.draw_indexed(self.index_buffer.len() as u32, 1, 0, 0, 0) }
                    .expect("planet draw must record");
                builder
                    .end_render_pass(Default::default())
                    .expect("render pass must end");
                if let Some(near) = rcx.near.as_ref() {
                    // Near band of the composite: color Loads the planet
                    // band, depth Clears to the tight linear range, the
                    // quad draws over it (near-over-far painter order
                    // across bands — the band plan's contract).
                    let near_mvp = MvpData {
                        mvp: (perspective(FOV_Y, aspect, 0.05, NEAR_PASS_FAR)
                            * self.camera.view_matrix())
                        .to_cols_array_2d(),
                    };
                    builder
                        .begin_render_pass(
                            RenderPassBeginInfo {
                                clear_values: vec![None, Some(1.0f32.into())],
                                ..RenderPassBeginInfo::framebuffer(
                                    near.framebuffers[image_index as usize].clone(),
                                )
                            },
                            SubpassBeginInfo {
                                contents: SubpassContents::Inline,
                                ..Default::default()
                            },
                        )
                        .expect("near render pass must begin")
                        .set_viewport(0, [rcx.viewport.clone()].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(near.pipeline.clone())
                        .expect("near pipeline must bind")
                        .bind_vertex_buffers(
                            0,
                            self.quad_vertex_buffer
                                .clone()
                                .expect("log-depth mode must upload the demo quad"),
                        )
                        .expect("quad vertex buffer must bind")
                        .bind_index_buffer(
                            self.quad_index_buffer
                                .clone()
                                .expect("log-depth mode must upload the demo quad"),
                        )
                        .expect("quad index buffer must bind")
                        .push_constants(near.pipeline.layout().clone(), 0, near_mvp)
                        .expect("near MVP push constants must upload");
                    unsafe { builder.draw_indexed(6, 1, 0, 0, 0) }.expect("quad draw must record");
                    builder
                        .end_render_pass(Default::default())
                        .expect("near render pass must end");
                }
                let command_buffer = builder.build().expect("command buffer must build");
                let future = rcx
                    .previous_frame_end
                    .take()
                    .expect("frame future")
                    .join(acquire_future)
                    .then_execute(self.queue.clone(), command_buffer)
                    .expect("command buffer must submit")
                    .then_swapchain_present(
                        self.queue.clone(),
                        SwapchainPresentInfo::swapchain_image_index(
                            rcx.swapchain.clone(),
                            image_index,
                        ),
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
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(rcx) = self.rcx.as_ref() {
            rcx.window.request_redraw();
        }
    }
}

fn main() {
    // `RUST_LOG` overrides; default to `info` so the smoke's device +
    // tier + planet lines show without extra flags.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    std::process::exit(run());
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    // The cooker subcommand dispatches before the smoke parser (which
    // would reject its flags) and never touches the Vulkan loader.
    if argv.get(1).is_some_and(|first| first == "catalog") {
        return catalog_cook::run(&argv[2..]);
    }
    let args = match parse_args(&argv) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if args.headless {
        return run_headless(&args);
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("event loop failed: {error}");
            return 1;
        }
    };
    let mut app = App::new(&event_loop, &args);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("smoke failed: {error:?}");
        return 1;
    }
    0
}
