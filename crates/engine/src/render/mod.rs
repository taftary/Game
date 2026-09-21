//! `engine::render` — Vulkan renderer: boot, tiers, camera, planet mesh.
//!
//! Floor rules (ADR-007, `docs/techstack/rendering.md`):
//!
//! - Gameplay-critical Vulkan stays within Vulkan 1.1 core; the instance
//!   caps `max_api_version` at 1.1 ([`boot::MAX_API_VERSION`]).
//! - Portability enumeration is always on (Apple/MoltenVK).
//! - `game` contains no `vulkano` pipeline code; it calls this module.
//!   The M1 consumer is the `game_tools` renderer smoke.
//!
//! [`camera`] and [`planet`] triangulation are pure math; [`shaders`]
//! compiles headlessly. Only [`boot`] helpers that touch `Instance` /
//! `PhysicalDevice` need a GPU at runtime. [`depth`] and [`bands`] plan
//! log-depth encodings and multi-pass compositing, also GPU-free.
//! [`stars`] expands catalog tiles to Backdrop point sprites, also
//! GPU-free (binaries own the buffers). [`exposure`] is the pure
//! auto-exposure / tone-map / dark-adaptation kernel and [`post`] its
//! headless-testable format-selection + resolve-shader half (binaries
//! own the HDR images and pipelines).

pub mod atmosphere;
pub mod bands;
pub mod boot;
pub mod camera;
pub mod checker;
pub mod chunk_flat;
pub mod cue;
pub mod depth;
pub mod exposure;
pub mod planet;
pub mod post;
pub mod shaders;
pub mod stars;
pub mod tier;
pub mod uv;
pub mod zodiacal;

pub use atmosphere::{
    ATMOSPHERE_SHELL_FRAG, AtmosphereParams, KARMAN_FAI_M, KARMAN_REANALYSIS_M, haze_transmission,
    limb_glow, sky_color,
};
pub use bands::{
    BandConfig, BandPass, ContentLayer, DepthMode, LayerKind, PassBucket, bucket_for, plan_passes,
    shares_depth_pass,
};
pub use boot::{
    MAX_API_VERSION, VALIDATION_LAYER, create_instance, device_score, instance_create_info,
    log_physical_device, required_device_extensions,
};
pub use camera::{FOV_Y, MAX_PITCH, OrbitCamera};
pub use checker::{
    CUBE_FACE_COUNT, CheckerTable, checker_local, checker_parity, checker_square, checker_table,
    glsl_const_block,
};
pub use chunk_flat::{orbit_viewpoint, project_to_tangent, tangent_basis, visible_hemisphere};
pub use cue::{
    CueRegime, CueState, DustColumn, ExtinctionParams, RaymarchBudget, WebDensity,
    atmosphere_radiance, color_excess, extinction_transmission, hubble_redshift, mie_phase,
    peculiar_velocity_tint, rayleigh_optical_depth, raymarch_web, redden_rgb, regime_for_frame,
    sample_dust_column, sample_web_density, visual_extinction,
};
pub use depth::{
    LOG_GUARD_EPSILON, LogDepthParams, glsl_log_depth_epilogue, glsl_log_depth_push_field,
    linear_depth_f32, planet_vert_logdepth, same_float_depth, same_unorm_quantum,
};
pub use exposure::{
    DominantSource, EXPOSURE_MAX, EXPOSURE_MIN, ExposureLoop, ExposureParams, MIDDLE_GREY,
    PHOTOMETRIC_ZERO_POINT, aces_approx, adapt_exposure, calibrate, exposure_for_key,
    select_source, star_visibility,
};
pub use planet::{IndexedMesh, PlanetVertex, SeededPlanet};
pub use post::{
    ACES_FIT_GLSL, BLOOM_BRIGHT_FRAG, BLOOM_DOWN_FRAG, BLOOM_PREFILTER_FRAG, BLOOM_UP_FRAG,
    BloomBrightPush, BloomDownPush, BloomParams, BloomPrefilterPush, BloomResolvePush, BloomUpPush,
    HDR_FORMAT_PREFERENCE, HdrSelection, MipBloomParams, RESOLVE_FRAG_FIXED, RESOLVE_VERT,
    ResolvePush, jimenez13_offsets, resolve_frag_aces, resolve_frag_bloom, select_hdr_format,
    soft_knee, tent9_weights,
};
pub use shaders::{
    PLANET_FRAG, PLANET_VERT, ShaderCompileError, ShaderKind, compile_glsl_to_spirv,
};
pub use stars::{
    CROSSFADE_MS, SKY_SHELL_RADIUS, StarPoint, WORLD_TO_EQUATORIAL, crossfade_alpha,
    equatorial_to_world, expand_catalog_record, expand_fallback_star, expand_tile_catalog,
    expand_tile_fallback, mag_to_size_px, spectral_color, world_to_equatorial,
};
pub use tier::{ParseTierError, QualityTier};
pub use uv::{
    FlatUnwrap, UvData, UvScheme, build_debug_uv, build_debug_uv_for_scheme, build_flat_unwrap,
    build_wireframe_uv_clipped, island_coverage, walk_tree_edges,
};
pub use zodiacal::{
    AnalyticZodiacal, ECLIPTIC_OBLIQUITY_DEG, MU_BRIGHTEST, MU_FAINTEST, ZodiacalParams,
    ZodiacalSource, ecliptic_coords, radiance_relative, solar_elongation, surface_brightness_mag,
};
