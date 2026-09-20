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
//! plus the cinematic enrichment layer [`strand_records`],
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
/// size, emissive): 1e12.3 M☉ → 0, 1e15 M☉ → 1. Small hubs stay
/// blue-white; only ≥1e14 M☉ clusters go golden (target's varied
/// cluster light, not one flat gold).
fn mass_level(mass_msun: f64) -> f32 {
    ((mass_msun.log10() - 12.3) / 2.7).clamp(0.0, 1.0) as f32
}

/// Mass-graded node tint: blue-white dwarfs → deep golden giants (the
/// gold end deepened in update-2026-09-19-1933 so massive hubs read
/// like the target reference — orange-gold, not pale yellow).
/// Shared by the demo and inspector uploads (one palette, two surfaces).
pub fn node_color(mass_msun: f64) -> [f32; 3] {
    let l = mass_level(mass_msun);
    [0.60 + 0.40 * l, 0.68 + 0.14 * l, 1.00 - 0.50 * l]
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

/// Dwarf glow points as tuples — the **gas veil**
/// (update-2026-09-19-1933, second pass): world-sized soft sprites
/// (`misc = (2.8 Mpc diameter, 0.045 alpha, kind 1)`) hugging the
/// descriptor's glow positions along the links, so filaments sit in a
/// faint blue mist like the target reference instead of on bare
/// black. Alpha stays tiny: the additive chain saturates fast, and
/// the points never enter voids (they emit along links only).
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
                [0.45, 0.50, 1.00],
                [2.8, 0.045, 1.0],
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
/// by the canonical endpoint pair (`seed ^ (a << 32 | b)`), so the
/// ribbon pass ([`strand_records`]) and the grain pass ([`grain_cloud`])
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

/// One braid strand's GPU-expansion record
/// (`update-2026-09-19-1245` P1): everything the ribbon vertex shader
/// needs to rebuild the strand — same [`braid_shape`] derivation the
/// grain pass uses, so grain still textures the drawn ribbons.
/// Compact (~100 B/strand, ~4 MB for the nominal web) vs the retired
/// baked `braid_segments` (~34 MB of `LineList` vertices per
/// surface). Origin-relative `a` only; trunk/basis/wander are
/// translation-invariant. `rgba` is the link-level density color at
/// mid-strand — the shader applies endpoint warming, redshift, and
/// the melt profile per vertex.
pub struct StrandRecord {
    /// Strand start = node `a` position, origin-relative Mpc f32.
    pub a: [f32; 3],
    /// Trunk vector `pb − pa`, Mpc f64 precision for shader replay.
    pub raw: [f64; 3],
    /// Lateral basis (orthonormal-adjacent), unit f64.
    pub u: [f64; 3],
    /// Lateral basis, unit f64.
    pub v: [f64; 3],
    /// Shared trunk wander: phase (rad), amplitude (Mpc).
    pub wander: [f64; 2],
    /// Strand twist: phase (rad), windings, lateral mix_u, mix_v.
    pub twist: [f64; 4],
    /// Link-level color (density ramp) + mid-strand alpha.
    pub rgba: [f32; 4],
}

/// Braided filament strands as compact per-strand records
/// (origin-relative Mpc f32 where it matters) for the instanced
/// ribbon pipeline. Strand count follows link density (1–3); the
/// density palette grades from dim indigo (faint) to blue-violet
/// past the bloom threshold (dense), alpha melting into the endpoint
/// nodes (the shader's melt profile, not baked here).
pub fn strand_records(web: &WebDescriptor, seed: u64, origin: DVec3) -> Vec<StrandRecord> {
    let mut out = Vec::new();
    for link in &web.links {
        let pa = web.nodes[link.a as usize].position_mpc;
        let pb = web.nodes[link.b as usize].position_mpc;
        let raw = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let shape = braid_shape(seed, link, pa, pb);
        let density = f64::from(link.density);
        // Filament palette: dim indigo → bright blue-violet by
        // density. Bands pinned by the enrichment tests: rgb ≤ 2.0,
        // monotonic in density, alpha ≤ 1.0.
        let rgb = [
            0.18 + 0.30 * density,
            0.22 + 0.35 * density,
            0.60 + 1.35 * density,
        ];
        let alpha = 0.05 + 0.95 * density;
        for strand in &shape.strands {
            out.push(StrandRecord {
                a: [
                    (pa[0] - origin.x) as f32,
                    (pa[1] - origin.y) as f32,
                    (pa[2] - origin.z) as f32,
                ],
                raw,
                u: shape.u,
                v: shape.v,
                wander: [shape.wander.phase, shape.wander.amplitude],
                twist: [strand.phase, strand.windings, strand.mix_u, strand.mix_v],
                rgba: [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, alpha as f32],
            });
        }
    }
    out
}

/// Smoke puffs replacing the retired ribbon tubes (perf-first gaseous
/// filaments): one soft camera-facing billboard per puff, hugging the
/// same [`braid_shape`] centerlines the grain pass uses. Counts scale
/// with `length × density` so dense hubs read thick and voids stay
/// empty; sizes are world Mpc diameters; alpha stays tiny (additive
/// chain saturates fast on mobile tile GPUs).
pub const SMOKE_PER_MPC: f64 = 0.08;
/// At least one puff per link (no filament ever vanishes).
pub const SMOKE_MIN_PER_LINK: u64 = 1;
/// Cap per link (overdraw control for the zoomed-out inspector, which
/// stacks ~50 strands/px).
pub const SMOKE_MAX_PER_LINK: u64 = 6;
/// Hard cap on emitted smoke puffs (buffer + fill-rate control).
pub const MAX_SMOKE_PUFFS: u32 = 65_000;
/// Domain-separated stream for smoke emission.
pub const SMOKE_STREAM: &str = "cosmic_web/smoke";

/// One smoke puff's CPU layout: origin-relative center Mpc f32,
/// world diameter Mpc, premultiplied link-level color + alpha, and a
/// deterministic noise seed (fragment dust variation). ~32 B/puff —
/// ~40k nominal puffs ≈ 1.3 MB/surface (vs ~3.8 MB strand records).
pub struct SmokePuff {
    /// Puff center, origin-relative Mpc f32.
    pub pos: [f32; 3],
    /// World diameter, Mpc (2.0–6.5 nominal).
    pub size_mpc: f32,
    /// Link-level color (density ramp, hub-warmed) + puff alpha.
    pub rgba: [f32; 4],
    /// Deterministic noise seed (radians).
    pub seed: f32,
}

/// Filament smoke as compact per-puff records for the instanced smoke
/// pipeline. Deterministic per (seed, web, origin); translation-
/// invariant except for `pos`.
pub fn smoke_puffs(web: &WebDescriptor, seed: u64, origin: DVec3) -> Vec<SmokePuff> {
    let mut rng = SeededRng::stream(seed, SMOKE_STREAM);
    let mut out: Vec<SmokePuff> = Vec::new();
    for link in &web.links {
        let pa = web.nodes[link.a as usize].position_mpc;
        let pb = web.nodes[link.b as usize].position_mpc;
        let raw_d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let len = ((raw_d[0]).powi(2) + (raw_d[1]).powi(2) + (raw_d[2]).powi(2)).sqrt();
        if !(len.is_finite() && len > 1e-6) {
            continue;
        }
        let density = f64::from(link.density);
        let want = len * SMOKE_PER_MPC * (0.4 + 0.6 * density);
        let mut emit = (want.floor() as u64).clamp(SMOKE_MIN_PER_LINK, SMOKE_MAX_PER_LINK);
        let frac = (want - want.floor()).clamp(0.0, 1.0);
        if rng.below(1000) < (frac * 1000.0) as u64 && emit < SMOKE_MAX_PER_LINK {
            emit += 1;
        }
        if out.len() as u64 + emit > u64::from(MAX_SMOKE_PUFFS) {
            break;
        }
        let shape = braid_shape(seed, link, pa, pb);
        let strands = shape.strands.len().max(1);
        let base_rgb = [
            0.18 + 0.30 * density,
            0.22 + 0.35 * density,
            0.60 + 1.35 * density,
        ];
        let alpha = 0.035 + 0.055 * density;
        for _ in 0..emit {
            let t = rng.unit_f64().clamp(0.02, 0.98);
            let strand = &shape.strands[rng.below(strands as u64) as usize];
            let c = braid_point(pa, raw_d, shape.u, shape.v, &shape.wander, strand, t);
            let j1 = ihalf3(&mut rng) * 2.0 * 1.0;
            let j2 = ihalf3(&mut rng) * 2.0 * 1.0;
            let melt = (std::f64::consts::PI * t).sin().max(0.0).sqrt();
            let warm = (1.0 - melt) * 0.55;
            let rgb = [
                base_rgb[0] + (1.05 - base_rgb[0]) * warm,
                base_rgb[1] + (0.72 - base_rgb[1]) * warm,
                base_rgb[2] + (0.42 - base_rgb[2]) * warm,
            ];
            let size = 2.0 + 3.0 * density + rng.unit_f64() * 1.5;
            let pseed = rng.unit_f64() * std::f64::consts::TAU;
            out.push(SmokePuff {
                pos: [
                    (c[0] + shape.u[0] * j1 + shape.v[0] * j2 - origin.x) as f32,
                    (c[1] + shape.u[1] * j1 + shape.v[1] * j2 - origin.y) as f32,
                    (c[2] + shape.u[2] * j1 + shape.v[2] * j2 - origin.z) as f32,
                ],
                size_mpc: size as f32,
                rgba: [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, alpha as f32],
                seed: pseed as f32,
            });
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

/// Node impostors as `(position, color, misc)` tuples: three sprites
/// per node — a white-hot pinpoint core (near-white, fixed pixel
/// size), the mass-graded golden mid core (emissive, the bloom
/// threshold's target), plus a soft amber halo (`kind` 1 =
/// world-unit diameter in Mpc, so it shrinks with distance instead of
/// plastering fixed-size quads over the whole web). Layered like the
/// target's cluster light (white center → gold → red-amber edge)
/// instead of one flat gold. Pure function of node mass/position (no
/// RNG): same node → same impostors, everywhere.
pub fn node_impostors(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    let mut out = Vec::with_capacity(web.nodes.len() * 3);
    for node in &web.nodes {
        let l = mass_level(node.mass_msun);
        let pos = [
            (node.position_mpc[0] - origin.x) as f32,
            (node.position_mpc[1] - origin.y) as f32,
            (node.position_mpc[2] - origin.z) as f32,
        ];
        let base = node_color(node.mass_msun);
        // White-hot pinpoint: near-white at every mass (dwarfs read
        // blue-white through the falloff, giants white-gold), fixed
        // pixel size, emissive. Band-checked: 1.05 × 3.2 ≤ 5.0.
        let pin = 1.2 + 2.0 * l;
        out.push((
            pos,
            [1.05 * pin, 1.00 * pin, 0.95 * pin],
            [1.5 + 2.0 * l, 1.0, 0.0],
        ));
        // Golden mid core: mass-graded, emissive (1.5–5.0x), fixed
        // pixel size (3–12 px — big enough that the bloom chain keeps
        // a visible halo).
        let emissive = 1.5 + 3.5 * l;
        out.push((
            pos,
            [base[0] * emissive, base[1] * emissive, base[2] * emissive],
            [3.0 + 9.0 * l, 1.0, 0.0],
        ));
        // Halo: large, faint, world-sized (2.5–6 Mpc diameter), cool
        // cyan for dwarfs warming to amber for giants.
        out.push((
            pos,
            [
                (0.60 + 0.40 * l) * (0.5 + 0.5 * l),
                (0.68 - 0.05 * l) * (0.5 + 0.5 * l),
                (0.95 - 0.45 * l) * (0.5 + 0.5 * l),
            ],
            [2.5 + 3.5 * l, 0.12 + 0.08 * l, 1.0],
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
    fn strand_record_counts_follow_density() {
        // Dense link → 3 records, faint link → 1 record; the two
        // toy links have orthogonal trunks so records split by `raw`.
        let records = strand_records(&toy_web(), 7, DVec3::ZERO);
        assert_eq!(records.len(), 3 + 1);
        let dense = records
            .iter()
            .filter(|r| (r.raw[0] - 30.0).abs() < 1e-6)
            .count();
        let faint = records
            .iter()
            .filter(|r| (r.raw[1] - 40.0).abs() < 1e-6)
            .count();
        assert_eq!((dense, faint), (3, 1));
    }

    #[test]
    fn strand_records_replay_identically_and_start_at_node_a() {
        let web = toy_web();
        // a[0] sits on node 0 (taper = 0 at t = 0 puts the ribbon
        // root exactly on the hub).
        let first = &strand_records(&web, 7, DVec3::ZERO)[0];
        assert!((f64::from(first.a[0])).abs() < 1e-3);
        assert!((f64::from(first.a[1])).abs() < 1e-3);
        assert!((f64::from(first.a[2])).abs() < 1e-3);
        // CPU-side shape bound (the GPU expands the same braid
        // math): mid-strand stays within the amplitude bound.
        for link in &web.links {
            let pa = web.nodes[link.a as usize].position_mpc;
            let pb = web.nodes[link.b as usize].position_mpc;
            let raw = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let shape = braid_shape(7, link, pa, pb);
            for strand in &shape.strands {
                let p = braid_point(pa, raw, shape.u, shape.v, &shape.wander, strand, 0.5);
                let lateral = ((p[0] - (pa[0] + raw[0] * 0.5)).powi(2)
                    + (p[1] - (pa[1] + raw[1] * 0.5)).powi(2)
                    + (p[2] - (pa[2] + raw[2] * 0.5)).powi(2))
                .sqrt();
                assert!(
                    lateral <= 2.0 * BRAID_AMPLITUDE_MPC + 1e-3,
                    "braid shape escaped its amplitude bound: {lateral}"
                );
            }
        }
    }

    #[test]
    fn strand_record_colors_grade_with_density() {
        // Every dense-link record must beat every faint-link record on
        // every channel (records carry the link-level palette; the
        // shader only re-scales it by the melt profile).
        let records = strand_records(&toy_web(), 7, DVec3::ZERO);
        let dense: Vec<&StrandRecord> = records
            .iter()
            .filter(|r| (r.raw[0] - 30.0).abs() < 1e-6)
            .collect();
        let faint: Vec<&StrandRecord> = records
            .iter()
            .filter(|r| (r.raw[1] - 40.0).abs() < 1e-6)
            .collect();
        assert_eq!((dense.len(), faint.len()), (3, 1));
        for d in &dense {
            for c in 0..4 {
                assert!(
                    d.rgba[c] > faint[0].rgba[c],
                    "dense record must outshine faint on channel {c}"
                );
            }
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
    fn impostors_emit_three_layered_sprites_per_node() {
        let web = toy_web();
        let impostors = node_impostors(&web, DVec3::ZERO);
        assert_eq!(impostors.len(), web.nodes.len() * 3);
        for i in 0..web.nodes.len() {
            let (pin, mid, halo) = (
                &impostors[3 * i],
                &impostors[3 * i + 1],
                &impostors[3 * i + 2],
            );
            // Same position (the hub).
            assert_eq!(pin.0, mid.0);
            assert_eq!(mid.0, halo.0);
            // White pinpoint: near-white, smallest, fixed pixel size.
            assert!(pin.1[0] >= pin.1[1] && pin.1[1] >= pin.1[2] - 1e-6);
            assert!(pin.2[0] < mid.2[0], "pinpoint must be smallest");
            assert_eq!(pin.2[2], 0.0, "pinpoint must be pixel-sized");
            // Golden mid core: emissive (a channel > 1.0), fixed pixel
            // size; halo is world-sized (kind 1, Mpc) and fainter.
            assert!(
                mid.1.iter().any(|c| *c > 1.0),
                "mid core must be emissive: {:?}",
                mid.1
            );
            assert_eq!(mid.2[2], 0.0, "mid core must be pixel-sized");
            assert_eq!(halo.2[2], 1.0, "halo must be world-sized");
            assert!(
                (2.0..=8.0).contains(&halo.2[0]),
                "halo world diameter out of band: {}",
                halo.2[0]
            );
            assert!(halo.2[1] < mid.2[1], "halo must be fainter");
        }
        // Mass grading: the 1e15 node outshines the 5e12 node on the
        // mid core's every channel.
        let heavy = impostors[1].1;
        let light = impostors[4].1;
        for c in 0..3 {
            assert!(heavy[c] > light[c], "heavy mid core must outshine light");
        }
    }

    #[test]
    fn enrichment_layouts_are_origin_relative() {
        // Rebase invariant for the new layouts: shifting the origin
        // shifts every strand root by exactly the delta (demo
        // rebase-safe; trunk/basis/wander are translation-invariant).
        let web = toy_web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = strand_records(&web, 7, a);
        let pb = strand_records(&web, 7, b);
        assert_eq!(pa.len(), pb.len());
        for (p, q) in pa.iter().zip(pb.iter()) {
            for axis in 0..3 {
                let delta = f64::from(p.a[axis]) - f64::from(q.a[axis]);
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
        for record in strand_records(&web, seed, origin) {
            for axis in 0..3 {
                assert!(record.a[axis].is_finite(), "strand root not finite");
                assert!(
                    (record.rgba[axis]).is_finite() && (0.0..=2.0).contains(&record.rgba[axis]),
                    "strand color out of band: {:?}",
                    record.rgba
                );
            }
            assert!(
                record.rgba[3].is_finite() && (0.0..=1.0).contains(&record.rgba[3]),
                "strand alpha out of band: {:?}",
                record.rgba
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

    #[test]
    fn smoke_replays_and_stays_in_budget() {
        let toy = toy_web();
        assert_eq!(
            smoke_puffs(&toy, 7, DVec3::ZERO).len(),
            smoke_puffs(&toy, 7, DVec3::ZERO).len()
        );
        let nominal = smoke_puffs(&web(), 1234, DVec3::ZERO);
        assert!(!nominal.is_empty(), "nominal web must emit smoke");
        assert!(
            nominal.len() <= MAX_SMOKE_PUFFS as usize,
            "smoke over budget: {}",
            nominal.len()
        );
        // Dense 30 Mpc link must emit at least as many puffs as the
        // faint 40 Mpc link emits per Mpc (density weighting).
        assert!(nominal.len() < 65_000);
    }

    #[test]
    fn smoke_puffs_are_finite_bounded_and_rebase_safe() {
        let web = toy_web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = smoke_puffs(&web, 7, a);
        let pb = smoke_puffs(&web, 7, b);
        assert_eq!(pa.len(), pb.len());
        assert!(!pa.is_empty());
        for (p, q) in pa.iter().zip(pb.iter()) {
            for axis in 0..3 {
                assert!(p.pos[axis].is_finite());
                let delta = f64::from(p.pos[axis]) - f64::from(q.pos[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
            for c in 0..3 {
                assert!(p.rgba[c].is_finite() && (0.0..=2.0).contains(&p.rgba[c]));
            }
            assert!(p.rgba[3].is_finite() && (0.0..=0.2).contains(&p.rgba[3]));
            assert!(p.size_mpc.is_finite() && (1.0..=10.0).contains(&p.size_mpc));
            assert!(p.seed.is_finite());
        }
    }
}
