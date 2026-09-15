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
//! `PhysicalDevice` need a GPU at runtime.

pub mod boot;
pub mod camera;
pub mod checker;
pub mod planet;
pub mod shaders;
pub mod tier;
pub mod uv;

pub use boot::{
    MAX_API_VERSION, VALIDATION_LAYER, create_instance, device_score, instance_create_info,
    log_physical_device, required_device_extensions,
};
pub use camera::{FOV_Y, MAX_PITCH, OrbitCamera};
pub use checker::{
    CUBE_FACE_COUNT, CheckerTable, checker_local, checker_parity, checker_square, checker_table,
    glsl_const_block,
};
pub use planet::{IndexedMesh, PlanetVertex, SeededPlanet};
pub use shaders::{
    PLANET_FRAG, PLANET_VERT, ShaderCompileError, ShaderKind, compile_glsl_to_spirv,
};
pub use tier::{ParseTierError, QualityTier};
pub use uv::{
    FlatUnwrap, UvData, UvScheme, build_debug_uv, build_debug_uv_for_scheme, build_flat_unwrap,
    build_wireframe_uv_clipped, island_coverage, walk_tree_edges,
};
