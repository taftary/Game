//! `game_tools` renderer smoke (M1).
//!
//! - `--headless`: GPU-free path (CI-safe — never loads the Vulkan
//!   loader): generates the tier planet and prints
//!   `tier subdiv cells tris hash gen_ms`.
//! - Windowed (default): `winit` window + `vulkano` boot via
//!   `engine::render::boot` (Instance → Surface → Device → Swapchain),
//!   tier planet uploaded to GPU buffers, naga-compiled shaded pipeline,
//!   drag-orbit / wheel-zoom camera, `1/2/3` to force Low/Med/High tiers,
//!   `H` to cycle HDR exposure demo keys (HDR mode), explicit swapchain
//!   recreation on resize.
//!
//! Usage: `game_tools [--tier low|medium|high] [--seed N] [--radius R]
//!   [--headless] [--log-depth] [--hdr]`.
//!
//! `game_tools catalog --out <dir> ...`: catalog cooker (see the
//! `catalog_cook` module): deterministic Gaia-schema tile writer,
//! GPU-free.
//!
//! `--log-depth` (windowed only) switches the planet pipeline to the
//! log-depth vertex variant (`engine::render::depth`, ADR-016) with a
//! `D32_SFLOAT` depth attachment. Default pixels are unchanged without
//! the flag.
//!
//! `--hdr` (windowed only) renders the scene into an HDR color
//! attachment (`engine::render::post` format selection, 16F preferred)
//! plus a fullscreen resolve pass into the swapchain
//! (`exposure-tone-mapping` Phase 2, fixed exposure 1.0 — plumbing
//! first, tone mapping next). Without the flag the direct
//! LDR-to-swapchain path is bit-identical to the M1 smoke.

use std::sync::Arc;
use std::time::Instant;

mod catalog_cook;

use game_engine::render::{
    DominantSource, ExposureLoop, ExposureParams, FOV_Y, HdrSelection, IndexedMesh, OrbitCamera,
    PLANET_FRAG, PLANET_VERT, PlanetVertex, QualityTier, RESOLVE_VERT, ResolvePush, SeededPlanet,
    ShaderKind, aces_approx, compile_glsl_to_spirv, create_instance, device_score,
    log_physical_device, planet_vert_logdepth, required_device_extensions, resolve_frag_aces,
    select_hdr_format, star_visibility,
};
use glam::Vec3;
use glam::camera::rh::proj::directx::perspective;
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo, SubpassBeginInfo,
    SubpassContents,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags};
use vulkano::format::{Format, FormatFeatures};
use vulkano::image::sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo};
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
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition, VertexInputState};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{
    DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout,
    PipelineShaderStageCreateInfo,
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
    hdr: bool,
}

fn usage() -> &'static str {
    "usage: game_tools [--tier low|medium|high] [--seed N] [--radius R] [--headless] [--log-depth] [--hdr]"
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut tier = QualityTier::Medium;
    let mut seed = DEFAULT_SEED;
    let mut radius = DEFAULT_RADIUS;
    let mut headless = false;
    let mut log_depth = false;
    let mut hdr = false;
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
            "--hdr" => hdr = true,
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
    if hdr && headless {
        return Err("--hdr needs the windowed path (it is a GPU pipeline variant)".to_owned());
    }
    Ok(Args {
        tier,
        seed,
        radius,
        headless,
        log_depth,
        hdr,
    })
}

/// GPU-free smoke: generate the tier planet, print stats, run the
/// exposure self-test lines, exit 0 on success (nonzero if any
/// self-test fails — gate-worthy). Never touches `VulkanLibrary` or
/// `EventLoop` — CI-safe.
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
    let mut ok = true;
    ok &= run_handoff_selftest();
    ok &= run_twilight_selftest();
    ok &= run_tonemap_selftest();
    if ok { 0 } else { 1 }
}

/// DoD-1: waypoint 6 → 5 rehearsal through the real adaptation loop at
/// 60 Hz — sun collapses disk→point, albedo recedes, starlight holds.
/// Exactly two handoffs, Starlight wins, no frame jumps the multiplier.
fn run_handoff_selftest() -> bool {
    let params = ExposureParams::spec_defaults();
    let mut loop_ = ExposureLoop::new(DominantSource::Sun, 1.0);
    let dt = 1.0 / 60.0;
    let (mut sun, mut albedo) = (1.0, 0.1);
    let mut switches = 0;
    let mut prev_source = DominantSource::Sun;
    let mut prev = 1.0;
    let mut max_step = 0.0f64;
    for _ in 0..4000 {
        let (source, exposure) = loop_.step(sun, albedo, 1e-7, dt, &params);
        if source != prev_source {
            switches += 1;
            prev_source = source;
        }
        max_step = max_step.max((exposure / prev - 1.0).abs());
        prev = exposure;
        sun *= 0.99;
        albedo *= 0.995;
    }
    let pass = switches == 2 && prev_source == DominantSource::Starlight && max_step < 0.05;
    println!(
        "exposure_handoff=switches={switches} final={prev_source:?} max_step={max_step:.4} pass={pass}"
    );
    pass
}

/// DoD-2: twilight sweep — visibility rises monotonically from exactly
/// 0 (day) to exactly 1 (astronomical night), passing through partial
/// visibility in every band.
fn run_twilight_selftest() -> bool {
    let params = ExposureParams::spec_defaults();
    let day = star_visibility(1.0, &params);
    let civil = star_visibility(10f64.powf(-4.5), &params);
    let nautical = star_visibility(10f64.powf(-6.25), &params);
    let astro = star_visibility(10f64.powf(-7.5), &params);
    let mut prev = 0.0;
    let mut monotonic = true;
    let mut seen_mid = false;
    for i in 0..=200 {
        let v = star_visibility(10f64.powf(1.0 - (i as f64) * (9.0 / 200.0)), &params);
        monotonic &= v >= prev;
        seen_mid |= v > 0.0 && v < 1.0;
        prev = v;
    }
    let pass = day == 0.0
        && astro == 1.0
        && civil > 0.0
        && civil < nautical
        && nautical < 1.0
        && monotonic
        && seen_mid;
    println!(
        "twilight_fade=day={day:.2} civil={civil:.2} nautical={nautical:.2} astro={astro:.2} monotonic={monotonic} pass={pass}"
    );
    pass
}

/// DoD-3: synthetic HDR scenes through the CPU tone-map mirror — every
/// output finite and ≤ 1.0 (never clips), zero maps to zero.
fn run_tonemap_selftest() -> bool {
    let mut max_out = 0.0f64;
    let mut clean = aces_approx(0.0) == 0.0;
    // Three scenes: dim interior, daylight exterior, sun-disk glare —
    // each swept over four decades of exposure around its key.
    for key in [1e-3, 0.18, 1e4] {
        let mut exposure = 0.18 / key / 100.0;
        for _ in 0..9 {
            let out = aces_approx(key * exposure);
            clean &= out.is_finite() && out <= 1.0;
            max_out = max_out.max(out);
            exposure *= 10.0f64.sqrt();
        }
    }
    let pass = clean && max_out <= 1.0;
    println!("tonemap_clip=max_out={max_out:.4} scenes=3 pass={pass}");
    pass
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
    /// HDR scene-target format (`Some` only with `--hdr` on a device
    /// that renders a `post::HDR_FORMAT_PREFERENCE` candidate).
    hdr_format: Option<Format>,
    /// Adaptation loop + demo keys (`Some`/active only in HDR mode).
    exposure_loop: Option<ExposureLoop>,
    exposure_keys: (f64, f64, f64),
    exposure_preset: usize,
    exposure_params: ExposureParams,
    last_source: DominantSource,
    last_frame: Instant,
    planet: SeededPlanet,
    camera: OrbitCamera,
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    /// Nearest, clamp-to-edge sampler for the resolve pass (exact
    /// texel copy, no filtering blur).
    sampler: Arc<Sampler>,
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

/// Tonemap resolve pass (HDR mode only): its own render pass over the
/// swapchain images sampling the transient HDR view, plus the
/// fullscreen-triangle pipeline and the sampled set over the current
/// HDR view (rebuilt on resize next to it).
struct ResolvePass {
    render_pass: Arc<RenderPass>,
    framebuffers: Vec<Arc<Framebuffer>>,
    pipeline: Arc<GraphicsPipeline>,
    set: Arc<DescriptorSet>,
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
    /// Transient HDR scene view (`Some` only in HDR mode; recreated on
    /// resize next to the framebuffers that sample it).
    hdr_view: Option<Arc<ImageView>>,
    /// Tonemap resolve pass (`Some` only in HDR mode).
    resolve: Option<ResolvePass>,
    /// Current adapted resolve exposure (1.0 without HDR mode).
    exposure: f32,
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
            physical_device.clone(),
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
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                address_mode: [SamplerAddressMode::ClampToEdge; 3],
                ..Default::default()
            },
        )
        .expect("resolve sampler must create");
        // HDR scene-target selection runs once at boot (physical-device
        // query, no window needed). `--hdr` on an incapable device logs
        // and falls back to the direct LDR path — content unchanged.
        let hdr_format = if args.hdr {
            match select_hdr_format(|format| hdr_support(&physical_device, format)) {
                HdrSelection::Hdr(format) => {
                    tracing::info!(format = ?format, "HDR scene target selected");
                    Some(format)
                }
                HdrSelection::LdrBypass => {
                    tracing::warn!(
                        "--hdr requested but no HDR color format is renderable; LDR bypass"
                    );
                    None
                }
            }
        } else {
            None
        };

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
            hdr_format,
            exposure_loop: hdr_format
                .is_some()
                .then(|| ExposureLoop::new(DominantSource::Sun, 1.0)),
            exposure_keys: (1.0, 0.1, 1e-7),
            exposure_preset: 0,
            exposure_params: ExposureParams::spec_defaults(),
            last_source: DominantSource::Sun,
            last_frame: Instant::now(),
            planet,
            camera,
            instance,
            device,
            queue,
            memory_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
            sampler,
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

    /// Cycle the HDR demo key triple (Sun-day → Albedo →
    /// Starlight-night): exercises hysteresis selection + adaptation
    /// live through the resolve exposure. HDR mode only; a no-op (with
    /// a hint) otherwise.
    fn cycle_exposure_preset(&mut self) {
        if self.exposure_loop.is_none() {
            tracing::info!("H cycles HDR exposure keys — restart with --hdr");
            return;
        }
        const PRESETS: [((f64, f64, f64), &str); 3] = [
            ((1.0, 0.1, 1e-7), "Sun-day"),
            ((1e-4, 0.3, 1e-7), "Albedo"),
            ((1e-9, 1e-8, 1e-7), "Starlight-night"),
        ];
        self.exposure_preset = (self.exposure_preset + 1) % PRESETS.len();
        let (keys, name) = PRESETS[self.exposure_preset];
        self.exposure_keys = keys;
        tracing::info!(
            preset = name,
            sun = keys.0,
            albedo = keys.1,
            starlight = keys.2,
            "exposure demo keys"
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
        // HDR mode renders the scene into the transient HDR target;
        // the resolve pass copies to the swapchain afterwards. The
        // default path keeps the swapchain format (M1 pixels unchanged).
        let scene_format = self.hdr_format.unwrap_or_else(|| swapchain.image_format());
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
                        format: scene_format,
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
                        format: scene_format,
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

    /// Tonemap resolve pass (HDR mode only): fullscreen triangle over
    /// the swapchain image sampling the transient HDR view. `DontCare`
    /// load — the triangle covers every pixel, no clear needed. The
    /// fragment source is the ACES composition (`post::resolve_frag_aces`,
    /// Phase 3: plumbing verified in Phase 2, filmic mapping now).
    fn build_resolve_pass(
        &self,
        swapchain: &Arc<Swapchain>,
    ) -> (Arc<RenderPass>, Arc<GraphicsPipeline>) {
        let vert_words = compile_glsl_to_spirv(ShaderKind::Vertex, RESOLVE_VERT)
            .expect("resolve vertex shader must compile");
        let frag_words = compile_glsl_to_spirv(ShaderKind::Fragment, &resolve_frag_aces())
            .expect("resolve fragment shader must compile");
        let render_pass = vulkano::single_pass_renderpass!(
            self.device.clone(),
            attachments: {
                color: {
                    format: swapchain.image_format(),
                    samples: 1,
                    load_op: DontCare,
                    store_op: Store,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {},
            },
        )
        .expect("resolve render pass must create");
        let pipeline =
            Self::build_resolve_pipeline(&self.device, &render_pass, &vert_words, &frag_words);
        (render_pass, pipeline)
    }

    /// Resolve pipeline constructor: empty vertex input (the shader
    /// derives everything from `gl_VertexIndex`), no depth test, no
    /// culling (the fullscreen triangle must survive regardless of
    /// winding — the scene passes keep their own CCW + Back state).
    fn build_resolve_pipeline(
        device: &Arc<Device>,
        render_pass: &Arc<RenderPass>,
        vert_words: &[u32],
        frag_words: &[u32],
    ) -> Arc<GraphicsPipeline> {
        // SAFETY: same contract as `build_planet_pipeline` — naga-
        // validated SPIR-V with the `main` entry point, used with the
        // matching (here: empty) vertex input layout.
        let vert_module =
            unsafe { ShaderModule::new(device.clone(), ShaderModuleCreateInfo::new(vert_words)) }
                .expect("resolve vertex module must load");
        let frag_module =
            unsafe { ShaderModule::new(device.clone(), ShaderModuleCreateInfo::new(frag_words)) }
                .expect("resolve fragment module must load");
        let vs = vert_module.entry_point("main").expect("vertex entry point");
        let fs = frag_module
            .entry_point("main")
            .expect("fragment entry point");
        let stages = [
            PipelineShaderStageCreateInfo::new(vs),
            PipelineShaderStageCreateInfo::new(fs),
        ];
        let layout = PipelineLayout::new(
            device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                .into_pipeline_layout_create_info(device.clone())
                .expect("resolve pipeline layout must build"),
        )
        .expect("resolve pipeline layout must create");
        let subpass = Subpass::from(render_pass.clone(), 0).expect("subpass 0 must exist");
        GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: stages.into_iter().collect(),
                // No vertex buffers: the shader derives the fullscreen
                // triangle from `gl_VertexIndex`. An explicit *empty*
                // input state (not `None` — that selects dynamic vertex
                // input, VUID-pStages-02097) keeps the layout static.
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
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(subpass.into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )
        .expect("resolve graphics pipeline must create")
    }
}

/// HDR scene-target support probe: a candidate format must serve both
/// as the scene color attachment and as the resolve sampler source
/// (the `post::HDR_FORMAT_PREFERENCE` contract). Unsupported formats
/// report cleanly (no validation error) so selection can fall through.
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

/// Transient HDR scene image: single-sampled, no mipmaps — one is
/// enough (frames execute sequentially behind `previous_frame_end`).
/// Recreated on resize next to the framebuffers.
fn create_hdr_view(
    allocator: &Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
    format: Format,
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
    .expect("HDR scene image must create");
    ImageView::new_default(image).expect("HDR scene view must create")
}

/// Scene framebuffers for the HDR path: the single transient HDR view
/// (+ depth in log-depth mode) under the scene render pass. The
/// swapchain images get their own framebuffers under the resolve pass.
fn build_hdr_framebuffers(
    hdr_view: &Arc<ImageView>,
    depth_view: Option<&Arc<ImageView>>,
    render_pass: &Arc<RenderPass>,
) -> Vec<Arc<Framebuffer>> {
    let mut attachments = vec![hdr_view.clone()];
    if let Some(depth) = depth_view {
        attachments.push(depth.clone());
    }
    vec![
        Framebuffer::new(
            render_pass.clone(),
            FramebufferCreateInfo {
                attachments,
                ..Default::default()
            },
        )
        .expect("HDR framebuffer must create"),
    ]
}

/// Resolve framebuffers: one swapchain view each under the resolve
/// render pass (the resolve pass has no depth attachment).
fn build_resolve_framebuffers(
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
            .expect("resolve framebuffer must create")
        })
        .collect()
}

/// Sampled set over the current HDR view (rebuilt on resize next to
/// it — same lifetime discipline as the debug atlas set).
fn build_resolve_set(
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    pipeline: &Arc<GraphicsPipeline>,
    hdr_view: &Arc<ImageView>,
    sampler: &Arc<Sampler>,
) -> Arc<DescriptorSet> {
    let layout = pipeline.layout().set_layouts()[0].clone();
    DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout,
        [
            WriteDescriptorSet::image_view(0, hdr_view.clone()),
            WriteDescriptorSet::sampler(1, sampler.clone()),
        ],
        [],
    )
    .expect("resolve descriptor set must create")
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
        // HDR mode renders the scene into the transient HDR view (the
        // swapchain images get framebuffers under the resolve pass
        // instead); the default path keeps one framebuffer per
        // swapchain image as before.
        let extent = images
            .first()
            .expect("swapchain must yield an image")
            .extent();
        let (framebuffers, depth_view, hdr_view) = if let Some(hdr_format) = self.hdr_format {
            let hdr_view =
                create_hdr_view(&self.memory_allocator, [extent[0], extent[1]], hdr_format);
            let depth_view = self
                .log_depth
                .then(|| create_depth_view(&self.memory_allocator, [extent[0], extent[1]]));
            let framebuffers = build_hdr_framebuffers(&hdr_view, depth_view.as_ref(), &render_pass);
            (framebuffers, depth_view, Some(hdr_view))
        } else {
            let (framebuffers, depth_view) = window_size_dependent_setup(
                &images,
                &render_pass,
                &self.memory_allocator,
                self.log_depth,
            );
            (framebuffers, depth_view, None)
        };
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
        // Tonemap resolve over the swapchain images (HDR mode only):
        // scene(HDR) → resolve(swap) → near(swap, Load) is the pass
        // order; the near band composites over resolved pixels.
        let resolve = hdr_view.as_ref().map(|hdr_view| {
            let (resolve_pass, resolve_pipeline) = self.build_resolve_pass(&swapchain);
            let resolve_framebuffers = build_resolve_framebuffers(&images, &resolve_pass);
            let resolve_set = build_resolve_set(
                &self.descriptor_set_allocator,
                &resolve_pipeline,
                hdr_view,
                &self.sampler,
            );
            ResolvePass {
                render_pass: resolve_pass,
                framebuffers: resolve_framebuffers,
                pipeline: resolve_pipeline,
                set: resolve_set,
            }
        });
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
            hdr_view,
            resolve,
            exposure: 1.0,
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
                        PhysicalKey::Code(KeyCode::KeyH) => self.cycle_exposure_preset(),
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
                    // HDR rebuild mirrors `resumed`: transient HDR view
                    // + scene framebuffers, then resolve framebuffers +
                    // the sampled set over the new view.
                    let new_extent = new_images
                        .first()
                        .expect("swapchain must yield an image")
                        .extent();
                    if let Some(hdr_format) = self.hdr_format {
                        let hdr_view = create_hdr_view(
                            &self.memory_allocator,
                            [new_extent[0], new_extent[1]],
                            hdr_format,
                        );
                        let depth_view = self.log_depth.then(|| {
                            create_depth_view(
                                &self.memory_allocator,
                                [new_extent[0], new_extent[1]],
                            )
                        });
                        rcx.framebuffers = build_hdr_framebuffers(
                            &hdr_view,
                            depth_view.as_ref(),
                            &rcx.render_pass,
                        );
                        rcx.depth_view = depth_view;
                        rcx.hdr_view = Some(hdr_view.clone());
                        if let Some(resolve) = rcx.resolve.as_mut() {
                            resolve.framebuffers =
                                build_resolve_framebuffers(&new_images, &resolve.render_pass);
                            resolve.set = build_resolve_set(
                                &self.descriptor_set_allocator,
                                &resolve.pipeline,
                                &hdr_view,
                                &self.sampler,
                            );
                        }
                    } else {
                        let (framebuffers, depth_view) = window_size_dependent_setup(
                            &new_images,
                            &rcx.render_pass,
                            &self.memory_allocator,
                            self.log_depth,
                        );
                        rcx.framebuffers = framebuffers;
                        rcx.depth_view = depth_view;
                    }
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
                // Adaptation loop (HDR mode only): ease the resolve
                // exposure toward the keyed target every frame; source
                // switches log once each (hysteresis, no oscillation).
                if let Some(loop_) = self.exposure_loop.as_mut() {
                    let now = Instant::now();
                    let dt = now
                        .duration_since(self.last_frame)
                        .as_secs_f64()
                        .clamp(0.0, 0.25);
                    self.last_frame = now;
                    let (source, exposure) = loop_.step(
                        self.exposure_keys.0,
                        self.exposure_keys.1,
                        self.exposure_keys.2,
                        dt,
                        &self.exposure_params,
                    );
                    if source != self.last_source {
                        tracing::info!(source = ?source, exposure, "dominant light switched");
                        self.last_source = source;
                    }
                    rcx.exposure = exposure as f32;
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
                // HDR mode draws the scene into the single transient
                // HDR framebuffer; the resolve pass fans out to the
                // acquired swapchain image afterwards.
                let scene_index = if rcx.resolve.is_some() {
                    0
                } else {
                    image_index as usize
                };
                builder
                    .begin_render_pass(
                        RenderPassBeginInfo {
                            clear_values,
                            ..RenderPassBeginInfo::framebuffer(
                                rcx.framebuffers[scene_index].clone(),
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
                if let Some(resolve) = rcx.resolve.as_ref() {
                    // Phase-4 exposure: the adaptation loop drives the
                    // multiplier (Phase 2 proved the plumbing at 1.0).
                    builder
                        .begin_render_pass(
                            RenderPassBeginInfo {
                                clear_values: vec![None],
                                ..RenderPassBeginInfo::framebuffer(
                                    resolve.framebuffers[image_index as usize].clone(),
                                )
                            },
                            SubpassBeginInfo {
                                contents: SubpassContents::Inline,
                                ..Default::default()
                            },
                        )
                        .expect("resolve render pass must begin")
                        .set_viewport(0, [rcx.viewport.clone()].into_iter().collect())
                        .expect("viewport must set")
                        .bind_pipeline_graphics(resolve.pipeline.clone())
                        .expect("resolve pipeline must bind")
                        .bind_descriptor_sets(
                            PipelineBindPoint::Graphics,
                            resolve.pipeline.layout().clone(),
                            0,
                            resolve.set.clone(),
                        )
                        .expect("resolve descriptor set must bind")
                        .push_constants(
                            resolve.pipeline.layout().clone(),
                            0,
                            ResolvePush {
                                exposure: rcx.exposure,
                            },
                        )
                        .expect("resolve push constants must upload");
                    // SAFETY: the resolve pipeline declares no vertex
                    // input — the shader derives the fullscreen triangle
                    // from `gl_VertexIndex`, so 3 unbuffered vertices are
                    // the whole draw.
                    unsafe { builder.draw(3, 1, 0, 0) }.expect("resolve draw must record");
                    builder
                        .end_render_pass(Default::default())
                        .expect("resolve render pass must end");
                }
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
