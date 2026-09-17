//! `game_engine` — custom engine library: renderer, sphere geometry,
//! simulation, assets, input, saves.
//!
//! Module map (see `docs/techstack/architecture.md`):
//!
//! - [`core`] — shared kernel: seeded RNG + quantization (determinism
//!   contract for all generation), later math/units/time/errors.
//! - [`frames`] — hierarchical nested reference frames (ADR-012) +
//!   floating-origin GPU upload (ADR-013). Pure + headless.
//! - [`hexsphere`] — hex-dominant geodesic sphere, the geometry base for
//!   every spherical body (planets, stars, moons). Pure + deterministic.
//! - [`render`] — Vulkan renderer boot (1.1 floor), quality tiers, orbit
//!   camera, seeded planet mesh. GPU calls live here; `game` never touches
//!   `vulkano` directly.
//! - [`universe`] — procedural universe generation (galaxy/system/planet
//!   descriptors, stages 1–2 in the maps milestone). Pure + deterministic.

/// Global allocator switch (ADR-009, validated in M1 behind this flag).
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod core;
pub mod frames;
pub mod hexsphere;
pub mod render;
pub mod universe;
