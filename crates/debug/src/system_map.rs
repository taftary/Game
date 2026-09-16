//! System-map view state: 3D camera, orbit rings, planet focus, selection.
//!
//! Pure + headless — AU map math lives here, GPU upload in the binary.
//! 3D perspective view (universe-maps-3d) over the same AU map space,
//! in the same world embedding as the galaxy map: `(east, up, −north)`.
//!
//! Patched positions ([`journey.md`](../../docs/game/journey.md) L3):
//! the orbit radius is data; the true anomaly is presentation. Planet
//! dots sit at a fixed golden-angle slot per index, now tilted out of
//! the plane by a presentation-only inclination per orbit —
//! deterministic, no stream, no state. `cos`/`sin` and the index hash
//! below appear ONLY in this view projection, never in a descriptor
//! or hash (1-ulp view jitter is invisible; 1-ulp data drift is not —
//! risks #2).

use game::transit::Transit;
use game_engine::render::{QualityTier, SeededPlanet};
use game_engine::universe::{
    Atmosphere, PlanetDescriptor, PlanetId, PlanetType, StarDescriptor, SystemDescriptor,
    generate_system,
};
use glam::Vec3;
use std::f64::consts::TAU;

use crate::map_camera::{DEFAULT_PITCH, DEFAULT_YAW, MapOrbitCamera};
use crate::picking::project_to_screen;
use crate::ui::Rect;

/// Click-select radius in screen pixels (nearest projected planet
/// within this distance wins).
pub const PICK_RADIUS_PX: f32 = 10.0;

/// Closest the camera may zoom: vertical half-extent in AU (mirrors
/// the old ortho minimum; converted to an orbit distance by
/// [`framing_camera`]).
pub const MIN_VIEW_RADIUS_AU: f64 = 0.05;

/// Orbit ring tessellation (line segments per ring).
pub const RING_SEGMENTS: usize = 128;

/// Golden-angle slot per planet index (radians): spreads siblings
/// around the star without streams or state.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

/// Presentation inclination ceiling (radians): per-orbit tilts stay
/// within ±10° of the plane (OQ-1 proposal — reads as 3D, still
/// plausible for a system).
pub const MAX_INCLINATION_RAD: f64 = 10.0 * std::f64::consts::PI / 180.0;

/// Default 3D framing for the system around `outer_orbit_au`: whole
/// system plus margin, tilted open from the south side. Zoom range
/// mirrors the old ortho half-extents via
/// [`MapOrbitCamera::distance_for_half_height`].
pub fn framing_camera(outer_orbit_au: f64) -> MapOrbitCamera {
    let max_half = (outer_orbit_au * 1.2).max(MIN_VIEW_RADIUS_AU * 2.0) as f32;
    MapOrbitCamera::new(
        Vec3::ZERO,
        MapOrbitCamera::distance_for_half_height(max_half),
        DEFAULT_YAW,
        DEFAULT_PITCH,
        MapOrbitCamera::distance_for_half_height(MIN_VIEW_RADIUS_AU as f32),
        MapOrbitCamera::distance_for_half_height(max_half),
        max_half,
    )
}

/// splitmix64 (Steele et al.): a stream-free index hash — integer ops
/// only, bit-identical on every platform. Separate salts decorrelate
/// the inclination and node draws.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Upper 53 bits as a dyadic fraction in [0, 1): exact division by a
/// power of two, identical on every platform.
fn unit_fraction(hash: u64) -> f64 {
    (hash >> 11) as f64 / (1u64 << 53) as f64
}

/// Presentation orientation of orbit `planet_index`: `(inclination,
/// ascending node)` in radians. Signed inclination within
/// ±[`MAX_INCLINATION_RAD`], node uniform in [0, τ). View-only —
/// never stored in a descriptor.
pub fn orbit_orientation(planet_index: u32) -> (f64, f64) {
    let index = planet_index as u64;
    let inclination = (unit_fraction(splitmix64(index.wrapping_add(0xC2B2_0217_B5D1_0AAD))) * 2.0
        - 1.0)
        * MAX_INCLINATION_RAD;
    let node = unit_fraction(splitmix64(index.wrapping_add(0x1656_2629_2E35_0B79))) * TAU;
    (inclination, node)
}

/// Rotate map-plane point `p` out of the plane by `inclination` about
/// the line of nodes `u = (cos node, 0, sin node)` (Rodrigues — a
/// rigid rotation, so radii and ring closure survive to f64 rounding).
fn tilt_point(p: (f64, f64, f64), inclination: f64, node: f64) -> (f64, f64, f64) {
    let (ux, uz) = (node.cos(), node.sin());
    let (ci, si) = (inclination.cos(), inclination.sin());
    let (px, py, pz) = p;
    // u × p with uy = 0.
    let cx = -uz * py;
    let cy = uz * px - ux * pz;
    let cz = ux * py;
    let dot = ux * px + uz * pz;
    let k = 1.0 - ci;
    (
        px * ci + cx * si + ux * dot * k,
        py * ci + cy * si,
        pz * ci + cz * si + uz * dot * k,
    )
}

/// Presentation slot of planet `planet_index` on its orbit: world
/// `(east, up, −north)` with golden-angle anomaly (kept from the 2D
/// map) plus the presentation tilt. View-only; the radius is
/// descriptor data.
pub fn planet_slot(orbit_au: f64, planet_index: u32) -> (f64, f64, f64) {
    let anomaly = f64::from(planet_index) * GOLDEN_ANGLE;
    // Flat ring in the map plane (east, north) = (r cos, r sin), north
    // flipping into world −z.
    let flat = (orbit_au * anomaly.cos(), 0.0, -(orbit_au * anomaly.sin()));
    let (inclination, node) = orbit_orientation(planet_index);
    tilt_point(flat, inclination, node)
}

/// Orbit ring tessellation in world space for the line pipeline.
/// Fixed segment count — identical on every platform. The planet slot
/// sits on this ring by construction (same tilt, same radius).
pub fn orbit_ring_points(orbit_au: f64, planet_index: u32) -> Vec<(f32, f32, f32)> {
    let (inclination, node) = orbit_orientation(planet_index);
    (0..=RING_SEGMENTS)
        .map(|i| {
            let t = i as f64 / RING_SEGMENTS as f64;
            let angle = t * TAU;
            let flat = (orbit_au * angle.cos(), 0.0, -(orbit_au * angle.sin()));
            let (x, y, z) = tilt_point(flat, inclination, node);
            (x as f32, y as f32, z as f32)
        })
        .collect()
}

/// Whole system-map screen state: the loaded system, its camera, the L4
/// focus mode, and the selection. The binary mirrors loads by
/// re-uploading the ring + point buffers.
#[derive(Clone, Debug)]
pub struct SystemMapView {
    pub seed: u64,
    pub system: SystemDescriptor,
    pub camera: MapOrbitCamera,
    pub selected: Option<u32>,
    /// L4 planet focus: within-layer zoom mode centered on the focused
    /// planet (`None` = wide system view). No fade, no prefetch — the
    /// journey machine treats it as zoom, not a layer swap.
    pub focus: Option<u32>,
    /// Travel offer (UMAP-015): armed by `T` on a selected planet,
    /// consumed by the timed transit (UMAP-018). Display-only until then.
    pub travel_offer: Option<u32>,
    /// Underway transit countdown (UMAP-018): `Some` while ticks accrue
    /// toward commit; `None` before begin and after commit/cancel.
    /// Cleared by `load` like every other per-system state.
    pub transit: Option<Transit>,
}

impl SystemMapView {
    /// Open the map on `star`: same as [`SystemMapView::load`] on a fresh
    /// view (the binary uses `load` for star switches).
    pub fn new(seed: u64, star: &StarDescriptor) -> Self {
        let system = generate_system(seed, star);
        let outer = system
            .planets
            .last()
            .map(|p| p.orbit_radius_au)
            .unwrap_or(1.0);
        let mut view = Self {
            seed,
            system,
            camera: framing_camera(outer),
            selected: None,
            focus: None,
            travel_offer: None,
            transit: None,
        };
        view.load(seed, star);
        view
    }

    /// Load the system around `star` for `seed`: regenerate descriptors,
    /// frame the orbits, clear selection/focus/offer.
    pub fn load(&mut self, seed: u64, star: &StarDescriptor) {
        self.seed = seed;
        self.system = generate_system(seed, star);
        let outer = self
            .system
            .planets
            .last()
            .map(|p| p.orbit_radius_au)
            .unwrap_or(1.0);
        self.camera = framing_camera(outer);
        self.selected = None;
        self.focus = None;
        self.travel_offer = None;
        self.transit = None;
    }

    /// Click-select: project every planet slot through the current
    /// view-projection, keep the nearest projected point within
    /// [`PICK_RADIUS_PX`]. Exact projected-distance ties keep the
    /// lowest planet index. Stores and returns the pick.
    pub fn select_at(&mut self, cursor: (f32, f32), vp: Rect) -> Option<u32> {
        let view_proj = self.camera.view_proj(vp.w / vp.h);
        let mut best: Option<(u32, f32)> = None;
        for (i, planet) in self.system.planets.iter().enumerate() {
            let (px, py, pz) = planet_slot(planet.orbit_radius_au, i as u32);
            let world = Vec3::new(px as f32, py as f32, pz as f32);
            if let Some((sx, sy)) = project_to_screen(world, view_proj, vp) {
                let d = (sx - cursor.0).hypot(sy - cursor.1);
                if d <= PICK_RADIUS_PX
                    && best.is_none_or(|(bi, bd)| d < bd || (d == bd && (i as u32) < bi))
                {
                    best = Some((i as u32, d));
                }
            }
        }
        self.selected = best.map(|(i, _)| i);
        self.selected
    }

    /// Toggle the L4 focus on the selected planet: focus jumps the
    /// camera target to the planet's world slot at 60% of its orbit
    /// radius; un-focus reframes the whole system. Focus follows
    /// selection — focusing with nothing selected is a no-op.
    pub fn toggle_focus(&mut self) {
        match (self.focus, self.selected) {
            (None, Some(i)) => {
                if let Some(planet) = self.system.planets.get(i as usize) {
                    let (px, py, pz) = planet_slot(planet.orbit_radius_au, i);
                    self.focus = Some(i);
                    self.camera
                        .set_target(Vec3::new(px as f32, py as f32, pz as f32));
                    let half = (planet.orbit_radius_au * 0.6).max(MIN_VIEW_RADIUS_AU) as f32;
                    self.camera
                        .set_distance(MapOrbitCamera::distance_for_half_height(half));
                }
            }
            _ => {
                self.focus = None;
                let outer = self
                    .system
                    .planets
                    .last()
                    .map(|p| p.orbit_radius_au)
                    .unwrap_or(1.0);
                self.camera = framing_camera(outer);
            }
        }
    }
}

/// Orbit arrival binding (UMAP-020): everything the orbit view needs
/// from the target descriptor, built once at transit commit.
///
/// - `seeded` binds the descriptor mesh seed + radius into a
///   [`SeededPlanet`] (the mesh itself stays seed-independent per
///   ADR-002 — the seed is reserved for per-seed surface layers in
///   M2/M3, and this record carries it there).
/// - `atmosphere` drives the orbit backdrop (debug clear color) and the
///   planet mesh tint — descriptor palettes, never hardcoded branches.
/// - `None` on an out-of-range index: the commit path surfaces it
///   instead of panicking (UMAP-017 follow-through).
#[derive(Clone, Debug)]
pub struct OrbitArrival {
    pub id: PlanetId,
    pub planet_type: PlanetType,
    pub radius_km: f32,
    pub atmosphere: Atmosphere,
    pub mesh_seed: u64,
    pub seeded: SeededPlanet,
}

/// Bind the arrival for `planet_index` in `system`. The descriptor
/// already carries its full content id (seed included), so no seed
/// argument is needed. `None` names no planet.
pub fn arrival_for(system: &SystemDescriptor, planet_index: u32) -> Option<OrbitArrival> {
    let planet = system.planets.get(planet_index as usize)?;
    let descriptor: &PlanetDescriptor = &planet.descriptor;
    Some(OrbitArrival {
        id: descriptor.id,
        planet_type: descriptor.planet_type,
        radius_km: descriptor.radius_km,
        atmosphere: descriptor.atmosphere,
        mesh_seed: descriptor.mesh_seed,
        seeded: SeededPlanet::generate(
            descriptor.mesh_seed,
            QualityTier::Medium,
            descriptor.radius_km,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::generate_galaxy;

    fn vp() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        }
    }

    fn sample_star() -> StarDescriptor {
        generate_galaxy(42, 4).stars.into_iter().next().unwrap()
    }

    fn sample_view() -> SystemMapView {
        let star = sample_star();
        let mut view = SystemMapView {
            seed: 0,
            system: generate_system(0, &star),
            camera: framing_camera(1.0),
            selected: None,
            focus: None,
            travel_offer: None,
            transit: None,
        };
        view.load(42, &star);
        view
    }

    #[test]
    fn orientations_are_deterministic_and_bounded() {
        for i in 0..64 {
            let (a_incl, a_node) = orbit_orientation(i);
            let (b_incl, b_node) = orbit_orientation(i);
            assert_eq!((a_incl, a_node), (b_incl, b_node), "index {i} must replay");
            assert!(
                a_incl.abs() <= MAX_INCLINATION_RAD,
                "inclination {a_incl} exceeds the ceiling"
            );
            assert!((0.0..TAU).contains(&a_node), "node {a_node} out of range");
        }
        // Distinct indices orient distinctly (inclination and node both
        // vary — a degenerate hash would collapse the 3D read).
        let incls: Vec<f64> = (0..16).map(|i| orbit_orientation(i).0).collect();
        let nodes: Vec<f64> = (0..16).map(|i| orbit_orientation(i).1).collect();
        assert!(incls.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6));
        assert!(nodes.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6));
    }

    #[test]
    fn slots_keep_radius_and_sit_on_their_rings() {
        // The tilt is a rigid rotation: |slot| == orbit radius to f64
        // rounding, every ring point shares the radius, and slot +
        // ring share one plane (both carry the same tilt).
        for i in 0..8u32 {
            let r = 1.0 + f64::from(i) * 0.7;
            let (sx, sy, sz) = planet_slot(r, i);
            let slot_r = (sx * sx + sy * sy + sz * sz).sqrt();
            assert!(
                (slot_r - r).abs() / r < 1e-9,
                "slot radius drifted: {slot_r} vs {r}"
            );
            let ring = orbit_ring_points(r, i);
            assert_eq!(ring.len(), RING_SEGMENTS + 1);
            for (x, y, z) in &ring {
                let pr = ((*x as f64).powi(2) + (*y as f64).powi(2) + (*z as f64).powi(2)).sqrt();
                assert!(
                    (pr - r).abs() / r < 1e-4,
                    "ring radius drifted: {pr} vs {r}"
                );
            }
            // Closes up to float noise.
            let (first, last) = (ring[0], ring[RING_SEGMENTS]);
            let gap = (first.0 - last.0)
                .hypot(first.1 - last.1)
                .hypot(first.2 - last.2);
            assert!(gap < 1e-5, "ring {i} does not close: {gap}");
            // Coplanarity: the ring normal is the tilted +Y; the slot
            // must lie in the ring plane.
            let (incl, node) = orbit_orientation(i);
            let (nx, ny, nz) = tilt_point((0.0, 1.0, 0.0), incl, node);
            let off = (nx * sx + ny * sy + nz * sz).abs() / r;
            assert!(off < 1e-9, "slot off its ring plane: {off}");
        }
    }

    #[test]
    fn default_framing_keeps_map_conventions() {
        let view = sample_view();
        let view_proj = view.camera.view_proj(vp().w / vp().h);
        let screen =
            |world: Vec3| project_to_screen(world, view_proj, vp()).expect("framed point projects");
        let (cx, cy) = screen(Vec3::ZERO);
        assert!((cx - 400.0).abs() < 1.0 && (cy - 300.0).abs() < 1.0);
        let (ex, ey) = screen(Vec3::new(1.0, 0.0, 0.0));
        assert!(ex > cx && (ey - cy).abs() < 1.0, "east must be right");
        let (nx, ny) = screen(Vec3::new(0.0, 0.0, -1.0));
        assert!(ny < cy && (nx - cx).abs() < 1.0, "north must be up");
    }

    #[test]
    fn load_frames_all_orbits_and_clears_state() {
        let mut view = sample_view();
        view.selected = Some(0);
        view.focus = Some(0);
        view.travel_offer = Some(0);
        view.camera.zoom_by(0.01);
        view.camera.rotate(20.0, 10.0);
        let star = sample_star();
        view.load(42, &star);
        assert_eq!(view.selected, None);
        assert_eq!(view.focus, None);
        assert_eq!(view.travel_offer, None);
        assert_eq!(
            view.camera,
            framing_camera(
                view.system
                    .planets
                    .last()
                    .map(|p| p.orbit_radius_au)
                    .unwrap_or(1.0)
            )
        );
        // Every orbit ring projects inside NDC (framed with margin).
        let view_proj = view.camera.view_proj(vp().w / vp().h);
        for (i, planet) in view.system.planets.iter().enumerate() {
            for (x, y, z) in orbit_ring_points(planet.orbit_radius_au, i as u32) {
                let clip = view_proj * glam::Vec4::new(x, y, z, 1.0);
                assert!(clip.w > 0.0, "ring {i} behind the camera");
                let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
                assert!(
                    ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
                    "orbit {} escapes NDC: {ndc:?}",
                    planet.orbit_radius_au
                );
            }
        }
    }

    #[test]
    fn select_at_picks_the_projected_planet() {
        let mut view = sample_view();
        let (px, py, pz) = planet_slot(view.system.planets[0].orbit_radius_au, 0);
        let (sx, sy) = project_to_screen(
            Vec3::new(px as f32, py as f32, pz as f32),
            view.camera.view_proj(vp().w / vp().h),
            vp(),
        )
        .expect("planet 0 projects");
        assert_eq!(view.select_at((sx, sy), vp()), Some(0));
    }

    #[test]
    fn focus_jumps_to_the_planet_slot_and_unfocus_reframes() {
        let mut view = sample_view();
        view.selected = Some(0);
        view.toggle_focus();
        assert_eq!(view.focus, Some(0));
        let (px, py, pz) = planet_slot(view.system.planets[0].orbit_radius_au, 0);
        let target = view.camera.target();
        assert!((target.x - px as f32).abs() < 1e-4);
        assert!((target.y - py as f32).abs() < 1e-4);
        assert!((target.z - pz as f32).abs() < 1e-4);
        // 60% of the orbit radius, inside the camera range.
        let expect = MapOrbitCamera::distance_for_half_height(
            (view.system.planets[0].orbit_radius_au * 0.6).max(MIN_VIEW_RADIUS_AU) as f32,
        )
        .clamp(view.camera.min_distance(), view.camera.max_distance());
        assert!((view.camera.distance() - expect).abs() < 1e-3);
        view.toggle_focus();
        assert_eq!(view.focus, None);
    }

    #[test]
    fn focus_without_selection_is_noop() {
        let mut view = sample_view();
        let before = view.camera.clone();
        view.toggle_focus();
        assert_eq!(view.focus, None);
        assert_eq!(view.camera, before);
    }

    #[test]
    fn arrival_binds_descriptor_fields() {
        let view = sample_view();
        let planet = &view.system.planets[0].descriptor;
        let arrival = arrival_for(&view.system, 0).expect("planet 0 exists");
        assert_eq!(arrival.id, planet.id);
        assert_eq!(arrival.planet_type, planet.planet_type);
        assert_eq!(arrival.radius_km, planet.radius_km);
        assert_eq!(arrival.atmosphere, planet.atmosphere);
        assert_eq!(arrival.mesh_seed, planet.mesh_seed);
        assert_eq!(arrival.seeded.seed(), planet.mesh_seed);
        assert_eq!(arrival.seeded.radius(), planet.radius_km);
        assert_eq!(arrival.seeded.tier(), QualityTier::Medium);
        // Out of range names no planet — the commit path surfaces it.
        assert!(arrival_for(&view.system, 99).is_none());
    }

    #[test]
    fn showcased_pair_is_visually_distinct() {
        // OQ-3 (UX, UMAP-021): Rocky + Ice are the v1 showcased pair —
        // maximal palette + atmosphere contrast. The generator emits all
        // six types; these two must differ on every variety axis the
        // views read, with no hardcoded branches anywhere downstream.
        let mut rocky = None;
        let mut ice = None;
        for seed in 0..200 {
            let star = StarDescriptor {
                star_index: seed as u32 % 7,
                ..sample_star()
            };
            for planet in &generate_system(seed, &star).planets {
                match planet.descriptor.planet_type {
                    PlanetType::Rocky => rocky = Some(planet.descriptor.atmosphere),
                    PlanetType::Ice => ice = Some(planet.descriptor.atmosphere),
                    _ => {}
                }
            }
        }
        let (rocky, ice) = (rocky.expect("rocky"), ice.expect("ice"));
        assert_ne!(rocky.color, ice.color);
        assert!(
            (rocky.density - ice.density).abs() > 0.1,
            "densities overlap: {} vs {}",
            rocky.density,
            ice.density
        );
    }
}
