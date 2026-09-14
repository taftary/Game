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
//!   [--headless]`.

use std::sync::Arc;
use std::time::Instant;

use game_engine::render::{
    IndexedMesh, OrbitCamera, PLANET_FRAG, PLANET_VERT, QualityTier, SeededPlanet, ShaderKind,
    compile_glsl_to_spirv, create_instance, device_score, log_physical_device,
    required_device_extensions,
};
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo, SubpassBeginInfo,
    SubpassContents,
};
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags};
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageUsage};
use vulkano::instance::Instance;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::{CullMode, RasterizationState};
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

/// MVP push-constant block (matches `PLANET_VERT`).
#[derive(BufferContents)]
#[repr(C)]
struct MvpData {
    mvp: [[f32; 4]; 4],
}

struct Args {
    tier: QualityTier,
    seed: u64,
    radius: f32,
    headless: bool,
}

fn usage() -> &'static str {
    "usage: game_tools [--tier low|medium|high] [--seed N] [--radius R] [--headless]"
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut tier = QualityTier::Medium;
    let mut seed = DEFAULT_SEED;
    let mut radius = DEFAULT_RADIUS;
    let mut headless = false;
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
    Ok(Args {
        tier,
        seed,
        radius,
        headless,
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
    planet: SeededPlanet,
    camera: OrbitCamera,
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    vertex_buffer: Subbuffer<[game_engine::render::PlanetVertex]>,
    index_buffer: Subbuffer<[u32]>,
    dragging: bool,
    last_cursor: Option<(f32, f32)>,
    rcx: Option<RenderContext>,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    pipeline: Arc<GraphicsPipeline>,
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

        App {
            seed: args.seed,
            radius: args.radius,
            tier: args.tier,
            planet,
            camera: OrbitCamera::framing_planet(args.radius),
            instance,
            device,
            queue,
            memory_allocator,
            command_buffer_allocator,
            vertex_buffer,
            index_buffer,
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

    fn build_pipeline(
        &self,
        swapchain: &Arc<Swapchain>,
    ) -> (Arc<RenderPass>, Arc<GraphicsPipeline>) {
        let vert_words = compile_glsl_to_spirv(ShaderKind::Vertex, PLANET_VERT)
            .expect("planet vertex shader must compile");
        let frag_words = compile_glsl_to_spirv(ShaderKind::Fragment, PLANET_FRAG)
            .expect("planet fragment shader must compile");
        // SAFETY: SPIR-V words come from `compile_glsl_to_spirv`, which
        // runs naga validation and pins the `main` entry point; the module
        // is used only with the matching vertex input layout below.
        let vert_module = unsafe {
            ShaderModule::new(
                self.device.clone(),
                ShaderModuleCreateInfo::new(&vert_words),
            )
        }
        .expect("vertex shader module must load");
        let frag_module = unsafe {
            ShaderModule::new(
                self.device.clone(),
                ShaderModuleCreateInfo::new(&frag_words),
            )
        }
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
            self.device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(self.device.clone())
                .expect("pipeline layout must build"),
        )
        .expect("pipeline layout must create");

        let render_pass = vulkano::single_pass_renderpass!(
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
        .expect("render pass must create");
        let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
        let pipeline = GraphicsPipeline::new(
            self.device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: stages.into_iter().collect(),
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState {
                    // Convex planet + CCW dual-cell fans: backface culling
                    // resolves visibility, no depth buffer in M1.
                    cull_mode: CullMode::Back,
                    ..Default::default()
                }),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    subpass.num_color_attachments(),
                    ColorBlendAttachmentState::default(),
                )),
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(subpass.into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )
        .expect("planet graphics pipeline must create");
        (render_pass, pipeline)
    }
}

fn window_size_dependent_setup(
    images: &[Arc<Image>],
    render_pass: &Arc<RenderPass>,
) -> Vec<Arc<Framebuffer>> {
    images
        .iter()
        .map(|image| {
            let view = ImageView::new_default(image.clone()).expect("swapchain image view");
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![view],
                    ..Default::default()
                },
            )
            .expect("framebuffer must create")
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
        let framebuffers = window_size_dependent_setup(&images, &render_pass);
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
            pipeline,
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
                    rcx.framebuffers = window_size_dependent_setup(&new_images, &rcx.render_pass);
                    rcx.viewport.extent = window_size.into();
                    rcx.recreate_swapchain = false;
                }
                let extent = rcx.swapchain.image_extent();
                let aspect = extent[0] as f32 / extent[1].max(1) as f32;
                let mvp = MvpData {
                    mvp: (self.camera.projection_matrix(aspect) * self.camera.view_matrix())
                        .to_cols_array_2d(),
                };
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
                builder
                    .begin_render_pass(
                        RenderPassBeginInfo {
                            clear_values: vec![Some([0.02, 0.03, 0.08, 1.0].into())],
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
                    .expect("index buffer must bind")
                    .push_constants(rcx.pipeline.layout().clone(), 0, mvp)
                    .expect("MVP push constants must upload");
                unsafe { builder.draw_indexed(self.index_buffer.len() as u32, 1, 0, 0, 0) }
                    .expect("planet draw must record");
                builder
                    .end_render_pass(Default::default())
                    .expect("render pass must end");
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
