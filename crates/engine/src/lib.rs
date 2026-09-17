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
//! - [`physics`] — cheapest-correct physics model per scale (spec §5) +
//!   symplectic integrator. Pure + headless.
//! - [`seeding`] — hierarchical procedural seeds (spec §6): region
//!   derivation, domain-separated layers, catalog overrides. Pure.
//! - [`handoff`] — soft patched-conic SOI handoff (spec §3): Laplace /
//!   Hill radii, smoothstep blend band, hysteresis monitor. Pure.
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
pub mod handoff;
pub mod hexsphere;
pub mod physics;
pub mod render;
pub mod seeding;
pub mod universe;
