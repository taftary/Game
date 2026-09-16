//! Stable content IDs for universe bodies.
//!
//! Canonical string forms (see `docs/game/universe.md`):
//!
//! - galaxy: `galaxy/seed:{seed}`
//! - system: `galaxy/seed:{seed}/system:{star_index}`
//! - planet: `galaxy/seed:{seed}/system:{star_index}/planet:{planet_index}`
//!
//! IDs are constructed only through the `new` constructors, so a stored
//! id always has the canonical shape — safe to persist in saves and
//! links (M4 era). [`std::fmt::Display`] renders the canonical string.
//!
//! ```
//! use game_engine::universe::{GalaxyId, PlanetId, SystemId};
//!
//! let galaxy = GalaxyId::new(42);
//! let system = SystemId::new(42, 3);
//! let planet = PlanetId::new(42, 3, 1);
//! assert_eq!(galaxy.to_string(), "galaxy/seed:42");
//! assert_eq!(system.to_string(), "galaxy/seed:42/system:3");
//! assert_eq!(planet.to_string(), "galaxy/seed:42/system:3/planet:1");
//! assert_eq!(planet.galaxy_id(), galaxy);
//! assert_eq!(planet.system_id(), system);
//! ```

use std::fmt;

/// Stable identity of one generated galaxy: its runtime seed.
///
/// Canonical string: `galaxy/seed:{seed}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GalaxyId {
    seed: u64,
}

impl GalaxyId {
    /// Build the id for `seed`. The constructor is the only way to make
    /// one, so every `GalaxyId` in the wild has canonical shape.
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// The runtime seed this galaxy generates from.
    pub fn seed(self) -> u64 {
        self.seed
    }
}

impl fmt::Display for GalaxyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "galaxy/seed:{}", self.seed)
    }
}

/// Stable identity of one star system: galaxy seed + star index.
///
/// Canonical string: `galaxy/seed:{seed}/system:{star_index}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SystemId {
    seed: u64,
    star_index: u32,
}

impl SystemId {
    /// Build the id for star `star_index` of galaxy `seed`.
    pub fn new(seed: u64, star_index: u32) -> Self {
        Self { seed, star_index }
    }

    /// The runtime seed of the parent galaxy.
    pub fn seed(self) -> u64 {
        self.seed
    }

    /// Index of the central star in its [`GalaxyId`]'s star list.
    pub fn star_index(self) -> u32 {
        self.star_index
    }

    /// The parent galaxy id.
    pub fn galaxy_id(self) -> GalaxyId {
        GalaxyId::new(self.seed)
    }
}

impl fmt::Display for SystemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/system:{}", self.galaxy_id(), self.star_index)
    }
}

/// Stable identity of one planet: galaxy seed + star index + planet
/// index.
///
/// Canonical string:
/// `galaxy/seed:{seed}/system:{star_index}/planet:{planet_index}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanetId {
    seed: u64,
    star_index: u32,
    planet_index: u32,
}

impl PlanetId {
    /// Build the id for planet `planet_index` of system
    /// (`seed`, `star_index`).
    pub fn new(seed: u64, star_index: u32, planet_index: u32) -> Self {
        Self {
            seed,
            star_index,
            planet_index,
        }
    }

    /// The runtime seed of the parent galaxy.
    pub fn seed(self) -> u64 {
        self.seed
    }

    /// Index of the parent star in its [`GalaxyId`]'s star list.
    pub fn star_index(self) -> u32 {
        self.star_index
    }

    /// Index of this planet in its [`SystemId`]'s planet list.
    pub fn planet_index(self) -> u32 {
        self.planet_index
    }

    /// The parent galaxy id.
    pub fn galaxy_id(self) -> GalaxyId {
        GalaxyId::new(self.seed)
    }

    /// The parent system id.
    pub fn system_id(self) -> SystemId {
        SystemId::new(self.seed, self.star_index)
    }
}

impl fmt::Display for PlanetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/planet:{}", self.system_id(), self.planet_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_strings_match_universe_spec() {
        assert_eq!(GalaxyId::new(0).to_string(), "galaxy/seed:0");
        assert_eq!(SystemId::new(7, 12).to_string(), "galaxy/seed:7/system:12");
        assert_eq!(
            PlanetId::new(7, 12, 3).to_string(),
            "galaxy/seed:7/system:12/planet:3"
        );
    }

    #[test]
    fn parent_ids_round_trip() {
        let planet = PlanetId::new(99, 4, 2);
        assert_eq!(planet.galaxy_id(), GalaxyId::new(99));
        assert_eq!(planet.system_id(), SystemId::new(99, 4));
        assert_eq!(planet.system_id().galaxy_id(), GalaxyId::new(99));
    }

    #[test]
    fn max_indices_stay_canonical() {
        let planet = PlanetId::new(u64::MAX, u32::MAX, u32::MAX);
        assert_eq!(
            planet.to_string(),
            format!(
                "galaxy/seed:{}/system:{}/planet:{}",
                u64::MAX,
                u32::MAX,
                u32::MAX
            )
        );
    }
}
