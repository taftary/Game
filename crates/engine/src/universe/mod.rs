//! Procedural universe generation: web → galaxy → systems → planets.
//!
//! Staged generation (see `docs/game/universe.md`):
//!
//! 0. Web seed → cosmic web: nodes (clusters/groups), filament links,
//!    dwarf glow, home node (v0.3.2 `cosmic-scale-player`; [`web`]).
//! 1. Galaxy seed → star systems: position, spectral class, companion count.
//! 2. System seed → planets: count, orbits, type, gravity, atmosphere
//!    density/color, resource bias, companions (moons/rings, visual-only
//!    in v1 — `docs/game/journey.md` Level 4).
//! 3. Planet seed → surface (M2/M3 era): elevation, biomes, water table,
//!    POIs, site candidates. Not here yet.
//!
//! Everything here is pure and deterministic: `(seed, version, id) →
//! descriptor`. No wall-clock, no thread-order-dependent iteration in
//! output. Descriptors carry [`UNIVERSE_VERSION`] for forward
//! compatibility (saves store it from M4 on); content IDs are stable
//! strings safe to store in saves and links. [`generate`] holds the
//! stage 1–2 generators, [`web`] the stage-0 generator, [`hash`] the
//! cross-platform descriptor hashes, [`crate::core`] the seeded RNG and
//! quantization they build on.
//!
//! ```no_run
//! use game_engine::universe::{PlanetId, UNIVERSE_VERSION};
//!
//! let id = PlanetId::new(1234, 0, 2);
//! assert_eq!(id.to_string(), "galaxy/seed:1234/system:0/planet:2");
//! assert!(UNIVERSE_VERSION >= 1);
//! ```

pub mod descriptors;
pub mod generate;
pub mod hash;
pub mod ids;
pub mod web;

pub use descriptors::{
    Atmosphere, GalaxyDescriptor, PlanetDescriptor, PlanetType, ResourceBias, SpectralClass,
    StarDescriptor, SystemDescriptor, SystemPlanet,
};
pub use generate::{
    COMPRESSION_KNEE_LY, DEFAULT_STAR_COUNT, DISK_HALF_THICKNESS_LY, GALAXY_RADIUS_LY,
    generate_galaxy, generate_system,
};
pub use hash::{galaxy_hash, planet_hash, system_hash, web_hash};
pub use ids::{GalaxyId, PlanetId, SystemId};
pub use web::{CosmicWebParams, WebDescriptor, WebLink, WebNode, generate_cosmic_web};

/// Universe format version. Bumped only with an intentional generation
/// change; stamped into every descriptor. Version bump = migration or
/// new game, never silent drift (see `docs/game/universe.md` rules).
///
/// v3 (v0.3.2 `cosmic-web-mass-rank-fix`): fixes inverted mass-rank
/// assignment in the stage-0 cosmic web — densest peak now correctly
/// receives the rarest (most massive) halo.  Stage 1–2 streams are
/// domain-separated and untouched, so galaxies and systems replay
/// identically — only their hashes re-roll via the stamp.
///
/// v2 (v0.3.2 `cosmic-scale-player`): adds the stage-0 cosmic web.
pub const UNIVERSE_VERSION: u32 = 3;
