//! Cosmic Web dimension tab: read-only inspector over the shared stage-0
//! descriptor (WS5 — the fourth absorbed view).
//!
//! [`CosmicWebInspector`] owns an orbit/pan/zoom camera (Mpc units,
//! `MapOrbitCamera` policy) plus a node selection. It never mutates
//! journey, transit, or viewer state — selection is local readout state
//! (pinned by `selection_is_read_only` in `app.rs`).
//!
//! This module also owns the CPU-side vertex layout both 3D surfaces
//! share ([`node_point_cloud`], [`link_segments`], [`glow_point_cloud`]
//! plus the cinematic enrichment layer [`braid_segments`],
//! [`grain_cloud`], [`node_impostors`]): one source of truth for
//! positions (origin-relative Mpc f32), colors, and sprite sizes. The
//! binary maps the tuples onto its GPU vertex types at upload.
//!
//! The enrichment layer is render-only: every value derives
//! deterministically from the descriptor + seed (never fed back into
//! selection, flight, or saves), so the stage-0 descriptor, its hash,
//! and `UNIVERSE_VERSION` are untouched.

use super::map_camera::{DEFAULT_PITCH, DEFAULT_YAW, MapOrbitCamera};
use super::picking::project_to_screen;
use super::ui::Rect;
use game_engine::core::SeededRng;
use game_engine::universe::{WebDescriptor, WebLink};
use glam::{DVec3, Mat4, Vec3};

/// Click-selection radius in px (the map-tab precedent).
pub const COSMIC_PICK_RADIUS_PX: f32 = 8.0;

/// Inspector default eye distance: the 500 Mpc sphere framed with
/// margin under the shared 60° FOV.
pub const INSPECTOR_DISTANCE_MPC: f32 = 430.0;
/// Inspector zoom band, Mpc.
pub const INSPECTOR_MIN_DISTANCE_MPC: f32 = 10.0;
/// Inspector zoom band, Mpc.
pub const INSPECTOR_MAX_DISTANCE_MPC: f32 = 800.0;

/// Read-only inspector state for the Cosmic Web dimension tab.
pub struct CosmicWebInspector {
    /// Orbit/pan/zoom camera in Mpc (inspector owns its view; the demo
    /// player camera is untouched).
    pub camera: MapOrbitCamera,
    /// Selected node index (readout only — never fed back anywhere).
    pub selected: Option<u32>,
}

impl CosmicWebInspector {
    /// Fresh inspector: whole-web framing, nothing selected.
    pub fn new() -> Self {
        Self {
            camera: MapOrbitCamera::new(
                Vec3::ZERO,
                INSPECTOR_DISTANCE_MPC,
                DEFAULT_YAW,
                DEFAULT_PITCH,
                INSPECTOR_MIN_DISTANCE_MPC,
                INSPECTOR_MAX_DISTANCE_MPC,
                250.0,
            ),
            selected: None,
        }
    }

    /// Click-select: project every node through the current
    /// view-projection, keep the nearest projected point within
    /// [`COSMIC_PICK_RADIUS_PX`]. Exact projected-distance ties keep the
    /// lowest node index (the map-tab rule). Stores and returns the
    /// pick. Read-only: touches nothing but `self.selected`.
    pub fn select_at(
        &mut self,
        web: &WebDescriptor,
        origin: DVec3,
        cursor: (f32, f32),
        vp: Rect,
    ) -> Option<u32> {
        let view_proj = self.camera.view_proj(vp.w / vp.h);
        let mut best: Option<(u32, f32)> = None;
        for node in &web.nodes {
            let world = Vec3::new(
                (node.position_mpc[0] - origin.x) as f32,
                (node.position_mpc[1] - origin.y) as f32,
                (node.position_mpc[2] - origin.z) as f32,
            );
            if let Some((sx, sy)) = project_to_screen(world, view_proj, vp) {
                let d = (sx - cursor.0).hypot(sy - cursor.1);
                if d <= COSMIC_PICK_RADIUS_PX
                    && best.is_none_or(|(bi, bd)| d < bd || (d == bd && node.node_index < bi))
                {
                    best = Some((node.node_index, d));
                }
            }
        }
        self.selected = best.map(|(i, _)| i);
        self.selected
    }

    /// Combined matrix the renderer pushes (test seam).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.camera.view_proj(aspect)
    }
}

impl Default for CosmicWebInspector {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared mass→[0,1] grading level for every node visual (color,
/// size, emissive): 1e12 M☉ → 0, ~3e14 M☉ → 1. Re-centered in
/// update-2026-09-19-1933 (was 1e12.7–1e15.4): the target reference
/// reads golden at ordinary cluster hubs (~1e14), not only at the
/// rarest giants, so the warm ramp must start earlier.
fn mass_level(mass_msun: f64) -> f32 {
    ((mass_msun.log10() - 12.0) / 2.5).clamp(0.0, 1.0) as f32
}

/// Mass-graded node tint: blue-white dwarfs → golden giants (the
/// gold end deepened in update-2026-09-19-1933 so massive hubs read
/// like the target reference, not pale yellow).
/// Shared by the demo and inspector uploads (one palette, two surfaces).
pub fn node_color(mass_msun: f64) -> [f32; 3] {
    let l = mass_level(mass_msun);
    [0.60 + 0.40 * l, 0.68 + 0.20 * l, 1.00 - 0.38 * l]
}

/// Node pixel size from mass (2–5 px sprite floor for legibility).
pub fn node_size_px(mass_msun: f64) -> f32 {
    2.0 + 3.0 * mass_level(mass_msun)
}

/// Halo nodes as `(position, color, misc)` tuples in origin-relative
/// Mpc f32: `misc = (pixel size, alpha, kind 0)`.
pub fn node_point_cloud(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    web.nodes
        .iter()
        .map(|node| {
            (
                [
                    (node.position_mpc[0] - origin.x) as f32,
                    (node.position_mpc[1] - origin.y) as f32,
                    (node.position_mpc[2] - origin.z) as f32,
                ],
                node_color(node.mass_msun),
                [node_size_px(node.mass_msun), 1.0, 0.0],
            )
        })
        .collect()
}

/// Dwarf glow points as tuples (`misc = (1.5 px, 0.09 alpha, kind
/// 0)` — dim filament body; the additive chain saturates fast, so
/// alphas stay small by design and were lowered further in
/// update-2026-09-19-1933 so sparse regions let voids read dark).
pub fn glow_point_cloud(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    web.glow_mpc
        .iter()
        .map(|g| {
            (
                [
                    (f64::from(g[0]) - origin.x) as f32,
                    (f64::from(g[1]) - origin.y) as f32,
                    (f64::from(g[2]) - origin.z) as f32,
                ],
                [0.55, 0.54, 0.90],
                [1.5, 0.09, 0.0],
            )
        })
        .collect()
}

/// Filament links as flattened endpoint pairs (origin-relative Mpc f32)
/// for the `LineList` pipeline.
pub fn link_segments(web: &WebDescriptor, origin: DVec3) -> Vec<[f32; 3]> {
    let pos = |i: u32| {
        let p = web.nodes[i as usize].position_mpc;
        [
            (p[0] - origin.x) as f32,
            (p[1] - origin.y) as f32,
            (p[2] - origin.z) as f32,
        ]
    };
    let mut out = Vec::with_capacity(web.links.len() * 2);
    for link in &web.links {
        out.push(pos(link.a));
        out.push(pos(link.b));
    }
    out
}

// ---------------------------------------------------------------------------
// Cinematic enrichment layer (update-2026-09-18-2328): braided filaments,
// particulate grain, emissive node impostors. Render-only derivations of
// the descriptor — deterministic per (seed, web, origin), never hashed.
// ---------------------------------------------------------------------------

/// Subdivisions per link per braid strand (segments = subdivisions).
pub const BRAID_SUBDIVISIONS: usize = 10;
/// Braid lateral amplitude in Mpc (strand + wander combined stay under
/// ~1.6× this; tests pin the bound).
pub const BRAID_AMPLITUDE_MPC: f64 = 1.5;
/// Strands for a zero-density link; a full-density link gets three
/// (`1 + floor(2·density)`).
pub const BRAID_MIN_STRANDS: u64 = 1;
/// Strands for a full-density link.
pub const BRAID_MAX_STRANDS: u64 = 3;
/// Visual grain points emitted per Mpc of link (pre-budget, density
/// weighted — the same two-pass budget pattern as descriptor glow).
pub const GRAIN_PER_MPC: f64 = 8.0;
/// Hard cap on emitted grain points (boot + rebase cost control).
pub const MAX_GRAIN_POINTS: u32 = 800_000;
/// Transverse jitter sigma of grain around its strand, Mpc.
pub const GRAIN_TRANSVERSE_SIGMA_MPC: f64 = 0.8;
/// Domain-separated stream for braid phases (order-independent from
/// the grain stream: both consume in canonical link order).
pub const BRAID_STREAM: &str = "cosmic_web/braid";
/// Domain-separated stream for grain emission.
pub const GRAIN_STREAM: &str = "cosmic_web/grain";
/// Exaggerated Hubble redshift strength per Mpc of view depth for the
/// cosmic glow shaders (spec §9.1 depth cue, artistically boosted: at
/// 250 Mpc the exaggerated depth is 0.5 — distant filaments redden
/// gently). Halved in update-2026-09-19-1933 (was 0.004, saturating
/// at 125 Mpc): the softer ramp keeps the depth cue readable while
/// letting golden hubs survive at depth. Single tuning knob, shared
/// by both cosmic surfaces.
pub const COSMIC_REDSHIFT_PER_MPC: f32 = 0.002;

/// Orthonormal-adjacent lateral basis for a link direction (the glow
/// precedent: `u = normalize(cross(d, reference))`, `v = cross(d, u)`).
fn lateral_basis(d: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let mut reference = [0.0, 1.0, 0.0];
    if (d[0] * reference[0] + d[1] * reference[1] + d[2] * reference[2]).abs() > 0.9 {
        reference = [1.0, 0.0, 0.0];
    }
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
    (u, v)
}

/// One braid strand's deterministic shape parameters (drawn from the
/// braid stream in canonical link order — replay-safe because the draw
/// count is a pure function of link density).
struct BraidStrand {
    phase: f64,
    windings: f64,
    mix_u: f64,
    mix_v: f64,
}

/// Shared low-frequency trunk wander per link (all strands of the link
/// braid around the same wandering center).
struct BraidWander {
    phase: f64,
    amplitude: f64,
}

/// Point on a strand at parameter `t ∈ [0, 1]`: trunk center + shared
/// wander + the strand's own twist, all tapered to zero at the nodes
/// so strands melt into the cluster hubs.
fn braid_point(
    a: [f64; 3],
    d: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    wander: &BraidWander,
    strand: &BraidStrand,
    t: f64,
) -> [f64; 3] {
    const TAU: f64 = std::f64::consts::TAU;
    let taper = (std::f64::consts::PI * t).sin();
    let wobble_u = (TAU * t + wander.phase).sin() * wander.amplitude * taper;
    let wobble_v = (TAU * t * 0.7 + wander.phase).cos() * wander.amplitude * taper;
    let angle = TAU * strand.windings * t + strand.phase;
    let off_u = (wobble_u + angle.sin() * strand.mix_u * BRAID_AMPLITUDE_MPC) * taper;
    let off_v = (wobble_v + angle.cos() * strand.mix_v * BRAID_AMPLITUDE_MPC) * taper;
    [
        a[0] + d[0] * t + u[0] * off_u + v[0] * off_v,
        a[1] + d[1] * t + u[1] * off_u + v[1] * off_v,
        a[2] + d[2] * t + u[2] * off_u + v[2] * off_v,
    ]
}

/// One link's full braid shape: lateral basis, shared trunk wander,
/// and the strand parameters. Derived from a per-link sub-stream keyed
/// by the canonical endpoint pair (`seed ^ (a << 32 | b)`), so the line
/// pass ([`braid_segments`]) and the grain pass ([`grain_cloud`])
/// derive identical strands independently — no shared stream state,
/// no cross-link coupling, replay-identical per (seed, link).
struct BraidShape {
    u: [f64; 3],
    v: [f64; 3],
    wander: BraidWander,
    strands: Vec<BraidStrand>,
}

fn braid_shape(seed: u64, link: &WebLink, pa: [f64; 3], pb: [f64; 3]) -> BraidShape {
    const TAU: f64 = std::f64::consts::TAU;
    let mut rng = SeededRng::stream(
        seed ^ ((u64::from(link.a) << 32) | u64::from(link.b)),
        BRAID_STREAM,
    );
    let raw = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
    let len = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2])
        .sqrt()
        .max(f64::MIN_POSITIVE);
    let dir = [raw[0] / len, raw[1] / len, raw[2] / len];
    let (u, v) = lateral_basis(dir);
    let strands =
        (BRAID_MIN_STRANDS + (f64::from(link.density) * 2.0).floor() as u64).min(BRAID_MAX_STRANDS);
    let wander = BraidWander {
        phase: rng.unit_f64() * TAU,
        amplitude: (0.3 + 0.7 * rng.unit_f64()) * BRAID_AMPLITUDE_MPC * 0.6,
    };
    let mut strand_params = Vec::with_capacity(strands as usize);
    for _ in 0..strands {
        strand_params.push(BraidStrand {
            phase: rng.unit_f64() * TAU,
            windings: 1.0 + rng.unit_f64(),
            mix_u: 0.6 + 0.4 * rng.unit_f64(),
            mix_v: 0.6 + 0.4 * rng.unit_f64(),
        });
    }
    BraidShape {
        u,
        v,
        wander,
        strands: strand_params,
    }
}

/// Braided filament strands as flattened `(position, rgba)` endpoint
/// pairs (origin-relative Mpc f32) for an additive colored-`LineList`
/// pipeline. Strand count follows link density (1–3); color grades
/// from dim indigo (faint) to bright cyan-violet (dense) with alpha
/// melting into the endpoint nodes.
pub fn braid_segments(web: &WebDescriptor, seed: u64, origin: DVec3) -> Vec<([f32; 3], [f32; 4])> {
    let mut out = Vec::new();
    for link in &web.links {
        let pa = web.nodes[link.a as usize].position_mpc;
        let pb = web.nodes[link.b as usize].position_mpc;
        let raw = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let shape = braid_shape(seed, link, pa, pb);
        let density = f64::from(link.density);
        // Filament palette (update-2026-09-19-1933): dim indigo →
        // bright blue-violet by density. Dense strands premultiply to
        // ~1.7 in blue — well past the bloom threshold (1.0), because
        // the blur chain keeps only ~1/4 of a 1-px line's over-
        // threshold energy: crossings and dense links bloom hard,
        // mid-density stays crisp, faint links sink to backdrop level
        // so voids read dark. Bands pinned by the enrichment tests:
        // rgb ≤ 2.0, monotonic in density.
        let rgb = [
            0.18 + 0.30 * density,
            0.22 + 0.35 * density,
            0.60 + 1.35 * density,
        ];
        // Alpha floor near zero: dozens of strands cross per pixel and
        // the additive chain saturates fast, so low-density links must
        // start almost invisible or voids never darken. The high
        // ceiling is what pushes dense strands past the bloom
        // threshold (premult = rgb × alpha = 1.95 in blue at d = 1).
        let alpha = 0.05 + 0.95 * density;
        for strand in &shape.strands {
            for s in 0..BRAID_SUBDIVISIONS {
                for end in [s, s + 1] {
                    let t = end as f64 / BRAID_SUBDIVISIONS as f64;
                    let p = braid_point(pa, raw, shape.u, shape.v, &shape.wander, strand, t);
                    let melt = (std::f64::consts::PI * t).sin().sqrt().max(0.0);
                    out.push((
                        [
                            (p[0] - origin.x) as f32,
                            (p[1] - origin.y) as f32,
                            (p[2] - origin.z) as f32,
                        ],
                        [
                            rgb[0] as f32,
                            rgb[1] as f32,
                            rgb[2] as f32,
                            (alpha * melt) as f32,
                        ],
                    ));
                }
            }
        }
    }
    out
}

/// Irwin–Hall-3 jitter, σ = 0.5 (the descriptor-glow precedent: pure
/// arithmetic shaping, no transcendentals in the sampling).
fn ihalf3(rng: &mut SeededRng) -> f64 {
    rng.unit_f64() + rng.unit_f64() + rng.unit_f64() - 1.5
}

/// Particulate grain along the braid strands as `(position, color,
/// misc)` tuples — the same shape as [`node_point_cloud`] (`misc =
/// (pixel size, alpha, kind 0)`), colors up to slightly emissive on
/// dense links. Deterministic per (seed, web, origin) under the
/// [`GRAIN_STREAM`] domain; two-pass budget capped at
/// [`MAX_GRAIN_POINTS`] (the descriptor-glow pattern).
pub fn grain_cloud(
    web: &WebDescriptor,
    seed: u64,
    origin: DVec3,
) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    // Pass 1: raw counts in canonical link order (pure function of
    // link data — no RNG involved).
    let mut raw: Vec<f64> = Vec::with_capacity(web.links.len());
    let mut total_raw = 0.0;
    for link in &web.links {
        let pa = web.nodes[link.a as usize].position_mpc;
        let pb = web.nodes[link.b as usize].position_mpc;
        let len =
            ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2) + (pb[2] - pa[2]).powi(2)).sqrt();
        let count = GRAIN_PER_MPC * len * f64::from(link.density);
        raw.push(count);
        total_raw += count;
    }
    let scale = if total_raw > 0.0 {
        (f64::from(MAX_GRAIN_POINTS) / total_raw).min(1.0)
    } else {
        0.0
    };
    // Pass 2: budgeted emission. Each grain point lands on a strand of
    // the SAME braid shape the line pass draws (shared `braid_shape`
    // derivation), plus transverse jitter — the grain textures the
    // drawn strands instead of floating beside them.
    let mut rng = SeededRng::stream(seed, GRAIN_STREAM);
    let mut out: Vec<([f32; 3], [f32; 3], [f32; 3])> = Vec::new();
    for (link, count) in web.links.iter().zip(raw.iter()) {
        let scaled = count * scale;
        let mut emit = scaled.floor() as u64;
        let frac = scaled - scaled.floor();
        if rng.below(1000) < (frac * 1000.0) as u64 {
            emit += 1;
        }
        if emit == 0 {
            continue;
        }
        let pa = web.nodes[link.a as usize].position_mpc;
        let pb = web.nodes[link.b as usize].position_mpc;
        let raw_d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let shape = braid_shape(seed, link, pa, pb);
        let strands = shape.strands.len().max(1);
        let sigma = GRAIN_TRANSVERSE_SIGMA_MPC;
        for _ in 0..emit {
            let t = rng.unit_f64();
            let strand = &shape.strands[rng.below(strands as u64) as usize];
            let c = braid_point(pa, raw_d, shape.u, shape.v, &shape.wander, strand, t);
            let j1 = ihalf3(&mut rng) * 2.0 * sigma;
            let j2 = ihalf3(&mut rng) * 2.0 * sigma;
            let bright = 0.35 + 0.55 * rng.unit_f64();
            let size = 1.5 + rng.unit_f64();
            out.push((
                [
                    (c[0] + shape.u[0] * j1 + shape.v[0] * j2 - origin.x) as f32,
                    (c[1] + shape.u[1] * j1 + shape.v[1] * j2 - origin.y) as f32,
                    (c[2] + shape.u[2] * j1 + shape.v[2] * j2 - origin.z) as f32,
                ],
                [
                    (0.68 * bright) as f32,
                    (0.62 * bright) as f32,
                    (1.0 * bright) as f32,
                ],
                // Grain textures; it must not light the scene.
                [size as f32, 0.08, 0.0],
            ));
        }
    }
    out.truncate(MAX_GRAIN_POINTS as usize);
    out
}

/// Node impostors as `(position, color, misc)` tuples: two sprites per
/// node — an emissive hot core (channels > 1.0, mass-graded, the bloom
/// threshold's target; `kind` 0 = fixed pixel size) plus a soft
/// pale-cyan halo (`kind` 1 = world-unit diameter in Mpc, so it
/// shrinks with distance instead of plastering fixed-size quads over
/// the whole web — the visual-issue fix). Pure function of node
/// mass/position (no RNG): same node → same impostors, everywhere.
pub fn node_impostors(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    let mut out = Vec::with_capacity(web.nodes.len() * 2);
    for node in &web.nodes {
        let l = mass_level(node.mass_msun);
        let pos = [
            (node.position_mpc[0] - origin.x) as f32,
            (node.position_mpc[1] - origin.y) as f32,
            (node.position_mpc[2] - origin.z) as f32,
        ];
        let base = node_color(node.mass_msun);
        // Emissive enough to cross the bloom threshold (1.0) without
        // flooding the additive chain. Mass-stratified and pushed hard
        // (update-2026-09-19-1933): giants reach the 5.0 test band max
        // with big fixed-px cores, because the half-res blur chain
        // dilutes point sources ~1/(2πσ²) — only large, very bright
        // cores survive the chain as visible golden blooms (max
        // channel 1.0 × 5.0 = 5.0 band).
        let emissive = 1.5 + 3.5 * l;
        // Hot core: near-white, emissive, fixed pixel size (3–12 px —
        // big enough for the bloom chain to keep a visible halo).
        out.push((
            pos,
            [base[0] * emissive, base[1] * emissive, base[2] * emissive],
            [3.0 + 9.0 * l, 1.0, 0.0],
        ));
        // Halo: large, faint, world-sized (Mpc diameter). Cool cyan
        // for dwarfs warming toward gold for giants
        // (update-2026-09-19-1933); alpha lifts with mass so cluster
        // hubs glow wider. Diameter stays in the pinned 2–8 Mpc band.
        out.push((
            pos,
            [
                (0.55 + 0.30 * l) * (0.5 + 0.5 * l),
                (0.72 + 0.10 * l) * (0.5 + 0.5 * l),
                (1.00 - 0.25 * l) * (0.5 + 0.5 * l),
            ],
            [3.0 + 5.0 * l, 0.14 + 0.10 * l, 1.0],
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{CosmicWebParams, WebLink, WebNode, generate_cosmic_web};

    fn web() -> WebDescriptor {
        generate_cosmic_web(1234, &CosmicWebParams::nominal())
    }

    /// Two-link toy web: a dense 30 Mpc link (3 strands) and a faint
    /// 40 Mpc link (1 strand), with light / heavy / mid nodes.
    fn toy_web() -> WebDescriptor {
        let nodes = vec![
            WebNode {
                node_index: 0,
                position_mpc: [0.0, 0.0, 0.0],
                mass_msun: 1.0e15,
                virial_radius_mpc: 2.0,
            },
            WebNode {
                node_index: 1,
                position_mpc: [30.0, 0.0, 0.0],
                mass_msun: 5.0e12,
                virial_radius_mpc: 0.3,
            },
            WebNode {
                node_index: 2,
                position_mpc: [0.0, 40.0, 0.0],
                mass_msun: 3.0e14,
                virial_radius_mpc: 1.2,
            },
        ];
        let links = vec![
            WebLink {
                a: 0,
                b: 1,
                density: 1.0,
            },
            WebLink {
                a: 0,
                b: 2,
                density: 0.1,
            },
        ];
        WebDescriptor::new(7, nodes, links, Vec::new(), 0, 0.75)
    }

    fn vp() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        }
    }

    #[test]
    fn inspector_opens_framed_and_unselected() {
        let inspector = CosmicWebInspector::new();
        assert_eq!(inspector.selected, None);
        assert!((inspector.camera.distance() - INSPECTOR_DISTANCE_MPC).abs() < 1e-5);
    }

    #[test]
    fn select_at_picks_the_projected_node() {
        let web = web();
        let mut inspector = CosmicWebInspector::new();
        // Click exactly on the home node's projection: some node wins,
        // and re-clicking off-sky clears nothing but may reselect.
        let home = web.home();
        let world = Vec3::new(
            home.position_mpc[0] as f32,
            home.position_mpc[1] as f32,
            home.position_mpc[2] as f32,
        );
        let view_proj = inspector.view_proj(800.0 / 600.0);
        let (sx, sy) = project_to_screen(world, view_proj, vp()).expect("home node must project");
        let picked = inspector.select_at(&web, DVec3::ZERO, (sx, sy), vp());
        assert!(picked.is_some(), "click on a node must select");
        let idx = picked.expect("checked above");
        assert!(idx < web.nodes.len() as u32);
    }

    #[test]
    fn select_at_empty_web_selects_nothing() {
        let web = WebDescriptor::new(7, Vec::new(), Vec::new(), Vec::new(), 0, 0.0);
        let mut inspector = CosmicWebInspector::new();
        assert_eq!(
            inspector.select_at(&web, DVec3::ZERO, (400.0, 300.0), vp()),
            None
        );
        assert_eq!(inspector.selected, None);
    }

    #[test]
    fn clouds_cover_nodes_links_and_glow() {
        let web = web();
        let origin = DVec3::ZERO;
        assert_eq!(node_point_cloud(&web, origin).len(), web.nodes.len());
        assert_eq!(glow_point_cloud(&web, origin).len(), web.glow_mpc.len());
        assert_eq!(link_segments(&web, origin).len(), web.links.len() * 2);
        // Mass grading: the heaviest node is yellower and bigger than
        // the lightest.
        let mut by_mass = web.nodes.clone();
        by_mass.sort_by(|a, b| a.mass_msun.total_cmp(&b.mass_msun));
        let light = node_color(by_mass[0].mass_msun);
        let heavy = node_color(by_mass[by_mass.len() - 1].mass_msun);
        assert!(heavy[0] > light[0] && heavy[2] < light[2]);
        assert!(
            node_size_px(by_mass[by_mass.len() - 1].mass_msun) > node_size_px(by_mass[0].mass_msun)
        );
    }

    #[test]
    fn braid_strand_counts_follow_density() {
        // Dense link → 3 strands, faint link → 1 strand; each strand
        // emits 2 verts per subdivision.
        let braided = braid_segments(&toy_web(), 7, DVec3::ZERO);
        assert_eq!(braided.len(), (3 + 1) * 2 * BRAID_SUBDIVISIONS);
        // The dense link's strands actually leave the trunk: some
        // midpoint vert sits laterally off the segment.
        let dense: Vec<[f32; 3]> = braided
            .iter()
            .take(3 * 2 * BRAID_SUBDIVISIONS)
            .map(|v| v.0)
            .collect();
        let lateral = dense
            .iter()
            .map(|p| (p[1] * p[1] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max);
        assert!(
            lateral > 1e-6 && lateral <= 2.0 * BRAID_AMPLITUDE_MPC as f32,
            "braid must leave the trunk within its amplitude bound: {lateral}"
        );
    }

    #[test]
    fn braid_replays_identically_and_tapers_at_endpoints() {
        let web = toy_web();
        let a = braid_segments(&web, 7, DVec3::ZERO);
        let b = braid_segments(&web, 7, DVec3::ZERO);
        assert_eq!(a, b);
        // Endpoints melt into the nodes: the first vert of each strand
        // sits on node `a` (taper = 0 at t = 0).
        assert!((a[0].0[0]).abs() < 1e-3 && (a[0].0[1]).abs() < 1e-3 && (a[0].0[2]).abs() < 1e-3);
        // Every vert stays within the amplitude bound of its segment.
        let segs = [
            ([0.0, 0.0, 0.0], [30.0, 0.0, 0.0]),
            ([0.0, 0.0, 0.0], [0.0, 40.0, 0.0]),
        ];
        for (pos, _) in &a {
            let p = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
            let near = segs.iter().any(|(s, e)| {
                let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
                let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let t = ((p[0] - s[0]) * d[0] + (p[1] - s[1]) * d[1] + (p[2] - s[2]) * d[2]) / l2;
                let t = t.clamp(0.0, 1.0);
                let q = [s[0] + d[0] * t, s[1] + d[1] * t, s[2] + d[2] * t];
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                    <= 2.0 * BRAID_AMPLITUDE_MPC + 1e-3
            });
            assert!(near, "braid vert drifted off every segment: {p:?}");
        }
    }

    #[test]
    fn braid_colors_grade_with_density() {
        // Mean endpoint color of the dense link must beat the faint
        // link on every channel (same melt schedule both sides).
        let braided = braid_segments(&toy_web(), 7, DVec3::ZERO);
        let mean = |verts: &[([f32; 3], [f32; 4])]| {
            let mut acc = [0.0_f64; 4];
            for v in verts {
                for (channel, sum) in acc.iter_mut().enumerate() {
                    *sum += f64::from(v.1[channel]);
                }
            }
            let n = verts.len() as f64;
            [acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n]
        };
        let dense = mean(&braided[..3 * 2 * BRAID_SUBDIVISIONS]);
        let faint = mean(&braided[3 * 2 * BRAID_SUBDIVISIONS..]);
        for c in 0..4 {
            assert!(
                dense[c] > faint[c],
                "dense link must outshine faint on channel {c}: {dense:?} vs {faint:?}"
            );
        }
    }

    #[test]
    fn grain_replays_identically_and_respects_budget() {
        let toy = toy_web();
        assert_eq!(
            grain_cloud(&toy, 7, DVec3::ZERO),
            grain_cloud(&toy, 7, DVec3::ZERO)
        );
        let nominal = grain_cloud(&web(), 1234, DVec3::ZERO);
        assert!(!nominal.is_empty(), "nominal web must emit grain");
        assert!(
            nominal.len() <= MAX_GRAIN_POINTS as usize,
            "grain over budget: {}",
            nominal.len()
        );
    }

    #[test]
    fn grain_stays_near_its_strand() {
        // Every toy grain point sits within the strand bundle (braid
        // amplitude + jitter headroom) of some link segment.
        let grain = grain_cloud(&toy_web(), 7, DVec3::ZERO);
        assert!(!grain.is_empty());
        let segs = [
            ([0.0, 0.0, 0.0], [30.0, 0.0, 0.0]),
            ([0.0, 0.0, 0.0], [0.0, 40.0, 0.0]),
        ];
        for (pos, _, _) in &grain {
            let p = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
            let near = segs.iter().any(|(s, e)| {
                let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
                let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let t = ((p[0] - s[0]) * d[0] + (p[1] - s[1]) * d[1] + (p[2] - s[2]) * d[2]) / l2;
                let t = t.clamp(0.0, 1.0);
                let q = [s[0] + d[0] * t, s[1] + d[1] * t, s[2] + d[2] * t];
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                    <= 8.0
            });
            assert!(near, "grain drifted off every strand: {p:?}");
        }
    }

    #[test]
    fn impostors_emit_core_and_halo_per_node() {
        let web = toy_web();
        let impostors = node_impostors(&web, DVec3::ZERO);
        assert_eq!(impostors.len(), web.nodes.len() * 2);
        for i in 0..web.nodes.len() {
            let (core, halo) = (&impostors[2 * i], &impostors[2 * i + 1]);
            // Same position (the hub).
            assert_eq!(core.0, halo.0);
            // Core is emissive (bloom target), fixed pixel size, fully
            // opaque; halo is world-sized (kind 1, Mpc) and fainter.
            assert!(
                core.1.iter().any(|c| *c > 1.0),
                "core must be emissive: {:?}",
                core.1
            );
            assert_eq!(core.2[2], 0.0, "core must be pixel-sized");
            assert_eq!(halo.2[2], 1.0, "halo must be world-sized");
            assert!(
                (2.0..=8.0).contains(&halo.2[0]),
                "halo world diameter out of band: {}",
                halo.2[0]
            );
            assert!(halo.2[1] < core.2[1], "halo must be fainter");
        }
        // Mass grading: the 1e15 node outshines the 5e12 node.
        let heavy = impostors[0].1;
        let light = impostors[2].1;
        for c in 0..3 {
            assert!(heavy[c] > light[c], "heavy core must outshine light");
        }
    }

    #[test]
    fn enrichment_layouts_are_origin_relative() {
        // Rebase invariant for the new layouts: shifting the origin
        // shifts every point by exactly the delta (demo rebase-safe).
        let web = toy_web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = braid_segments(&web, 7, a);
        let pb = braid_segments(&web, 7, b);
        assert_eq!(pa.len(), pb.len());
        for (p, q) in pa.iter().zip(pb.iter()).take(50) {
            for axis in 0..3 {
                let delta = f64::from(p.0[axis]) - f64::from(q.0[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
        let ga = grain_cloud(&web, 7, a);
        let gb = grain_cloud(&web, 7, b);
        assert_eq!(ga.len(), gb.len());
        for ((p, _, _), (q, _, _)) in ga.iter().zip(gb.iter()).take(50) {
            for axis in 0..3 {
                let delta = f64::from(p[axis]) - f64::from(q[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
        let ia = node_impostors(&web, a);
        let ib = node_impostors(&web, b);
        for (p, q) in ia.iter().zip(ib.iter()) {
            for axis in 0..3 {
                let delta = f64::from(p.0[axis]) - f64::from(q.0[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
    }

    #[test]
    fn enrichment_layouts_are_finite_and_bounded() {
        // GPU-debug aid (visual-issue round 2): scans every emitted
        // vertex of the nominal web for non-finite or out-of-band
        // values. The shaders assume finite inputs with sane
        // magnitudes — Inf/NaN here would decorrelate color channels
        // through the additive chain into rainbow squares on screen.
        let web = web();
        let origin = DVec3::ZERO;
        let seed = 1234;
        for (pos, rgba) in braid_segments(&web, seed, origin) {
            for axis in 0..3 {
                assert!(pos[axis].is_finite(), "braid pos not finite");
                assert!(
                    rgba[axis].is_finite() && (0.0..=2.0).contains(&rgba[axis]),
                    "braid color out of band: {:?}",
                    rgba
                );
            }
            assert!(
                rgba[3].is_finite() && (0.0..=1.0).contains(&rgba[3]),
                "braid alpha out of band: {:?}",
                rgba
            );
        }
        let points = grain_cloud(&web, seed, origin)
            .into_iter()
            .chain(glow_point_cloud(&web, origin))
            .chain(node_impostors(&web, origin))
            .collect::<Vec<_>>();
        assert!(!points.is_empty());
        for (pos, color, misc) in &points {
            for axis in 0..3 {
                assert!(pos[axis].is_finite(), "point pos not finite");
                assert!(
                    color[axis].is_finite() && (0.0..=5.0).contains(&color[axis]),
                    "point color out of band: {color:?}"
                );
            }
            assert!(
                misc[0].is_finite() && (0.0..=300.0).contains(&misc[0]),
                "sprite size out of band: {misc:?}"
            );
            assert!(
                misc[1].is_finite() && (0.0..=1.0).contains(&misc[1]),
                "sprite alpha out of band: {misc:?}"
            );
            assert!(
                misc[2] == 0.0 || misc[2] == 1.0,
                "sprite kind must be 0 or 1: {misc:?}"
            );
        }
    }

    #[test]
    fn clouds_are_origin_relative() {
        // Layout invariant both surfaces rely on: shifting the origin
        // shifts every point by exactly the delta (rebase-safe).
        let web = web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = node_point_cloud(&web, a);
        let pb = node_point_cloud(&web, b);
        for (p, q) in pa.iter().zip(pb.iter()).take(50) {
            for axis in 0..3 {
                let delta = f64::from(p.0[axis]) - f64::from(q.0[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
    }
}
