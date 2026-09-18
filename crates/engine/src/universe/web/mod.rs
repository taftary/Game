//! Stage-0 cosmic web: seeded Zel'dovich generation for W1/L1.
//!
//! Pipeline (one pure function, [`generate_cosmic_web`]):
//!
//! - **A** ([`field`]): hashed Gaussian initial field on an integer
//!   Lagrangian lattice (Irwin–Hall shaping + dyadic smoothing).
//! - **B** ([`displace`]): Zel'dovich displacement `x = q − D₊·∇Ψ` with
//!   NGP deposit into Eulerian density.
//! - **C** ([`classify`]): T-web classification (Jacobi eigensolver,
//!   sqrt-only) + node peaks + Press–Schechter rank masses + home node.
//! - **D** ([`descriptor`]): mass-ranked nodes, filament links, budgeted
//!   dwarf glow, stable content IDs.
//!
//! Deterministic per `(seed, [`UNIVERSE_VERSION`](super::UNIVERSE_VERSION),
//! params)` — same triple replays the same descriptor on every platform
//! (integer decisions + sqrt-only in hashed paths; `exp`/`cbrt` confined
//! to value transforms quantized before hashing). Statistical realism is
//! asserted as tolerance bands below (CSP-005), not exact values.

pub mod classify;
pub mod descriptor;
pub mod displace;
pub mod field;
pub mod params;

pub use classify::{FILAMENT, NODE, SHEET, VOID};
pub use descriptor::{WebDescriptor, WebLink, WebNode};
pub use params::CosmicWebParams;

use classify::classify;
use descriptor::assemble;
use displace::displace_to_eulerian;
use field::initial_field;

/// Generate the stage-0 cosmic web for `seed` under `params`.
///
/// Deterministic per `(seed, version, params)` — same triple replays the
/// same descriptor on every platform. Boot-time cost is ~0.5 s at nominal
/// parameters (single-threaded lattice pipeline); the TECHLEAD cut option
/// is a 96³ lattice if device profiles ever implicate cold start.
///
/// ```
/// use game_engine::universe::{CosmicWebParams, generate_cosmic_web};
///
/// let params = CosmicWebParams::new(32, 4.0, 50.0).expect("test params fit");
/// let a = generate_cosmic_web(1234, &params);
/// let b = generate_cosmic_web(1234, &params);
/// assert_eq!(a, b); // same triple → identical descriptor, everywhere
/// assert!(!a.nodes.is_empty());
/// assert!(a.home_node < a.nodes.len() as u32);
/// let home = a.home();
/// assert!((1.0e12..=1.0e13).contains(&home.mass_msun));
/// ```
pub fn generate_cosmic_web(seed: u64, params: &CosmicWebParams) -> WebDescriptor {
    let field = initial_field(seed, params);
    let euler = displace_to_eulerian(&field, params);
    let classified = classify(&field, &euler, params);
    assemble(seed, &euler, &classified, params)
}

#[cfg(test)]
mod tests {
    use super::super::hash::web_hash;
    use super::*;

    fn nominal() -> CosmicWebParams {
        CosmicWebParams::nominal()
    }

    /// Effective void diameters in Mpc, ZOBOV-style: void cells are
    /// partitioned by multi-source watershed from local density minima
    /// (FIFO BFS over void cells, ties → lowest minimum index, fully
    /// deterministic), so the percolating void network splits into
    /// minimum-centered basins instead of one giant component plus dust.
    /// Diameter = 2·(3V/4π)^⅓ per basin. `cbrt` here is test-only math
    /// feeding band asserts — 1-ulp drift cannot cross a 10–100 Mpc band.
    fn void_diameters_mpc(seed: u64, params: &CosmicWebParams) -> Vec<f64> {
        let field = initial_field(seed, params);
        let euler = displace_to_eulerian(&field, params);
        let classified = classify(&field, &euler, params);
        let n = params.lattice_cells as usize;
        let flat = |x: usize, y: usize, z: usize| x + n * (y + n * z);
        let dirs = [
            (-1, 0, 0),
            (1, 0, 0),
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
        ];
        // Local minima among void cells, compared WITHIN the void mask
        // only: a void cell ringed by non-void cells is still the minimum
        // of its (singleton) basin. Ties → lowest flat index.
        let mut minima = Vec::new();
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let idx = flat(x, y, z);
                    if classified.web_type[idx] != VOID {
                        continue;
                    }
                    let d = euler.density[idx];
                    let mut is_min = true;
                    for (dx, dy, dz) in dirs {
                        let nidx = flat(
                            (x as i64 + dx).rem_euclid(n as i64) as usize,
                            (y as i64 + dy).rem_euclid(n as i64) as usize,
                            (z as i64 + dz).rem_euclid(n as i64) as usize,
                        );
                        if classified.web_type[nidx] != VOID {
                            continue;
                        }
                        let nd = euler.density[nidx];
                        if nd < d || (nd == d && nidx < idx) {
                            is_min = false;
                            break;
                        }
                    }
                    if is_min {
                        minima.push(idx);
                    }
                }
            }
        }
        // Multi-source watershed: every void cell joins its nearest
        // minimum's basin (FIFO from ascending minima = deterministic
        // tiebreaks).
        let mut basin = vec![-1_i64; n * n * n];
        let mut queue = std::collections::VecDeque::new();
        for (label, root) in minima.iter().enumerate() {
            basin[*root] = label as i64;
            queue.push_back(*root);
        }
        while let Some(c) = queue.pop_front() {
            let cx = c % n;
            let cy = (c / n) % n;
            let cz = c / (n * n);
            for (dx, dy, dz) in dirs {
                let nidx = flat(
                    (cx as i64 + dx).rem_euclid(n as i64) as usize,
                    (cy as i64 + dy).rem_euclid(n as i64) as usize,
                    (cz as i64 + dz).rem_euclid(n as i64) as usize,
                );
                if classified.web_type[nidx] == VOID && basin[nidx] == -1 {
                    basin[nidx] = basin[c];
                    queue.push_back(nidx);
                }
            }
        }
        let mut volumes = vec![0_u64; minima.len()];
        for (i, b) in basin.iter().enumerate() {
            if classified.web_type[i] == VOID {
                // Every void cell belongs to exactly one basin: minima are
                // compared within the void mask, so each void component
                // seeds at least its lowest-index minimum and the
                // watershed covers it fully.
                debug_assert!(*b >= 0, "void cell left unassigned");
                volumes[*b as usize] += 1;
            }
        }
        let mut diameters: Vec<f64> = volumes
            .iter()
            .map(|v| {
                let v_mpc3 = *v as f64 * params.cell_size_mpc.powi(3);
                2.0 * (3.0 * v_mpc3 / (4.0 * std::f64::consts::PI)).cbrt()
            })
            .collect();
        diameters.sort_by(|a, b| a.total_cmp(b));
        diameters
    }

    fn median(sorted: &[f64]) -> f64 {
        sorted[sorted.len() / 2]
    }

    #[test]
    fn nominal_descriptor_replays_identically() {
        let p = nominal();
        assert_eq!(generate_cosmic_web(1234, &p), generate_cosmic_web(1234, &p));
    }

    #[test]
    fn statistical_gates_hold_at_nominal() {
        let p = nominal();
        let web = generate_cosmic_web(1234, &p);
        // Node count band (thousands of groups/clusters in 250 Mpc).
        assert!(
            (3_000..=8_000).contains(&web.nodes.len()),
            "node count out of band: {}",
            web.nodes.len()
        );
        // Mass function shape: power-law dominated (median below M\*)
        // with a rare massive tail (a Coma-scale node exists).
        let mut masses: Vec<f64> = web.nodes.iter().map(|nd| nd.mass_msun).collect();
        masses.sort_by(|a, b| a.total_cmp(b));
        let median_mass = masses[masses.len() / 2];
        assert!(
            median_mass < p.mass_star_msun,
            "median mass above M*: {median_mass}"
        );
        let max_mass = masses[masses.len() - 1];
        assert!(max_mass >= 3.0e14, "no rich-cluster tail: max {max_mass}");
        let low = masses.iter().filter(|m| **m < 2.0e13).count();
        let high = masses.iter().filter(|m| **m > 1.0e14).count();
        assert!(
            low >= high,
            "mass function not bottom-heavy: low {low} vs high {high}"
        );
        let ultra = masses.iter().filter(|m| **m > 3.0e14).count();
        assert!(
            ultra * 20 <= masses.len(),
            "cutoff too weak: {ultra} ultra-massive of {}",
            masses.len()
        );
        // Filament links reach the observed 50–80 Mpc class.
        let pos = |i: u32| web.nodes[i as usize].position_mpc;
        let mut max_len = 0.0_f64;
        let mut long = 0;
        for link in &web.links {
            let a = pos(link.a);
            let b = pos(link.b);
            let len =
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
            max_len = max_len.max(len);
            if len >= 30.0 {
                long += 1;
            }
        }
        assert!(max_len >= 50.0, "no long filaments: max {max_len}");
        assert!(long >= 50, "too few 30+ Mpc links: {long}");
        // Home node: group band, inside the sphere.
        let home = web.home();
        assert!(
            (p.home_mass_lo_msun..=p.home_mass_hi_msun).contains(&home.mass_msun),
            "home mass out of band: {}",
            home.mass_msun
        );
        let hr = (home.position_mpc[0].powi(2)
            + home.position_mpc[1].powi(2)
            + home.position_mpc[2].powi(2))
        .sqrt();
        assert!(hr <= p.descriptor_radius_mpc, "home outside sphere: {hr}");
        // Spawn reference: the home node has a departure link.
        assert!(
            web.strongest_link_from(web.home_node).is_some(),
            "home node is isolated — no spawn path"
        );
        // Glow budget respected.
        assert!(
            web.glow_mpc.len() <= p.max_glow_points as usize,
            "glow over budget: {}",
            web.glow_mpc.len()
        );
        assert!(!web.glow_mpc.is_empty(), "no glow emitted");
    }

    #[test]
    fn void_bands_hold_at_nominal() {
        let p = nominal();
        let field = initial_field(1234, &p);
        let euler = displace_to_eulerian(&field, &p);
        let classified = classify(&field, &euler, &p);
        assert!(
            (0.60..=0.90).contains(&classified.void_fraction),
            "void fraction out of band: {}",
            classified.void_fraction
        );
        let mut diameters = void_diameters_mpc(1234, &p);
        // VoidFinder-style catalog floor (verified precedent: minimum
        // radius cut): single-cell shot-noise pockets are not voids.
        diameters.retain(|d| *d >= 8.0);
        assert!(!diameters.is_empty(), "no enclosed voids found");
        let med = median(&diameters);
        assert!(
            (10.0..=100.0).contains(&med),
            "void median diameter out of band: {med}"
        );
    }

    #[test]
    fn ulp_perturbation_cannot_flip_web_hash() {
        // Drift model: 1 ulp relative (toward +inf) on the libm-derived
        // fields — masses via the exp-built PS table, radii via cbrt.
        // Positions, links, and glow come from exact arithmetic
        // (+,-,*,/,sqrt: all correctly rounded per IEEE-754, hence
        // bit-identical on every platform) and are covered by
        // replay-identical + the committed vectors instead; perturbing
        // them would only probe exact-half lattice fractions, which are
        // stable by exactness rather than by quantum.
        let p = nominal();
        let mut web = generate_cosmic_web(11, &p);
        let before = web_hash(&web);
        let bump = |v: f64| v + v.abs() * f64::EPSILON;
        let near_half = |v: f64, q: f64| {
            let f = ((v * q).abs()).fract();
            (f - 0.5).abs() < 1e-9
        };
        let mut skipped = 0;
        for node in &mut web.nodes {
            if near_half(node.mass_msun / 1.0e9, 1.0) {
                skipped += 1;
                continue;
            }
            node.mass_msun = bump(node.mass_msun);
            if near_half(node.virial_radius_mpc, 1_000.0) {
                skipped += 1;
                continue;
            }
            node.virial_radius_mpc = bump(node.virial_radius_mpc);
        }
        assert_eq!(skipped, 0, "unexpected exact-half libm values");
        assert_eq!(web_hash(&web), before);
    }

    #[test]
    fn different_seeds_diverge() {
        let p = CosmicWebParams::new(32, 4.0, 50.0).expect("test params fit");
        let a = generate_cosmic_web(1, &p);
        let b = generate_cosmic_web(2, &p);
        assert_ne!(a.nodes[0].position_mpc, b.nodes[0].position_mpc);
        assert_ne!(web_hash(&a), web_hash(&b));
    }

    #[test]
    fn committed_web_vectors_pin_stage0() {
        // Change-detectors, not oracles: any intentional generation
        // change updates these alongside a UNIVERSE_VERSION bump.
        assert_eq!(
            web_hash(&generate_cosmic_web(1234, &CosmicWebParams::nominal())),
            14_062_203_475_186_774_008
        );
    }
}
