//! `game_engine` — custom engine library: renderer, sphere geometry,
//! simulation, assets, input, saves.
//!
//! Module map (see `docs/techstack/architecture.md`):
//!
//! - [`hexsphere`] — hex-dominant geodesic sphere, the geometry base for
//!   every spherical body (planets, stars, moons). Pure + deterministic.
//! - [`render`] — Vulkan renderer boot (1.1 floor), quality tiers, orbit
//!   camera, seeded planet mesh. GPU calls live here; `game` never touches
//!   `vulkano` directly.

/// Global allocator switch (ADR-009, validated in M1 behind this flag).
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod hexsphere;
pub mod render;
