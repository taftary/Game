//! `engine::seeding` — hierarchical procedural seeds (spec §6, ADR-019).
//!
//! One master seed regenerates the whole universe with no stored
//! content: `region_seed = hash(master_seed, frame_id, cell)` feeds
//! domain-separated layer streams (`"galaxy_arms"`, `"star_field"`,
//! …), and cataloged real objects suppress procedural cells inside an
//! exclusion radius — checked *before* the procedural hash fires.
//!
//! The hash is the existing [`crate::core`] kernel (FNV-1a domain mix +
//! SplitMix64 avalanche, pinned by reference-vector tests): derivation
//! is not hot, so a new hash dependency would add audit surface for
//! zero behavioral gain. The stable contract is the domain grammar +
//! [`SEED_VERSION`], trip-wired by committed snapshot vectors below.
//!
//! ```
//! use game_engine::frames::FrameId;
//! use game_engine::seeding::{region_seed, RegionId};
//!
//! let region = RegionId { frame: FrameId::SolarSystem, cell: [3, -1, 0] };
//! let seed = region_seed(42, region);
//! // Raw region seeds never escape: only domain-separated layer streams.
//! let mut arms = seed.layer("galaxy_arms");
//! let mut field = seed.layer("star_field");
//! assert_ne!(arms.next_u64(), field.next_u64());
//! ```

use crate::core::SeededRng;
use crate::frames::FrameId;

/// Seed-derivation algorithm version. Bumps on ANY grammar change —
/// old saves fork explicitly, never silently (ADR-004).
pub const SEED_VERSION: u32 = 1;

/// One addressable procedural region: a frame plus generator-defined
/// cell coordinates (HEALPix tiles, chunk ids, … define cell semantics;
/// this module only hashes them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionId {
    /// Frame the cell lives in.
    pub frame: FrameId,
    /// Cell coordinates in frame-local cells.
    pub cell: [i64; 3],
}

impl RegionId {
    /// Stable domain string. Grammar v1:
    /// `seed/v1/frame/{d}/body/{b}/cell/{x},{y},{z}`, with the frame
    /// encoding from [`FrameId::code`] (never `Debug`-formatted: format
    /// stability is not a determinism basis).
    fn domain(self) -> String {
        let (d, b) = self.frame.code();
        let [x, y, z] = self.cell;
        format!("seed/v{SEED_VERSION}/frame/{d}/body/{b}/cell/{x},{y},{z}")
    }
}

/// Opaque region seed. Deliberately no raw-`u64` accessor: content
/// layers may only draw through [`RegionSeed::layer`], so a raw region
/// seed can never be reused across layers (ADR-019 domain separation,
/// enforced by the type system).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionSeed(u64);

impl RegionSeed {
    /// Domain-separated layer stream, e.g. `"galaxy_arms"`.
    /// Same `(region, layer)` replays the same stream on every platform.
    pub fn layer(self, name: &str) -> SeededRng {
        SeededRng::stream(self.0, name)
    }
}

/// Derive a region seed from the master seed. Pure: same inputs ⇒ same
/// seed on every platform, every session.
pub fn region_seed(master: u64, region: RegionId) -> RegionSeed {
    let mut stream = SeededRng::stream(master, &region.domain());
    RegionSeed(stream.next_u64())
}

/// Procedural-suppression zone around one cataloged real object
/// (ADR-019 real-data override). Generators check this *before* the
/// procedural hash fires for a cell; the catalog lookup feeding zones
/// lands with `star-catalog-streaming`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExclusionZone {
    /// Zone center in the region's cell coordinates.
    pub center: [i64; 3],
    /// Suppression radius in cells (inclusive boundary).
    pub radius_cells: u64,
}

/// True when `cell` falls inside `zone` (Euclidean, inclusive boundary;
/// `u128` distance math cannot overflow `i64` coordinates).
pub fn is_suppressed(zone: &ExclusionZone, cell: [i64; 3]) -> bool {
    // i64 − i64 always fits i128, so unsigned_abs cannot overflow.
    let dx = (cell[0] as i128 - zone.center[0] as i128).unsigned_abs();
    let dy = (cell[1] as i128 - zone.center[1] as i128).unsigned_abs();
    let dz = (cell[2] as i128 - zone.center[2] as i128).unsigned_abs();
    dx * dx + dy * dy + dz * dz <= (zone.radius_cells as u128).pow(2)
}

/// True when any zone suppresses `cell`.
pub fn suppressed_by_any(zones: &[ExclusionZone], cell: [i64; 3]) -> bool {
    zones.iter().any(|z| is_suppressed(z, cell))
}

/// Seed identity for save metadata (ADR-004, frozen shape for
/// `autosave-persistence`): changing the master forks the universe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedMetadata {
    /// Master seed the universe regenerates from.
    pub master_seed: u64,
    /// [`SEED_VERSION`] the save was built against.
    pub seed_version: u32,
    /// Catalog data version, if any (`None` = procedural only).
    /// Version semantics are owned by `star-catalog-streaming`.
    pub catalog_version: Option<String>,
}

impl SeedMetadata {
    /// Build metadata for `master_seed` (+ optional catalog version).
    /// The version stamp is always the current [`SEED_VERSION`].
    pub fn new(master_seed: u64, catalog_version: Option<String>) -> Self {
        Self {
            master_seed,
            seed_version: SEED_VERSION,
            catalog_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::BodyId;

    fn solar_region() -> RegionId {
        RegionId {
            frame: FrameId::SolarSystem,
            cell: [3, -1, 0],
        }
    }

    #[test]
    fn committed_snapshot_pins_the_derivation() {
        // Committed 2026-09-17 (x86-64): ANY grammar/kernel change must
        // bump SEED_VERSION instead of silently forking the universe.
        // Values captured from first run; change deliberately or not at
        // all.
        let a = region_seed(42, solar_region());
        let mut arms = a.layer("galaxy_arms");
        let mut field = a.layer("star_field");
        assert_eq!(arms.next_u64(), 4041527373913777303);
        assert_eq!(field.next_u64(), 7033640996740128932);
    }

    #[test]
    fn layers_from_one_region_share_no_prefix() {
        let seed = region_seed(7, solar_region());
        let mut a = seed.layer("galaxy_arms");
        let mut b = seed.layer("star_field");
        for _ in 0..64 {
            assert_ne!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn layers_are_statistically_independent() {
        // Pearson |r| < 0.1 over 10k samples: domain separation must
        // decorrelate, not just relabel.
        let seed = region_seed(7, solar_region());
        let mut a = seed.layer("galaxy_arms");
        let mut b = seed.layer("star_field");
        const N: usize = 10_000;
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut sxx = 0.0;
        let mut syy = 0.0;
        let mut sxy = 0.0;
        for _ in 0..N {
            let x = a.unit_f64();
            let y = b.unit_f64();
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
        let n = N as f64;
        let r = (n * sxy - sx * sy) / ((n * sxx - sx * sx) * (n * syy - sy * sy)).sqrt();
        assert!(r.abs() < 0.1, "layer correlation {r}");
    }

    #[test]
    fn regions_and_frames_are_independent_seeds() {
        let base = region_seed(42, solar_region());
        let moved = region_seed(
            42,
            RegionId {
                frame: FrameId::SolarSystem,
                cell: [4, -1, 0],
            },
        );
        let reframed = region_seed(
            42,
            RegionId {
                frame: FrameId::StellarNeighborhood,
                cell: [3, -1, 0],
            },
        );
        let remastered = region_seed(43, solar_region());
        assert_ne!(base, moved);
        assert_ne!(base, reframed);
        assert_ne!(base, remastered);
    }

    #[test]
    fn override_suppresses_inside_radius_only() {
        // Catalog object at origin, exclusion radius 2: the object cell
        // and its shell suppress; outside generates procedurally.
        let zone = ExclusionZone {
            center: [0, 0, 0],
            radius_cells: 2,
        };
        assert!(is_suppressed(&zone, [0, 0, 0]));
        assert!(is_suppressed(&zone, [2, 0, 0]));
        assert!(is_suppressed(&zone, [1, 1, 1]));
        assert!(!is_suppressed(&zone, [2, 1, 0]));
        assert!(!is_suppressed(&zone, [10, 0, 0]));
        assert!(!suppressed_by_any(&[], [0, 0, 0]));
        assert!(suppressed_by_any(&[zone], [0, 1, 0]));
        // Extreme coordinates cannot overflow the distance math.
        let far = ExclusionZone {
            center: [i64::MIN, 0, 0],
            radius_cells: u64::MAX,
        };
        assert!(is_suppressed(&far, [i64::MAX, 0, 0]));
    }

    #[test]
    fn determinism_soak_regenerate_after_departure() {
        // Depart (drop everything) and return: identical region content
        // from the same (master, region, layer). The pure-function
        // contract behind "fly away and back".
        let draw = || {
            let seed = region_seed(99, solar_region());
            let mut rng = seed.layer("crater_field");
            let mut out = [0u64; 64];
            for slot in &mut out {
                *slot = rng.next_u64();
            }
            out
        };
        assert_eq!(draw(), draw());
    }

    #[test]
    fn layer_stream_drives_populations_deterministically() {
        // Handoff closed (scale-physics plan Risks): a layer stream
        // feeds sample_orbit with bit-stable results.
        use crate::physics::{OrbitPriors, sample_orbit};
        let priors = OrbitPriors::default();
        let orbit_a = {
            let seed = region_seed(5, solar_region());
            sample_orbit(&mut seed.layer("star_field"), &priors)
        };
        let orbit_b = {
            let seed = region_seed(5, solar_region());
            sample_orbit(&mut seed.layer("star_field"), &priors)
        };
        assert_eq!(orbit_a, orbit_b);
        assert!((1.0..=10_950.0).contains(&orbit_a.period_days));
    }

    #[test]
    fn metadata_carries_version_and_catalog() {
        let m = SeedMetadata::new(42, Some("gaia-dr3/1.0".to_string()));
        assert_eq!(m.master_seed, 42);
        assert_eq!(m.seed_version, SEED_VERSION);
        assert_eq!(m.catalog_version.as_deref(), Some("gaia-dr3/1.0"));
        let bare = SeedMetadata::new(42, None);
        assert_eq!(bare.catalog_version, None);
    }

    #[test]
    fn body_frames_encode_distinctly() {
        let earth = RegionId {
            frame: FrameId::Planetocentric(BodyId::EARTH),
            cell: [0, 0, 0],
        };
        let mars_like = RegionId {
            frame: FrameId::Planetocentric(BodyId(2)),
            cell: [0, 0, 0],
        };
        let enu = RegionId {
            frame: FrameId::LocalEnu(BodyId::EARTH),
            cell: [0, 0, 0],
        };
        assert_ne!(region_seed(42, earth), region_seed(42, mars_like));
        assert_ne!(region_seed(42, earth), region_seed(42, enu));
    }
}
