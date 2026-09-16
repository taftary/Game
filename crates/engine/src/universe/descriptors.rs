//! Universe descriptors: the pure data later milestones consume.
//!
//! A descriptor is everything a stage knows about its bodies — the same
//! structs the maps (M5), descent arrivals (M2), surface detail (M3) and
//! saves (M4) will load against, so no throwaway map-only data: what the
//! maps show here is what the saves store later. Every descriptor is
//! stamped with [`UNIVERSE_VERSION`](super::UNIVERSE_VERSION) at
//! construction.
//!
//! The generators that fill these structs arrive in UMAP-003–005; this
//! file owns the shapes plus the stamping constructors.
//!
//! ```
//! use game_engine::universe::{GalaxyDescriptor, PlanetId, PlanetType};
//!
//! let id = PlanetId::new(42, 3, 1);
//! let galaxy = GalaxyDescriptor::new(42, Vec::new());
//! assert_eq!(galaxy.id.to_string(), "galaxy/seed:42");
//! assert_eq!(galaxy.seed, 42);
//! assert_eq!(galaxy.stars.len(), 0);
//! assert_eq!(id.galaxy_id(), galaxy.id);
//! ```

use super::UNIVERSE_VERSION;
use super::ids::{GalaxyId, PlanetId, SystemId};

/// Stellar spectral class (Morgan–Keenan sequence), stage-1 output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpectralClass {
    O,
    B,
    A,
    F,
    G,
    K,
    M,
}

/// Planet type subset, stage-2 output. The full v1 list per
/// `docs/game/universe.md` is rocky / desert / ice / volcanic / toxic /
/// oceanic; the shipped subset finalizes with UX in UMAP-021.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanetType {
    Rocky,
    Desert,
    Ice,
    Volcanic,
    Toxic,
    Oceanic,
}

/// Atmosphere look: color drives the orbit/focus backdrop, density the
/// scattering shell the descent milestone (M2) builds. Density 0.0 =
/// airless.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// Linear-space RGB tint.
    pub color: [f32; 3],
    /// 0.0 (airless) .. 1.0+ (dense); unbounded above for gas-shrouded
    /// outliers the generators may emit.
    pub density: f32,
}

/// Resource bias per planet type (`docs/game/gameplay.md` v1 set:
/// energy, metal, water/ice, organics, rare element). Multipliers around
/// 1.0; the colonies milestone (M4) consumes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceBias {
    pub energy: f32,
    pub metal: f32,
    pub water_ice: f32,
    pub organics: f32,
    pub rare: f32,
}

/// One star in a [`GalaxyDescriptor`]: stage-1 output.
#[derive(Clone, Debug, PartialEq)]
pub struct StarDescriptor {
    /// Index into the parent galaxy's star list; matches
    /// [`SystemId::star_index`].
    pub star_index: u32,
    pub spectral_class: SpectralClass,
    /// f64 log-compressed galactic position in light-years
    /// (`docs/game/journey.md` L2, 10k–100k ly compressed extent).
    pub position_ly: [f64; 3],
    /// Visual-only companions in v1 (universe.md stage 1).
    pub companion_count: u32,
}

/// One planet in a [`SystemDescriptor`]: its orbit plus its descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemPlanet {
    /// Compressed AU-scale orbital radius (`docs/game/journey.md` L3).
    pub orbit_radius_au: f64,
    pub descriptor: PlanetDescriptor,
}

/// One planet: stage-2 output, consumed by maps, descent, surface, saves.
///
/// Constructed literally (all fields public, bands documented per
/// field): validation of the v1 bands is the generator's job (UMAP-004).
/// Unlike [`GalaxyDescriptor::new`] and [`SystemDescriptor::new`] there
/// is no identity to derive here that the caller doesn't already hold,
/// so no stamping constructor exists on purpose.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanetDescriptor {
    pub id: PlanetId,
    /// Gameplay radius in km; the v1 start band is 2–8 km
    /// (`docs/game/journey.md` L4).
    pub radius_km: f32,
    pub planet_type: PlanetType,
    /// Surface gravity in g: narrow band so controls stay consistent
    /// (`docs/game/universe.md` variety axes). The v1 generator emits
    /// 0.8–1.2.
    pub gravity_g: f32,
    /// Seed binding to `engine::render::SeededPlanet` params — the mesh
    /// the orbit view (M5) and the surface (M2/M3) build from.
    pub mesh_seed: u64,
    pub atmosphere: Atmosphere,
    pub resource_bias: ResourceBias,
    /// Moons/rings: visual-only in v1 (`docs/game/scope.md`).
    pub companion_count: u32,
}

/// One generated galaxy: stage-1 output.
#[derive(Clone, Debug, PartialEq)]
pub struct GalaxyDescriptor {
    pub id: GalaxyId,
    /// Runtime seed this galaxy generates from; a new save rolls a new
    /// one (regeneration, not persistence, until M4).
    pub seed: u64,
    /// [`UNIVERSE_VERSION`] at generation time — forward compatibility
    /// for saves (M4) and migrations (`tools`).
    pub universe_version: u32,
    pub stars: Vec<StarDescriptor>,
}

impl GalaxyDescriptor {
    /// Stamp a generator-produced star list with identity + version.
    pub fn new(seed: u64, stars: Vec<StarDescriptor>) -> Self {
        Self {
            id: GalaxyId::new(seed),
            seed,
            universe_version: UNIVERSE_VERSION,
            stars,
        }
    }
}

/// One star system: stage-2 output.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemDescriptor {
    pub id: SystemId,
    pub star: StarDescriptor,
    pub planets: Vec<SystemPlanet>,
}

impl SystemDescriptor {
    /// Stamp a generator-produced system with identity. Takes the galaxy
    /// `seed` explicitly: a [`StarDescriptor`] alone does not carry it,
    /// and guessing it here would forge a wrong [`SystemId`].
    pub fn new(seed: u64, star: StarDescriptor, planets: Vec<SystemPlanet>) -> Self {
        let id = SystemId::new(seed, star.star_index);
        Self { id, star, planets }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_atmosphere() -> Atmosphere {
        Atmosphere {
            color: [0.4, 0.6, 1.0],
            density: 0.8,
        }
    }

    fn sample_bias() -> ResourceBias {
        ResourceBias {
            energy: 1.0,
            metal: 1.2,
            water_ice: 0.8,
            organics: 0.5,
            rare: 0.1,
        }
    }

    #[test]
    fn galaxy_constructor_stamps_version_and_id() {
        let galaxy = GalaxyDescriptor::new(42, Vec::new());
        assert_eq!(galaxy.universe_version, UNIVERSE_VERSION);
        assert_eq!(galaxy.id, GalaxyId::new(42));
        assert_eq!(galaxy.seed, 42);
    }

    #[test]
    fn system_constructor_derives_id_from_star() {
        let star = StarDescriptor {
            star_index: 3,
            spectral_class: SpectralClass::G,
            position_ly: [12000.0, -4500.0, 800.0],
            companion_count: 0,
        };
        let system = SystemDescriptor::new(42, star, Vec::new());
        assert_eq!(system.id, SystemId::new(42, 3));
    }

    #[test]
    fn planet_descriptor_round_trips_fields() {
        let id = PlanetId::new(42, 3, 1);
        let planet = PlanetDescriptor {
            id,
            radius_km: 5.0,
            planet_type: PlanetType::Ice,
            gravity_g: 1.0,
            mesh_seed: 0xC0FFEE,
            atmosphere: sample_atmosphere(),
            resource_bias: sample_bias(),
            companion_count: 2,
        };
        assert_eq!(planet.id, id);
        assert_eq!(planet.radius_km, 5.0);
        assert_eq!(planet.planet_type, PlanetType::Ice);
        assert_eq!(planet.gravity_g, 1.0);
        assert_eq!(planet.mesh_seed, 0xC0FFEE);
        assert_eq!(planet.atmosphere.density, 0.8);
        assert_eq!(planet.resource_bias.metal, 1.2);
        assert_eq!(planet.companion_count, 2);
    }
}
