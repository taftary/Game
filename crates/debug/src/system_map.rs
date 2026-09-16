//! System-map view state: camera, orbit rings, planet focus, selection.
//!
//! Pure + headless — AU map math lives here, GPU upload in the binary.
//! Same projection contract as the galaxy map (east/right, north/up,
//! NDC +1 = top): the two map cameras share semantics, not code —
//! galaxy units are compressed ly, system units are AU, and the constants
//! differ.
//!
//! Patched positions (`docs/game/journey.md` L3): the orbit radius is
//! data; the true anomaly is presentation. Planet dots sit at a fixed
//! golden-angle slot per index — deterministic, no stream, no state.
//! `cos`/`sin` appear ONLY in this view projection, never in a
//! descriptor or hash (1-ulp view jitter is invisible; 1-ulp data drift
//! is not — risks #2).

use game::transit::Transit;
use game_engine::render::{QualityTier, SeededPlanet};
use game_engine::universe::{
    Atmosphere, PlanetDescriptor, PlanetId, PlanetType, StarDescriptor, SystemDescriptor,
    generate_system,
};

/// Click-select radius in screen pixels (converted to AU at pick time).
pub const PICK_RADIUS_PX: f32 = 10.0;

/// Closest the camera may zoom: half-extent in AU.
pub const MIN_VIEW_RADIUS_AU: f64 = 0.05;

/// Orbit ring tessellation (line segments per ring).
pub const RING_SEGMENTS: usize = 128;

/// Golden-angle slot per planet index (radians): spreads siblings
/// around the star without streams or state.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

/// AU map camera: center (x, z) + half-height extent. Same zoom/pan
/// contract as [`crate::galaxy_map::GalaxyCamera`], AU-flavored clamps.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemCamera {
    pub center_x: f64,
    pub center_z: f64,
    /// Half-extent of the visible z range; x scales by viewport aspect.
    pub view_radius: f64,
    /// Widest zoom for this system (outermost orbit + margin).
    pub max_radius: f64,
}

impl SystemCamera {
    /// Frame `outer_orbit_au`: whole system in view plus margin.
    pub fn frame_system(outer_orbit_au: f64) -> Self {
        let max_radius = (outer_orbit_au * 1.2).max(MIN_VIEW_RADIUS_AU * 2.0);
        Self {
            center_x: 0.0,
            center_z: 0.0,
            view_radius: max_radius,
            max_radius,
        }
    }

    /// Zoom: `factor < 1` zooms in. Clamped to
    /// [`MIN_VIEW_RADIUS_AU`]..=`max_radius`.
    pub fn zoom_by(&mut self, factor: f64) {
        self.view_radius = (self.view_radius * factor).clamp(MIN_VIEW_RADIUS_AU, self.max_radius);
    }

    /// Pan by map-unit deltas, clamped to the framed disk plus a
    /// half-view margin.
    pub fn pan_by(&mut self, dx: f64, dz: f64) {
        let margin = self.max_radius + self.view_radius;
        self.center_x = (self.center_x + dx).clamp(-margin, margin);
        self.center_z = (self.center_z + dz).clamp(-margin, margin);
    }

    /// Compressed AU per screen pixel (x and z agree by construction).
    pub fn au_per_pixel(&self, _vp_w: f32, vp_h: f32) -> f64 {
        2.0 * self.view_radius / f64::from(vp_h.max(1.0))
    }

    /// Map (x, z) → y-down viewport pixels. `vp` is (x, y, w, h).
    pub fn map_to_screen(&self, x: f64, z: f64, vp: (f32, f32, f32, f32)) -> (f32, f32) {
        let (vpx, vpy, vpw, vph) = vp;
        let nx = (x - self.center_x)
            / (self.view_radius * f64::from(vpw.max(1.0)) / f64::from(vph.max(1.0)));
        let ny = (z - self.center_z) / self.view_radius;
        (
            vpx + ((nx * 0.5 + 0.5) * f64::from(vpw.max(1.0))) as f32,
            vpy + ((0.5 - ny * 0.5) * f64::from(vph.max(1.0))) as f32,
        )
    }

    /// Viewport pixels → map (x, z). Exact inverse of
    /// [`SystemCamera::map_to_screen`] (round-trip tested).
    pub fn screen_to_map(&self, sx: f32, sy: f32, vp: (f32, f32, f32, f32)) -> (f64, f64) {
        let (vpx, vpy, vpw, vph) = vp;
        let w = f64::from(vpw.max(1.0));
        let h = f64::from(vph.max(1.0));
        let nx = (f64::from(sx - vpx) / w - 0.5) * 2.0;
        let ny = (0.5 - f64::from(sy - vpy) / h) * 2.0;
        (
            self.center_x + nx * self.view_radius * w / h,
            self.center_z + ny * self.view_radius,
        )
    }

    /// Ortho MVP over AU map space, column-major (same contract as the
    /// galaxy camera: north = +y NDC).
    pub fn mvp(&self, vp_w: f32, vp_h: f32) -> [[f32; 4]; 4] {
        let sx = (self.view_radius * f64::from(vp_w.max(1.0)) / f64::from(vp_h.max(1.0))) as f32;
        let sy = self.view_radius as f32;
        let (cx, cz) = (self.center_x as f32, self.center_z as f32);
        [
            [1.0 / sx, 0.0, 0.0, 0.0],
            [0.0, 1.0 / sy, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-cx / sx, -cz / sy, 0.0, 1.0],
        ]
    }
}

/// Presentation slot of planet `planet_index` on its orbit: golden-angle
/// anomaly (view-only; the radius is descriptor data).
pub fn planet_slot(orbit_au: f64, planet_index: u32) -> (f64, f64) {
    let angle = f64::from(planet_index) * GOLDEN_ANGLE;
    (orbit_au * angle.cos(), orbit_au * angle.sin())
}

/// Orbit ring tessellation in map space (x, z) pairs for the line
/// pipeline. Fixed segment count — identical on every platform.
pub fn orbit_ring_points(orbit_au: f64) -> Vec<(f32, f32)> {
    (0..=RING_SEGMENTS)
        .map(|i| {
            let t = i as f64 / RING_SEGMENTS as f64;
            let angle = t * std::f64::consts::TAU;
            (
                (orbit_au * angle.cos()) as f32,
                (orbit_au * angle.sin()) as f32,
            )
        })
        .collect()
}

/// Nearest planet to map point (x, z) within `threshold` AU. Exact ties
/// keep the lowest planet index.
pub fn pick_planet(system: &SystemDescriptor, x: f64, z: f64, threshold: f64) -> Option<u32> {
    let mut best: Option<(u32, f64)> = None;
    for (i, planet) in system.planets.iter().enumerate() {
        let (px, pz) = planet_slot(planet.orbit_radius_au, i as u32);
        let dx = px - x;
        let dz = pz - z;
        let d = dx * dx + dz * dz;
        if d <= threshold * threshold
            && best.is_none_or(|(bi, bd)| d < bd || (d == bd && (i as u32) < bi))
        {
            best = Some((i as u32, d));
        }
    }
    best.map(|(i, _)| i)
}

/// Whole system-map screen state: the loaded system, its camera, the L4
/// focus mode, and the selection. The binary mirrors loads by
/// re-uploading the ring + point buffers.
#[derive(Clone, Debug)]
pub struct SystemMapView {
    pub seed: u64,
    pub system: SystemDescriptor,
    pub camera: SystemCamera,
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
            camera: SystemCamera::frame_system(outer),
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
        self.camera = SystemCamera::frame_system(outer);
        self.selected = None;
        self.focus = None;
        self.travel_offer = None;
        self.transit = None;
    }

    /// Click-select: viewport cursor → map point → nearest planet within
    /// [`PICK_RADIUS_PX`]. Stores and returns the pick.
    pub fn select_at(&mut self, cursor: (f32, f32), vp: (f32, f32, f32, f32)) -> Option<u32> {
        let (x, z) = self.camera.screen_to_map(cursor.0, cursor.1, vp);
        let threshold = f64::from(PICK_RADIUS_PX) * self.camera.au_per_pixel(vp.2, vp.3);
        self.selected = pick_planet(&self.system, x, z, threshold);
        self.selected
    }

    /// Toggle the L4 focus on the selected planet: focus centers the
    /// camera on the planet's slot at 60% of its orbit radius;
    /// un-focus reframes the whole system. Focus follows selection —
    /// focusing with nothing selected is a no-op.
    pub fn toggle_focus(&mut self) {
        match (self.focus, self.selected) {
            (None, Some(i)) => {
                if let Some(planet) = self.system.planets.get(i as usize) {
                    let (px, pz) = planet_slot(planet.orbit_radius_au, i);
                    self.focus = Some(i);
                    self.camera.center_x = px;
                    self.camera.center_z = pz;
                    self.camera.view_radius = (planet.orbit_radius_au * 0.6)
                        .clamp(MIN_VIEW_RADIUS_AU, self.camera.max_radius);
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
                self.camera = SystemCamera::frame_system(outer);
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

    const VP: (f32, f32, f32, f32) = (0.0, 0.0, 800.0, 600.0);

    fn sample_star() -> StarDescriptor {
        generate_galaxy(42, 4).stars.into_iter().next().unwrap()
    }

    fn sample_view() -> SystemMapView {
        let star = sample_star();
        let mut view = SystemMapView {
            seed: 0,
            system: generate_system(0, &star),
            camera: SystemCamera::frame_system(1.0),
            selected: None,
            focus: None,
            travel_offer: None,
            transit: None,
        };
        view.load(42, &star);
        view
    }

    #[test]
    fn screen_map_round_trips() {
        let view = sample_view();
        let tol = view.camera.au_per_pixel(VP.2, VP.3);
        for (x, z) in [(0.0, 0.0), (1.5, -0.7), (-8.0, 8.0)] {
            let (sx, sy) = view.camera.map_to_screen(x, z, VP);
            let (rx, rz) = view.camera.screen_to_map(sx, sy, VP);
            assert!((rx - x).abs() < tol, "{x} -> {rx}");
            assert!((rz - z).abs() < tol, "{z} -> {rz}");
        }
    }

    #[test]
    fn load_frames_all_orbits_and_clears_state() {
        let mut view = sample_view();
        view.selected = Some(0);
        view.focus = Some(0);
        view.travel_offer = Some(0);
        view.camera.zoom_by(0.01);
        let star = sample_star();
        view.load(42, &star);
        assert_eq!(view.selected, None);
        assert_eq!(view.focus, None);
        assert_eq!(view.travel_offer, None);
        // Every orbit ring projects inside the viewport.
        for planet in &view.system.planets {
            for (x, z) in orbit_ring_points(planet.orbit_radius_au) {
                let (sx, sy) = view.camera.map_to_screen(x.into(), z.into(), VP);
                assert!(
                    (0.0..=800.0).contains(&sx) && (0.0..=600.0).contains(&sy),
                    "orbit {} escapes the viewport",
                    planet.orbit_radius_au
                );
            }
        }
    }

    #[test]
    fn select_at_picks_the_projected_planet() {
        let mut view = sample_view();
        let (px, pz) = planet_slot(view.system.planets[0].orbit_radius_au, 0);
        let (sx, sy) = view.camera.map_to_screen(px, pz, VP);
        assert_eq!(view.select_at((sx, sy), VP), Some(0));
    }

    #[test]
    fn focus_centers_and_unfocus_reframes() {
        let mut view = sample_view();
        view.selected = Some(0);
        view.toggle_focus();
        assert_eq!(view.focus, Some(0));
        let (px, pz) = planet_slot(view.system.planets[0].orbit_radius_au, 0);
        assert!((view.camera.center_x - px).abs() < 1e-9);
        assert!((view.camera.center_z - pz).abs() < 1e-9);
        view.toggle_focus();
        assert_eq!(view.focus, None);
        assert_eq!(view.camera.center_x, 0.0);
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
    fn slots_differ_per_index_and_rings_close() {
        let (ax, az) = planet_slot(2.0, 0);
        let (bx, bz) = planet_slot(2.0, 1);
        assert!((ax - bx).abs() + (az - bz).abs() > 0.5);
        let ring = orbit_ring_points(2.0);
        assert_eq!(ring.len(), RING_SEGMENTS + 1);
        // Closes up to float noise (sin(2π) is ~1e-16, not 0).
        let (first, last) = (ring[0], ring[RING_SEGMENTS]);
        assert!((first.0 - last.0).abs() + (first.1 - last.1).abs() < 1e-6);
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
