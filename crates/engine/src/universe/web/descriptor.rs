//! Stage D — renderable web descriptor: nodes, filament links, glow.
//!
//! Peaks become mass-ranked nodes (Press–Schechter quantile by density
//! rank), node pairs joined by dense segments become filament links, and
//! links seed dwarf glow points under an exact two-pass budget. Link
//! order is canonical `(a, b)` ascending; glow streams are consumed in
//! that order, so the descriptor replays bit-identically. Transcendentals
//! (`sqrt` for basis normalization) live only in value transforms whose
//! outputs are quantized before hashing.

use super::classify::{ClassifiedWeb, PsMassTable, accept_peaks, virial_radius_mpc};
use super::displace::EulerianField;
use super::field::index;
use super::params::CosmicWebParams;
use crate::core::SeededRng;
use std::collections::BTreeMap;

/// One halo node: a collapsed cluster/group analog.
#[derive(Clone, Debug, PartialEq)]
pub struct WebNode {
    /// Index into the parent descriptor's node list (fly-to + inspector key).
    pub node_index: u32,
    /// Comoving Mpc relative to the descriptor center (f64: flight truth).
    /// Bounded by `descriptor_radius_mpc`: the sphere is a generation cut
    /// (ADR-026 §1, v0.3.4 `cosmic-sphere-clip`), so every emitted node
    /// satisfies `|position_mpc| ≤ radius`.
    pub position_mpc: [f64; 3],
    /// Halo mass in solar masses (Press–Schechter rank-mapped).
    pub mass_msun: f64,
    /// Virial radius in Mpc (M = 200·ρc·4/3πr³).
    pub virial_radius_mpc: f64,
}

/// One filament link between two nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct WebLink {
    /// Endpoint node indices, canonical: `a < b`.
    pub a: u32,
    /// Endpoint node indices, canonical: `a < b`.
    pub b: u32,
    /// Normalized link density in [0, 1] (segment mean over peak cut).
    pub density: f32,
}

/// Stage-0 output: the generated cosmic web.
#[derive(Clone, Debug, PartialEq)]
pub struct WebDescriptor {
    /// Runtime seed this web generates from.
    pub seed: u64,
    /// [`UNIVERSE_VERSION`](super::super::UNIVERSE_VERSION) at generation
    /// time — the stage-0 addition is what bumped it to 2.
    pub universe_version: u32,
    /// Halo nodes, densest-peak-first.
    pub nodes: Vec<WebNode>,
    /// Filament links, canonical `(a, b)` order.
    pub links: Vec<WebLink>,
    /// Dwarf glow points along links, Mpc relative to center (f32:
    /// visual data; relative precision at 250 Mpc is ~0.03 pc).
    pub glow_mpc: Vec<[f32; 3]>,
    /// Index of the home node (Local-Group analog hosting the Milky Way).
    pub home_node: u32,
    /// Fraction of lattice cells classified void (diagnostic summary for
    /// the inspector dock). Deliberately EXCLUDED from [`web_hash`](super::super::hash::web_hash):
    /// it derives deterministically from the same pipeline, so identical
    /// content implies an identical fraction — hashing it would only
    /// re-roll vectors without adding identity.
    pub void_fraction: f64,
}

impl WebDescriptor {
    /// Stamp a generator-produced web with identity + version.
    pub fn new(
        seed: u64,
        nodes: Vec<WebNode>,
        links: Vec<WebLink>,
        glow_mpc: Vec<[f32; 3]>,
        home_node: u32,
        void_fraction: f64,
    ) -> Self {
        Self {
            seed,
            universe_version: super::super::UNIVERSE_VERSION,
            nodes,
            links,
            glow_mpc,
            home_node,
            void_fraction,
        }
    }

    /// The home node (Local-Group analog).
    pub fn home(&self) -> &WebNode {
        &self.nodes[self.home_node as usize]
    }

    /// Strongest link out of `node`: maximum density × length (the most
    /// luminous departure path — spawn and fly-to reference it).
    pub fn strongest_link_from(&self, node: u32) -> Option<&WebLink> {
        let pos = |i: u32| self.nodes[i as usize].position_mpc;
        self.links
            .iter()
            .filter(|l| l.a == node || l.b == node)
            .max_by(|l, m| {
                let len = |l: &WebLink| {
                    let a = pos(l.a);
                    let b = pos(l.b);
                    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
                        * f64::from(l.density)
                };
                len(l).total_cmp(&len(m))
            })
    }

    /// Stable content ID for saves and links (`docs/game/universe.md`
    /// rule: stable strings, safe to store).
    pub fn node_content_id(&self, node: u32) -> String {
        format!("web/seed:{}/node:{node}", self.seed)
    }
}

/// Parabolic sub-cell peak refinement on the Eulerian density (basic ops;
///
/// offset clamped to ±0.5 cell). Named `peak_refine` so the CGT-010
/// retirement pin stays literally clean — this stage-C helper is
/// unrelated to the retired export method and untouched.
fn peak_refine(pos: [f64; 3], euler: &EulerianField, cell: [usize; 3], n: usize) -> [f64; 3] {
    let mut out = pos;
    for axis in 0..3 {
        let mut c = cell;
        let f0 = euler.density[index(n, c[0], c[1], c[2])];
        c[axis] = (c[axis] + 1).min(n - 1);
        let fp = euler.density[index(n, c[0], c[1], c[2])];
        let mut c2 = cell;
        c2[axis] = c2[axis].saturating_sub(1);
        let fm = euler.density[index(n, c2[0], c2[1], c2[2])];
        let denom = fm - 2.0 * f0 + fp;
        let offset = if denom != 0.0 {
            ((fm - fp) * 0.5 / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        out[axis] += offset;
    }
    out
}

/// NGP Eulerian lookup at an Mpc position (relative to center).
fn density_at(euler: &EulerianField, params: &CosmicWebParams, p: [f64; 3]) -> f64 {
    let n = params.lattice_cells as usize;
    let half = params.half_width_mpc();
    let cell = |v: f64| {
        ((v + half) / params.cell_size_mpc)
            .floor()
            .clamp(0.0, n as f64 - 1.0) as usize
    };
    euler.density[index(n, cell(p[0]), cell(p[1]), cell(p[2]))]
}

/// Irwin–Hall-3: sum of 3 uniforms, centered, σ = 0.5 (pure arithmetic).
fn ihalf3(rng: &mut SeededRng) -> f64 {
    rng.unit_f64() + rng.unit_f64() + rng.unit_f64() - 1.5
}

/// Assemble the descriptor from the classified web (densest-first
/// peaks inside the descriptor sphere — ADR-026 §1: the sphere is a
/// generation cut, so peaks whose refined position lies outside
/// `descriptor_radius_mpc` are rejected before greedy acceptance).
#[allow(clippy::too_many_lines)]
pub fn assemble(
    seed: u64,
    euler: &EulerianField,
    classified: &ClassifiedWeb,
    params: &CosmicWebParams,
) -> WebDescriptor {
    let peaks = &classified.peaks;
    let n = params.lattice_cells as usize;
    let half = params.half_width_mpc();
    let to_mpc = |cell: usize| -> [f64; 3] {
        let x = cell % n;
        let y = (cell / n) % n;
        let z = cell / (n * n);
        let base = [
            (x as f64 + 0.5) * params.cell_size_mpc - half,
            (y as f64 + 0.5) * params.cell_size_mpc - half,
            (z as f64 + 0.5) * params.cell_size_mpc - half,
        ];
        peak_refine(base, euler, [x, y, z], n)
    };
    // Node acceptance (densest-first, greedy separation). ADR-026 §1:
    // the descriptor sphere is a generation cut — reject refined peak
    // positions with `r² > R²` (f64, no sqrt) before acceptance so the
    // `target_node_count` cap applies to in-sphere peaks only.
    let radius = params.descriptor_radius_mpc;
    let radius2 = radius * radius;
    let in_sphere: Vec<_> = peaks
        .iter()
        .copied()
        .filter(|peak| {
            let p = to_mpc(peak.cell);
            p[0] * p[0] + p[1] * p[1] + p[2] * p[2] <= radius2
        })
        .collect();
    let accepted = accept_peaks(&in_sphere, params, to_mpc);
    // Masses by rank through the Press–Schechter quantile table.
    let table = PsMassTable::new(params.mass_min_msun / params.mass_star_msun, 40.0, 4096);
    let count = accepted.len();
    let mut positioned: Vec<([f64; 3], f64)> = Vec::with_capacity(count);
    for (rank, cell) in accepted.iter().enumerate() {
        let u = 1.0 - (rank as f64 + 0.5) / count.max(1) as f64;
        let mass = params.mass_star_msun * table.quantile(u);
        positioned.push((to_mpc(*cell), mass));
    }
    // Home: group-band node nearest the center, else nearest overall.
    let dist2 = |p: [f64; 3]| p[0] * p[0] + p[1] * p[1] + p[2] * p[2];
    let mut home = 0_usize;
    let mut home_d2 = f64::INFINITY;
    for (i, (pos, mass)) in positioned.iter().enumerate() {
        if *mass >= params.home_mass_lo_msun && *mass <= params.home_mass_hi_msun {
            let d2 = dist2(*pos);
            if d2 < home_d2 {
                home_d2 = d2;
                home = i;
            }
        }
    }
    if home_d2.is_infinite() {
        for (i, (pos, _)) in positioned.iter().enumerate() {
            let d2 = dist2(*pos);
            if d2 < home_d2 {
                home_d2 = d2;
                home = i;
            }
        }
    }
    let nodes: Vec<WebNode> = positioned
        .iter()
        .enumerate()
        .map(|(i, (pos, mass))| WebNode {
            node_index: i as u32,
            position_mpc: *pos,
            mass_msun: *mass,
            virial_radius_mpc: virial_radius_mpc(*mass),
        })
        .collect();
    assert!(
        !nodes.is_empty(),
        "stage C produced no acceptable peaks — check calibration (peak cut, threshold, growth)"
    );
    // Links: spatial grid over nodes, canonical (a, b) pairs, segment
    // density cut, ≤ 8 shortest passing links per node.
    let links = build_links(&nodes, euler, params);
    // Glow: exact two-pass budget in canonical link order.
    let glow_mpc = emit_glow(seed, &nodes, &links, params);
    WebDescriptor::new(
        seed,
        nodes,
        links,
        glow_mpc,
        home as u32,
        classified.void_fraction,
    )
}

/// Candidate link with its Euclidean length (link building only).
struct Candidate {
    a: u32,
    b: u32,
    length: f64,
    density: f32,
}

fn build_links(nodes: &[WebNode], euler: &EulerianField, params: &CosmicWebParams) -> Vec<WebLink> {
    let link_len = params.linking_length_mpc;
    let cut = params.link_density_ratio * euler.mean;
    let mut grid: BTreeMap<[i64; 3], Vec<u32>> = BTreeMap::new();
    for node in nodes {
        let p = node.position_mpc;
        grid.entry([
            (p[0] / link_len).floor() as i64,
            (p[1] / link_len).floor() as i64,
            (p[2] / link_len).floor() as i64,
        ])
        .or_default()
        .push(node.node_index);
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    for node in nodes {
        let p = node.position_mpc;
        let k = [
            (p[0] / link_len).floor() as i64,
            (p[1] / link_len).floor() as i64,
            (p[2] / link_len).floor() as i64,
        ];
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(cell) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for other in cell {
                            if *other <= node.node_index {
                                continue;
                            }
                            let q = nodes[*other as usize].position_mpc;
                            let len = ((q[0] - p[0]).powi(2)
                                + (q[1] - p[1]).powi(2)
                                + (q[2] - p[2]).powi(2))
                            .sqrt();
                            if len > link_len || len == 0.0 {
                                continue;
                            }
                            // Segment density: 8 NGP samples.
                            let mut sum = 0.0;
                            for s in 0..8 {
                                let t = (s as f64 + 0.5) / 8.0;
                                sum += density_at(
                                    euler,
                                    params,
                                    [
                                        p[0] * (1.0 - t) + q[0] * t,
                                        p[1] * (1.0 - t) + q[1] * t,
                                        p[2] * (1.0 - t) + q[2] * t,
                                    ],
                                );
                            }
                            let mean_seg = sum / 8.0;
                            if mean_seg >= cut {
                                let density = (mean_seg / (params.peak_density_ratio * euler.mean))
                                    .clamp(0.0, 1.0)
                                    as f32;
                                candidates.push(Candidate {
                                    a: node.node_index,
                                    b: *other,
                                    length: len,
                                    density,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    // ≤ 8 shortest passing links per node; canonical output order.
    candidates.sort_by(|x, y| {
        x.length
            .total_cmp(&y.length)
            .then_with(|| x.a.cmp(&y.a))
            .then_with(|| x.b.cmp(&y.b))
    });
    let mut per_node = vec![0_u32; nodes.len()];
    let mut links: Vec<WebLink> = Vec::new();
    for c in candidates {
        if per_node[c.a as usize] < 8 && per_node[c.b as usize] < 8 {
            per_node[c.a as usize] += 1;
            per_node[c.b as usize] += 1;
            links.push(WebLink {
                a: c.a,
                b: c.b,
                density: c.density,
            });
        }
    }
    links.sort_by(|x, y| x.a.cmp(&y.a).then_with(|| x.b.cmp(&y.b)));
    links
}

fn emit_glow(
    seed: u64,
    nodes: &[WebNode],
    links: &[WebLink],
    params: &CosmicWebParams,
) -> Vec<[f32; 3]> {
    // Pass 1: raw counts in canonical link order.
    let pos = |i: u32| nodes[i as usize].position_mpc;
    let mut raw: Vec<f64> = Vec::with_capacity(links.len());
    let mut total_raw = 0.0;
    for link in links {
        let a = pos(link.a);
        let b = pos(link.b);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        let count = params.glow_per_mpc * len * f64::from(link.density);
        raw.push(count);
        total_raw += count;
    }
    let scale = if total_raw > 0.0 {
        (params.max_glow_points as f64 / total_raw).min(1.0)
    } else {
        0.0
    };
    // Pass 2: budgeted emission with deterministic fractional rounding.
    let mut rng = SeededRng::stream(seed, "cosmic_web/glow");
    let mut glow: Vec<[f32; 3]> = Vec::new();
    for (link, count) in links.iter().zip(raw.iter()) {
        let scaled = count * scale;
        let mut emit = scaled.floor() as u64;
        let frac = scaled - scaled.floor();
        if rng.below(1000) < (frac * 1000.0) as u64 {
            emit += 1;
        }
        if emit == 0 {
            // Still advance nothing — zero emission draws nothing.
            continue;
        }
        let a = pos(link.a);
        let b = pos(link.b);
        let mut d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
            .sqrt()
            .max(f64::MIN_POSITIVE);
        d = [d[0] / len, d[1] / len, d[2] / len];
        let mut reference = [0.0, 1.0, 0.0];
        if (d[0] * reference[0] + d[1] * reference[1] + d[2] * reference[2]).abs() > 0.9 {
            reference = [1.0, 0.0, 0.0];
        }
        // u = normalize(cross(d, reference)); v = cross(d, u).
        let mut u = [
            d[1] * reference[2] - d[2] * reference[1],
            d[2] * reference[0] - d[0] * reference[2],
            d[0] * reference[1] - d[1] * reference[0],
        ];
        let ulen = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2])
            .sqrt()
            .max(f64::MIN_POSITIVE);
        u = [u[0] / ulen, u[1] / ulen, u[2] / ulen];
        let v = [
            d[1] * u[2] - d[2] * u[1],
            d[2] * u[0] - d[0] * u[2],
            d[0] * u[1] - d[1] * u[0],
        ];
        let sigma = params.glow_transverse_sigma_mpc;
        for _ in 0..emit {
            let t = rng.unit_f64();
            let j1 = ihalf3(&mut rng) * 2.0 * sigma;
            let j2 = ihalf3(&mut rng) * 2.0 * sigma;
            glow.push([
                (a[0] * (1.0 - t) + b[0] * t + u[0] * j1 + v[0] * j2) as f32,
                (a[1] * (1.0 - t) + b[1] * t + u[1] * j1 + v[1] * j2) as f32,
                (a[2] * (1.0 - t) + b[2] * t + u[2] * j1 + v[2] * j2) as f32,
            ]);
        }
    }
    // Hard cap, deterministic: emission order is canonical link order,
    // so truncation drops glow points only from the highest-(a, b) links.
    // Overshoot past the expected value is sub-0.1% by construction
    // (scale bounds the mean; per-link variance ≤ 1/4).
    glow.truncate(params.max_glow_points as usize);
    glow
}

#[cfg(test)]
mod tests {
    use super::super::super::UNIVERSE_VERSION;
    use super::*;

    fn sample_web() -> WebDescriptor {
        let nodes = vec![
            WebNode {
                node_index: 0,
                position_mpc: [0.0, 0.0, 0.0],
                mass_msun: 1.0e14,
                virial_radius_mpc: 1.0,
            },
            WebNode {
                node_index: 1,
                position_mpc: [30.0, 0.0, 0.0],
                mass_msun: 2.0e12,
                virial_radius_mpc: 0.3,
            },
        ];
        let links = vec![WebLink {
            a: 0,
            b: 1,
            density: 0.8,
        }];
        WebDescriptor::new(7, nodes, links, Vec::new(), 1, 0.75)
    }

    #[test]
    fn constructor_stamps_version_and_home() {
        let web = sample_web();
        assert_eq!(web.universe_version, UNIVERSE_VERSION);
        assert_eq!(web.home_node, 1);
        assert_eq!(web.home().mass_msun, 2.0e12);
        assert_eq!(web.void_fraction, 0.75);
    }

    #[test]
    fn content_ids_are_stable_strings() {
        let web = sample_web();
        assert_eq!(web.node_content_id(0), "web/seed:7/node:0");
        assert_eq!(web.node_content_id(1), "web/seed:7/node:1");
    }

    #[test]
    fn strongest_link_picks_density_times_length() {
        let web = sample_web();
        let link = web.strongest_link_from(0).expect("link exists");
        assert_eq!((link.a, link.b), (0, 1));
        assert!(web.strongest_link_from(1).is_some());
    }
}
