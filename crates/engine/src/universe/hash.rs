//! Descriptor hashes: byte-identical across runs and platforms.
//!
//! [`galaxy_hash`], [`system_hash`], [`planet_hash`] fold every
//! descriptor field through FNV-1a over **quantized** integers —
//! never raw floats. Two inputs within half a quantum hash equal, so
//! 1-ulp drift between ARM and x86 cannot flip a hash (risks #2). The
//! hasher is fully specified here (not `std`'s `DefaultHasher`, whose
//! algorithm may change across toolchains), so a hash value is stable
//! for a fixed `(descriptor, [`UNIVERSE_VERSION`](super::UNIVERSE_VERSION))`
//! pair forever.
//!
//! Field order is declaration order, children in index order; enum
//! discriminants hash as `u8`. Reordering fields or enum variants
//! changes hashes — that is a version-bump decision, never silent.
//!
//! Quantums (chosen per unit, documented once here):
//!
//! - galactic positions: micro-light-years (`1e6`)
//! - orbit radii: nano-AU (`1e9`)
//! - planet radius/gravity: milli-units (`1e3`)
//! - atmosphere color/density, resource bias: `1e6` / `1e3`
//! - web node/glow positions: milli-Mpc (`1e3`)
//! - web node masses: giga-solar-mass units (`mass_msun / 1e9`, quantum 1)
//! - web virial radii, link densities: `1e3`
//!
//! ```
//! use game_engine::universe::{generate_galaxy, galaxy_hash};
//!
//! // Same triple replays the same hash on every platform…
//! assert_eq!(galaxy_hash(&generate_galaxy(7, 50)), galaxy_hash(&generate_galaxy(7, 50)));
//! // …and different seeds diverge.
//! assert_ne!(galaxy_hash(&generate_galaxy(7, 50)), galaxy_hash(&generate_galaxy(8, 50)));
//! ```

use super::descriptors::{
    GalaxyDescriptor, PlanetDescriptor, PlanetType, SpectralClass, SystemDescriptor,
};
use super::web::WebDescriptor;
use crate::core::quantize_f64;

/// FNV-1a 64 offset basis / prime (same constants as
/// [`crate::core`] domain hashing — one specified primitive).
fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    hash
}

/// Fixed FNV-1a fold over little-endian field bytes.
struct FnvFold(u64);

impl FnvFold {
    fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = fnv1a(&value.to_le_bytes(), self.0);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn spectral_discriminant(class: SpectralClass) -> u32 {
    match class {
        SpectralClass::O => 0,
        SpectralClass::B => 1,
        SpectralClass::A => 2,
        SpectralClass::F => 3,
        SpectralClass::G => 4,
        SpectralClass::K => 5,
        SpectralClass::M => 6,
    }
}

fn planet_discriminant(kind: PlanetType) -> u32 {
    match kind {
        PlanetType::Rocky => 0,
        PlanetType::Desert => 1,
        PlanetType::Ice => 2,
        PlanetType::Volcanic => 3,
        PlanetType::Toxic => 4,
        PlanetType::Oceanic => 5,
    }
}

/// Hash one planet: version is inherited from the parent descriptors'
/// stamp (planets don't carry their own copy — the galaxy's
/// `universe_version` covers the whole tree; see [`galaxy_hash`]).
pub fn planet_hash(planet: &PlanetDescriptor) -> u64 {
    let mut h = FnvFold::new();
    h.write_u64(planet.id.seed());
    h.write_u32(planet.id.star_index());
    h.write_u32(planet.id.planet_index());
    h.write_u32(planet_discriminant(planet.planet_type));
    h.write_i64(quantize_f64(f64::from(planet.radius_km), 1_000.0));
    h.write_i64(quantize_f64(f64::from(planet.gravity_g), 1_000.0));
    h.write_u64(planet.mesh_seed);
    for c in planet.atmosphere.color {
        h.write_i64(quantize_f64(f64::from(c), 1_000_000.0));
    }
    h.write_i64(quantize_f64(
        f64::from(planet.atmosphere.density),
        1_000_000.0,
    ));
    let b = planet.resource_bias;
    for v in [b.energy, b.metal, b.water_ice, b.organics, b.rare] {
        h.write_i64(quantize_f64(f64::from(v), 1_000.0));
    }
    h.write_u32(planet.companion_count);
    h.finish()
}

/// Hash one system: id + star + planets in index order.
pub fn system_hash(system: &SystemDescriptor) -> u64 {
    let mut h = FnvFold::new();
    h.write_u64(system.id.seed());
    h.write_u32(system.id.star_index());
    h.write_u32(spectral_discriminant(system.star.spectral_class));
    for c in system.star.position_ly {
        h.write_i64(quantize_f64(c, 1_000_000.0));
    }
    h.write_u32(system.star.companion_count);
    h.write_u32(system.planets.len() as u32);
    // Child hashes folded in index order — order-dependent by design.
    let mut acc = h.finish();
    for planet in &system.planets {
        h = FnvFold(acc);
        h.write_i64(quantize_f64(planet.orbit_radius_au, 1_000_000_000.0));
        h.write_u64(planet_hash(&planet.descriptor));
        acc = h.finish();
    }
    acc
}

/// Hash one galaxy: version stamp + id + stars in index order. The
/// `universe_version` participates first, so a version bump always
/// re-rolls hashes (migration signal, never silent drift).
pub fn galaxy_hash(galaxy: &GalaxyDescriptor) -> u64 {
    let mut h = FnvFold::new();
    h.write_u32(galaxy.universe_version);
    h.write_u64(galaxy.seed);
    h.write_u32(galaxy.stars.len() as u32);
    let mut acc = h.finish();
    for star in &galaxy.stars {
        h = FnvFold(acc);
        h.write_u32(star.star_index);
        h.write_u32(spectral_discriminant(star.spectral_class));
        for c in star.position_ly {
            h.write_i64(quantize_f64(c, 1_000_000.0));
        }
        h.write_u32(star.companion_count);
        acc = h.finish();
    }
    acc
}

/// Hash one cosmic web: version stamp + seed + counts + nodes, links,
/// and glow points in canonical order. Masses hash in giga-solar-mass
/// units, positions in milli-Mpc — 1-ulp value-transform drift (from the
/// stage-C `exp`/`cbrt` or stage-D `sqrt`) vanishes in the quantum.
///
/// Lattice-exact positions may sit exactly on half-quantum points (dyadic
/// cell-center fractions); those are stable across platforms anyway
/// because every op in the position path (+,-,*,/ on integers and dyadic
/// rationals) is exactly rounded per IEEE-754 — identical inputs replay
/// identical bits, so the committed vectors pin them.
pub fn web_hash(web: &WebDescriptor) -> u64 {
    let mut h = FnvFold::new();
    h.write_u32(web.universe_version);
    h.write_u64(web.seed);
    h.write_u32(web.nodes.len() as u32);
    let mut acc = h.finish();
    for node in &web.nodes {
        h = FnvFold(acc);
        h.write_u32(node.node_index);
        for c in node.position_mpc {
            h.write_i64(quantize_f64(c, 1_000.0));
        }
        h.write_i64(quantize_f64(node.mass_msun / 1.0e9, 1.0));
        h.write_i64(quantize_f64(node.virial_radius_mpc, 1_000.0));
        acc = h.finish();
    }
    h = FnvFold(acc);
    h.write_u32(web.links.len() as u32);
    acc = h.finish();
    for link in &web.links {
        h = FnvFold(acc);
        h.write_u32(link.a);
        h.write_u32(link.b);
        h.write_i64(quantize_f64(f64::from(link.density), 1_000.0));
        acc = h.finish();
    }
    h = FnvFold(acc);
    h.write_u32(web.glow_mpc.len() as u32);
    acc = h.finish();
    for g in &web.glow_mpc {
        h = FnvFold(acc);
        for c in g {
            h.write_i64(quantize_f64(f64::from(*c), 1_000.0));
        }
        acc = h.finish();
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::super::generate::{generate_galaxy, generate_system};
    use super::*;

    #[test]
    fn repeated_runs_replay_identical_hashes() {
        let first = galaxy_hash(&generate_galaxy(2026, 300));
        for _ in 0..50 {
            assert_eq!(galaxy_hash(&generate_galaxy(2026, 300)), first);
        }
        let galaxy = generate_galaxy(2026, 300);
        let system = generate_system(2026, &galaxy.stars[0]);
        let first_sys = system_hash(&system);
        for _ in 0..50 {
            assert_eq!(
                system_hash(&generate_system(2026, &galaxy.stars[0])),
                first_sys
            );
        }
    }

    #[test]
    fn ulp_perturbation_cannot_flip_a_hash() {
        // The risks-#2 core: 1-ulp platform drift on any hashed float
        // must vanish in the quantum.
        let mut galaxy = generate_galaxy(11, 50);
        let before = galaxy_hash(&galaxy);
        for star in &mut galaxy.stars {
            star.position_ly[0] += f64::EPSILON * 4.0;
        }
        assert_eq!(galaxy_hash(&galaxy), before);

        let mut system = generate_system(11, &galaxy.stars[0]);
        let before_sys = system_hash(&system);
        for planet in &mut system.planets {
            planet.orbit_radius_au += f64::EPSILON * 4.0;
            planet.descriptor.radius_km += f32::EPSILON;
        }
        assert_eq!(system_hash(&system), before_sys);
    }

    #[test]
    fn version_stamp_participates() {
        let mut galaxy = generate_galaxy(3, 20);
        let before = galaxy_hash(&galaxy);
        galaxy.universe_version += 1;
        assert_ne!(galaxy_hash(&galaxy), before);
    }

    #[test]
    fn committed_vectors_pin_stage1_and_stage2() {
        // Change-detectors, not oracles: any intentional generation
        // change updates these alongside a UNIVERSE_VERSION bump.
        assert_eq!(
            galaxy_hash(&generate_galaxy(1234, 100)),
            4_159_227_938_759_867_464
        );
        let galaxy = generate_galaxy(1234, 100);
        assert_eq!(
            system_hash(&generate_system(1234, &galaxy.stars[0])),
            2_970_470_675_423_035_933
        );
    }
}
