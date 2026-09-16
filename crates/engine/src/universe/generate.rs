//! Universe generators: pure `(seed, version, …) → descriptor`.
//!
//! Stage 1 ([`generate_galaxy`]) ships in this milestone; stage 2
//! (system → planets) arrives in UMAP-004, stage 3 (planet → surface)
//! with the descent/surface milestones. See `docs/game/universe.md`.
//!
//! Cross-platform rules (risks #2) enforced here, not just documented:
//! the generator uses integer decisions, rejection sampling, and the
//! IEEE-exact `sqrt` only — no `sin`/`cos`/`ln`/`exp` anywhere in a
//! hashed path. Positions are stored log-compressed (rational saturating
//! map, basic ops only) and hashed quantized (UMAP-006), so 1-ulp
//! platform drift cannot change any descriptor hash.

use super::descriptors::{
    Atmosphere, GalaxyDescriptor, PlanetDescriptor, PlanetType, ResourceBias, SpectralClass,
    StarDescriptor, SystemDescriptor, SystemPlanet,
};
use super::ids::PlanetId;
use crate::core::SeededRng;

/// Compressed map-space radius of the galaxy in light-years:
/// `docs/game/journey.md` L2 gives a 10k–100k ly compressed extent; v1
/// uses the full band (100k ly across).
pub const GALAXY_RADIUS_LY: f64 = 50_000.0;

/// Knee of the saturating compression map: inside a few knee radii the
/// map is near-linear (local structure preserved); far field compresses
/// toward [`GALAXY_RADIUS_LY`].
pub const COMPRESSION_KNEE_LY: f64 = 8_000.0;

/// Physical half-thickness of the generated disk in light-years,
/// tapered to zero at the rim.
pub const DISK_HALF_THICKNESS_LY: f64 = 1_200.0;

/// v1 shipped star count for the galaxy map (OQ-1, TECHLEAD-approved in
/// UMAP-010). Headroom math, worst case (Low tier, phones):
///
/// - Draw: 1 draw call, 25k point verts — 20× under the 500k surface tri
///   budget, and points are cheaper than tris (near-zero fragment work).
/// - Zoomed-out overdraw: 25k points into ~1.2M pixels (1080p × 0.6
///   dynamic floor) ≈ 2% coverage pre-blend — negligible.
/// - CPU cull: 25k × a few float ops per frame ≈ 0.1 ms.
/// - Generation: ~150k SplitMix steps — sub-millisecond; cold start
///   unaffected. Memory: ~1.2 MB of descriptors.
/// - Visual floor: below ~10k the 100k-ly disk reads empty; 25k keeps
///   the dense core legible at full zoom-out.
///
/// Tier-gate rule: if M6 device profiles ever implicate the map views,
/// cut this one constant (floor: 10,000) — no code changes elsewhere.
/// The generator itself stays parameterized; this is only the shipped
/// default.
pub const DEFAULT_STAR_COUNT: u32 = 25_000;

/// Compress a physical radius into map space: `R·r/(r+Rc)`. Rational and
/// saturating — basic ops only, so identical on every platform. 0 maps
/// to 0, infinity saturates at [`GALAXY_RADIUS_LY`].
pub fn compress_radius(physical_ly: f64) -> f64 {
    debug_assert!(physical_ly >= 0.0 && physical_ly.is_finite());
    GALAXY_RADIUS_LY * physical_ly / (physical_ly + COMPRESSION_KNEE_LY)
}

/// Inverse of [`compress_radius`]: recover the physical radius from a
/// compressed one. Views that need physical-ish distances (orbit radii
/// are AU-scale and never do) use this; the maps draw compressed space
/// directly.
pub fn uncompress_radius(compressed_ly: f64) -> f64 {
    debug_assert!((0.0..GALAXY_RADIUS_LY).contains(&compressed_ly));
    COMPRESSION_KNEE_LY * compressed_ly / (GALAXY_RADIUS_LY - compressed_ly)
}

/// Spectral-class weights per mille (IMF-flavored: M dwarfs dominate,
/// O stars are one-in-a-thousand exotica). Integer thresholds keep the
/// pick platform-identical.
fn spectral_class(rng: &mut SeededRng) -> SpectralClass {
    match rng.below(1000) {
        0 => SpectralClass::O,
        1..=14 => SpectralClass::B,
        15..=69 => SpectralClass::A,
        70..=159 => SpectralClass::F,
        160..=299 => SpectralClass::G,
        300..=549 => SpectralClass::K,
        _ => SpectralClass::M,
    }
}

/// Companion count: 0 is the common case (70%), wide visual-only
/// companions trail off to 3. Integer thresholds, deterministic.
fn companion_count(rng: &mut SeededRng) -> u32 {
    match rng.below(100) {
        0..=69 => 0,
        70..=89 => 1,
        90..=97 => 2,
        _ => 3,
    }
}

/// Generate the galaxy for `seed`: `star_count` stars with compressed
/// f64 positions, spectral classes, and companion counts.
///
/// Deterministic per `(seed, [`UNIVERSE_VERSION`](super::UNIVERSE_VERSION),
/// star_count)` — same triple replays the same descriptor on every
/// platform. `star_count` is a parameter (not a constant) so the v1
/// default proposed in UMAP-010 plugs in without touching this code;
/// changing the shipped default changes output, which is a version-bump
/// decision, never silent.
///
/// Shape: uniform-density disk (rejection-sampled, no trig) with a
/// dense core (30% of stars pulled to 18% radius) and rim-tapered
/// thickness, compressed radially into map space. Spiral structure is a
/// view concern (nebula impostors, UMAP-011), not data.
///
/// ```
/// use game_engine::universe::{generate_galaxy, GALAXY_RADIUS_LY};
///
/// let a = generate_galaxy(1234, 100);
/// let b = generate_galaxy(1234, 100);
/// assert_eq!(a, b); // same triple → identical descriptor, everywhere
/// assert_eq!(a.stars.len(), 100);
/// for star in &a.stars {
///     let r = star.position_ly.iter().map(|c| c * c).sum::<f64>().sqrt();
///     assert!(r <= GALAXY_RADIUS_LY);
/// }
/// ```
pub fn generate_galaxy(seed: u64, star_count: u32) -> GalaxyDescriptor {
    let mut rng = SeededRng::stream(seed, "galaxy/stars");
    let mut stars = Vec::with_capacity(star_count as usize);
    for star_index in 0..star_count {
        // Uniform point in the unit disk by rejection: two integer-grid
        // floats, accept inside the circle. No angle, no trig.
        let (ux, uz) = loop {
            let x = rng.range_f64(-1.0, 1.0);
            let z = rng.range_f64(-1.0, 1.0);
            if x * x + z * z <= 1.0 {
                break (x, z);
            }
        };
        // Dense core: ~30% of stars live at 18% radius.
        let core = if rng.below(100) < 30 { 0.18 } else { 1.0 };
        let px = ux * GALAXY_RADIUS_LY * core;
        let pz = uz * GALAXY_RADIUS_LY * core;
        // Rim-tapered thickness: full at the center, zero at the rim.
        let rim = (px * px + pz * pz).sqrt() / GALAXY_RADIUS_LY;
        let py = rng.range_f64(-1.0, 1.0) * DISK_HALF_THICKNESS_LY * (1.0 - rim);
        // Radial compression into map space (direction preserved).
        let physical = (px * px + py * py + pz * pz).sqrt();
        let scale = if physical > 0.0 {
            compress_radius(physical) / physical
        } else {
            0.0
        };
        stars.push(StarDescriptor {
            star_index,
            spectral_class: spectral_class(&mut rng),
            position_ly: [px * scale, py * scale, pz * scale],
            companion_count: companion_count(&mut rng),
        });
    }
    GalaxyDescriptor::new(seed, stars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_triple_replays_identical_descriptor() {
        assert_eq!(generate_galaxy(1234, 500), generate_galaxy(1234, 500));
    }

    #[test]
    fn different_seeds_diverge() {
        let a = generate_galaxy(1, 200);
        let b = generate_galaxy(2, 200);
        assert_ne!(a.stars[0].position_ly, b.stars[0].position_ly);
    }

    #[test]
    fn count_and_indices_are_exact() {
        let galaxy = generate_galaxy(9, 1_000);
        assert_eq!(galaxy.stars.len(), 1_000);
        assert_eq!(galaxy.id.seed(), 9);
        for (i, star) in galaxy.stars.iter().enumerate() {
            assert_eq!(star.star_index, i as u32);
        }
    }

    #[test]
    fn all_stars_inside_compressed_extent() {
        let galaxy = generate_galaxy(77, 5_000);
        for star in &galaxy.stars {
            let [x, y, z] = star.position_ly;
            assert!(x.is_finite() && y.is_finite() && z.is_finite());
            let r = (x * x + y * y + z * z).sqrt();
            assert!(r <= GALAXY_RADIUS_LY);
        }
    }

    #[test]
    fn compression_round_trips() {
        for physical in [0.0, 100.0, 8_000.0, 50_000.0, 1_000_000.0] {
            let round_tripped = uncompress_radius(compress_radius(physical));
            let rel = ((round_tripped - physical) / physical.max(1.0)).abs();
            assert!(rel < 1e-9, "{physical} -> {round_tripped}");
        }
    }

    #[test]
    fn spectral_mix_is_dwarf_dominated_with_rare_o_stars() {
        let galaxy = generate_galaxy(5, 20_000);
        let mut m = 0;
        let mut o = 0;
        for star in &galaxy.stars {
            match star.spectral_class {
                SpectralClass::M => m += 1,
                SpectralClass::O => o += 1,
                _ => {}
            }
        }
        assert!(m as f64 / 20_000.0 > 0.40, "M fraction too low: {m}");
        assert!(o as f64 / 20_000.0 < 0.01, "O fraction too high: {o}");
    }

    #[test]
    fn empty_galaxy_is_valid() {
        let galaxy = generate_galaxy(0, 0);
        assert!(galaxy.stars.is_empty());
        assert_eq!(galaxy.universe_version, super::super::UNIVERSE_VERSION);
    }

    #[test]
    fn default_star_count_is_honored() {
        let galaxy = generate_galaxy(1, DEFAULT_STAR_COUNT);
        assert_eq!(galaxy.stars.len(), DEFAULT_STAR_COUNT as usize);
    }
}

/// Inner edge of the first orbit in AU: keeps the innermost planet clear
/// of the stellar corona sprite the SystemMap view draws.
pub const INNER_ORBIT_AU: f64 = 0.3;

/// Planet count weights per hundred: compact systems are the common
/// case, 8-planet full houses are rare. Integer thresholds, deterministic.
fn planet_count(rng: &mut SeededRng) -> u32 {
    match rng.below(100) {
        0..=9 => 1,
        10..=29 => 2,
        30..=49 => 3,
        50..=64 => 4,
        65..=77 => 5,
        78..=87 => 6,
        88..=94 => 7,
        _ => 8,
    }
}

/// Planet type weights per hundred. All six v1 types are generatable
/// from day one; the shipped subset finalizes with UX in UMAP-021.
fn planet_type(rng: &mut SeededRng) -> PlanetType {
    match rng.below(100) {
        0..=29 => PlanetType::Rocky,
        30..=49 => PlanetType::Desert,
        50..=69 => PlanetType::Ice,
        70..=79 => PlanetType::Volcanic,
        80..=89 => PlanetType::Toxic,
        _ => PlanetType::Oceanic,
    }
}

/// Per-type look and habitability data: atmosphere tint (linear RGB),
/// atmosphere density band, and resource-bias base multipliers
/// (energy, metal, water/ice, organics, rare). The views (UMAP-021)
/// read these off the descriptor — no hardcoded palettes in rendering.
fn type_profile(planet_type: PlanetType) -> ([f32; 3], (f32, f32), [f32; 5]) {
    match planet_type {
        PlanetType::Rocky => ([0.75, 0.55, 0.40], (0.3, 0.7), [1.0, 1.2, 0.6, 0.5, 0.4]),
        PlanetType::Desert => ([0.90, 0.75, 0.50], (0.2, 0.5), [1.3, 0.9, 0.2, 0.3, 0.5]),
        PlanetType::Ice => ([0.65, 0.80, 0.95], (0.1, 0.4), [0.7, 0.6, 1.6, 0.4, 0.3]),
        PlanetType::Volcanic => ([0.55, 0.25, 0.20], (0.8, 1.4), [1.4, 1.5, 0.1, 0.2, 1.2]),
        PlanetType::Toxic => ([0.45, 0.75, 0.30], (0.7, 1.2), [0.8, 0.8, 0.5, 1.4, 0.9]),
        PlanetType::Oceanic => ([0.25, 0.50, 0.80], (0.5, 0.9), [0.9, 0.5, 1.5, 1.5, 0.4]),
    }
}

/// Visual-only companion (moon/ring) count: most planets are bare,
/// triple systems are rare. Integer thresholds, deterministic.
fn visual_companions(rng: &mut SeededRng) -> u32 {
    match rng.below(100) {
        0..=49 => 0,
        50..=74 => 1,
        75..=89 => 2,
        _ => 3,
    }
}

/// Generate the system around `star` for galaxy `seed`: planet count,
/// AU-scale compressed orbit radii (patched positions — the radial
/// coordinate in the system frame; true anomaly is view/transit state,
/// not data, so no trig lives here), types, 2–8 km radii, gravity,
/// mesh seeds, atmospheres, resource biases, visual companions.
///
/// Deterministic per `(seed, [`UNIVERSE_VERSION`](super::UNIVERSE_VERSION),
/// star_index)` — each star draws from its own domain-separated stream,
/// so systems never share a sequence and regenerating one star never
/// perturbs its siblings.
///
/// ```
/// use game_engine::universe::{generate_system, SpectralClass, StarDescriptor};
///
/// let star = StarDescriptor {
///     star_index: 0,
///     spectral_class: SpectralClass::G,
///     position_ly: [12000.0, 0.0, 800.0],
///     companion_count: 0,
/// };
/// let a = generate_system(42, &star);
/// let b = generate_system(42, &star);
/// assert_eq!(a, b); // same inputs → identical descriptor, everywhere
/// assert!((1..=8).contains(&a.planets.len()));
/// let mut prev = 0.0;
/// for planet in &a.planets {
///     assert!(planet.orbit_radius_au > prev); // strictly increasing
///     prev = planet.orbit_radius_au;
/// }
/// ```
pub fn generate_system(seed: u64, star: &StarDescriptor) -> SystemDescriptor {
    let domain = format!("system/{}/planets", star.star_index);
    let mut rng = SeededRng::stream(seed, &domain);
    let count = planet_count(&mut rng);
    let mut planets = Vec::with_capacity(count as usize);
    // Geometric spacing: each orbit 1.35–2.10× the previous — basic
    // float ops only, deterministic on every platform.
    let mut orbit_au = rng.range_f64(INNER_ORBIT_AU, INNER_ORBIT_AU * 2.0);
    for planet_index in 0..count {
        let kind = planet_type(&mut rng);
        let (tint, (lo_density, hi_density), bias) = type_profile(kind);
        // Integer-grid jitter ±10% on the bias bases: deterministic
        // without float-stream dependence.
        let jitter = 0.9 + rng.below(21) as f32 / 100.0;
        let id = PlanetId::new(seed, star.star_index, planet_index);
        let descriptor = PlanetDescriptor {
            id,
            radius_km: rng.range_f64(2.0, 8.0) as f32,
            planet_type: kind,
            gravity_g: rng.range_f64(0.8, 1.2) as f32,
            mesh_seed: rng.next_u64(),
            atmosphere: Atmosphere {
                color: tint,
                density: rng.range_f64(lo_density.into(), hi_density.into()) as f32,
            },
            resource_bias: ResourceBias {
                energy: bias[0] * jitter,
                metal: bias[1] * jitter,
                water_ice: bias[2] * jitter,
                organics: bias[3] * jitter,
                rare: bias[4] * jitter,
            },
            companion_count: visual_companions(&mut rng),
        };
        planets.push(SystemPlanet {
            orbit_radius_au: orbit_au,
            descriptor,
        });
        orbit_au *= rng.range_f64(1.35, 2.10);
    }
    SystemDescriptor::new(seed, star.clone(), planets)
}

#[cfg(test)]
mod stage2_tests {
    use super::*;

    fn g_star() -> StarDescriptor {
        StarDescriptor {
            star_index: 3,
            spectral_class: SpectralClass::G,
            position_ly: [12000.0, 0.0, 800.0],
            companion_count: 0,
        }
    }

    #[test]
    fn same_inputs_replay_identical_system() {
        assert_eq!(
            generate_system(42, &g_star()),
            generate_system(42, &g_star())
        );
    }

    #[test]
    fn sibling_stars_get_independent_systems() {
        let mut other = g_star();
        other.star_index = 4;
        let a = generate_system(42, &g_star());
        let b = generate_system(42, &other);
        // Ids embed the star index, so siblings always diverge; the
        // domain-separated streams diverge the content too.
        assert_ne!(a, b);
    }

    #[test]
    fn bands_and_ids_hold() {
        let system = generate_system(7, &g_star());
        assert!((1..=8).contains(&system.planets.len()));
        assert_eq!(system.id, super::super::ids::SystemId::new(7, 3));
        let mut prev = 0.0;
        for (i, planet) in system.planets.iter().enumerate() {
            let d = &planet.descriptor;
            assert_eq!(d.id, PlanetId::new(7, 3, i as u32));
            assert_eq!(d.id.system_id(), system.id);
            assert!(planet.orbit_radius_au > prev);
            assert!(planet.orbit_radius_au >= INNER_ORBIT_AU);
            prev = planet.orbit_radius_au;
            assert!((2.0..=8.0).contains(&d.radius_km));
            assert!((0.8..=1.2).contains(&d.gravity_g));
            assert!(d.atmosphere.density.is_finite());
        }
    }

    #[test]
    fn all_six_types_are_generatable() {
        let mut seen = [false; 6];
        for seed in 0..50 {
            let star = StarDescriptor {
                star_index: seed as u32 % 7,
                ..g_star()
            };
            for planet in &generate_system(seed, &star).planets {
                seen[planet.descriptor.planet_type as usize] = true;
            }
        }
        assert!(seen.iter().all(|s| *s));
    }
}
