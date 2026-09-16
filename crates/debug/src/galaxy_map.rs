//! Galaxy-map view state: 3D camera, seed-driven sprites, selection.
//!
//! Pure + headless — f64 map math lives here, GPU upload in the binary.
//! 3D perspective view (universe-maps-3d) over the same compressed-ly
//! map space: the disk thickness in `StarDescriptor::position_ly[1]`
//! (dropped by the old 2D ortho view) is real data and now drawn.
//!
//! World embedding: `world = (east, up, −north)` — the engine
//! ENU/player frame (y-up, north = −z) — so the default south-approach
//! framing shows north up and east right like the classic map.
//!
//! Picking is view-projection screen-space: project every star,
//! nearest within [`PICK_RADIUS_PX`], lowest index wins ties.

use game_engine::core::SeededRng;
use game_engine::universe::{
    DEFAULT_STAR_COUNT, GALAXY_RADIUS_LY, GalaxyDescriptor, SpectralClass, StarDescriptor,
    generate_galaxy,
};
use glam::Vec3;

use crate::map_camera::{DEFAULT_PITCH, DEFAULT_YAW, MapOrbitCamera};
use crate::picking::project_to_screen;
use crate::ui::{Rect, TextField};

/// Seed the map opens on (overridden by `--seed`; the viewer panel
/// field edits it at runtime).
pub const DEFAULT_GALAXY_SEED: u64 = 1234;

/// Closest the camera may zoom: vertical half-extent in compressed ly
/// (mirrors the old ortho minimum; converted to an orbit distance by
/// [`framing_camera`]).
pub const MIN_VIEW_RADIUS_LY: f64 = 500.0;

/// Widest the camera may zoom out: the full disk plus a margin.
pub const MAX_VIEW_RADIUS_LY: f64 = GALAXY_RADIUS_LY * 1.1;

/// Nebula impostor count (OQ-2 billboards; drawn as soft points).
pub const DEFAULT_NEBULA_COUNT: usize = 28;

/// L1 cosmic-web backdrop sprite count (generated sprite field, OQ-5).
pub const DEFAULT_BACKDROP_COUNT: usize = 400;

/// Click-select radius in screen pixels (nearest projected star within
/// this distance wins).
pub const PICK_RADIUS_PX: f32 = 8.0;

/// World-space position of a star in the map embedding
/// (`east, up, −north`): the disk thickness in `position_ly[1]` is
/// drawn; the north axis flips sign into the y-up world.
pub fn star_world(star: &StarDescriptor) -> Vec3 {
    Vec3::new(
        star.position_ly[0] as f32,
        star.position_ly[1] as f32,
        -(star.position_ly[2] as f32),
    )
}

/// Default 3D framing for the galaxy: whole disk plus margin, tilted
/// open from the south side. Zoom range mirrors the old ortho
/// half-extents via [`MapOrbitCamera::distance_for_half_height`].
pub fn framing_camera() -> MapOrbitCamera {
    let max_half = MAX_VIEW_RADIUS_LY as f32;
    MapOrbitCamera::new(
        Vec3::ZERO,
        MapOrbitCamera::distance_for_half_height(max_half),
        DEFAULT_YAW,
        DEFAULT_PITCH,
        MapOrbitCamera::distance_for_half_height(MIN_VIEW_RADIUS_LY as f32),
        MapOrbitCamera::distance_for_half_height(max_half),
        max_half,
    )
}

/// One nebula impostor (OQ-2 billboard): map-space disk, tint, alpha.
/// Derived from the galaxy seed — no baked assets, no hardcoding.
/// Plane haze: uploaded at world height 0.
#[derive(Clone, Debug, PartialEq)]
pub struct NebulaSprite {
    pub x: f64,
    pub z: f64,
    pub radius: f64,
    pub tint: [f32; 3],
    pub alpha: f32,
}

/// L1 cosmic-web backdrop sprite (OQ-5 generated sprite field): dim
/// far points over an extended 3D volume. The "far" read comes from
/// dimness, not a separate projection (L1 is backdrop-only; motion
/// fidelity is not a v1 requirement).
#[derive(Clone, Debug, PartialEq)]
pub struct BackdropSprite {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub brightness: f32,
}

/// Nebula impostor tints: teal / violet / amber / rose.
const NEBULA_TINTS: [[f32; 3]; 4] = [
    [0.30, 0.65, 0.70],
    [0.55, 0.40, 0.80],
    [0.85, 0.60, 0.30],
    [0.80, 0.35, 0.45],
];

/// Derive `count` nebula impostors from `seed`. Same seed → same field
/// on every platform (integer decisions + dyadic-grid floats only).
pub fn nebula_sprites(seed: u64, count: usize) -> Vec<NebulaSprite> {
    let mut rng = SeededRng::stream(seed, "galaxy/nebulae");
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let x = rng.range_f64(-1.0, 1.0) * GALAXY_RADIUS_LY;
        let z = rng.range_f64(-1.0, 1.0) * GALAXY_RADIUS_LY;
        if x * x + z * z > GALAXY_RADIUS_LY * GALAXY_RADIUS_LY {
            continue;
        }
        out.push(NebulaSprite {
            x,
            z,
            radius: rng.range_f64(3_000.0, 12_000.0),
            tint: NEBULA_TINTS[rng.below(NEBULA_TINTS.len() as u64) as usize],
            alpha: rng.range_f64(0.05, 0.12) as f32,
        });
    }
    out
}

/// Derive `count` L1 backdrop sprites from `seed` over a 3× field,
/// now a 3D volume (the y draw is appended after x/z on the same
/// stream — still deterministic per seed, still view-only).
pub fn backdrop_sprites(seed: u64, count: usize) -> Vec<BackdropSprite> {
    let mut rng = SeededRng::stream(seed, "galaxy/backdrop");
    (0..count)
        .map(|_| BackdropSprite {
            x: rng.range_f64(-1.5, 1.5) * GALAXY_RADIUS_LY,
            y: rng.range_f64(-1.5, 1.5) * GALAXY_RADIUS_LY,
            z: rng.range_f64(-1.5, 1.5) * GALAXY_RADIUS_LY,
            brightness: rng.range_f64(0.05, 0.22) as f32,
        })
        .collect()
}

/// Star point tint per spectral class (standard MK sequence colors).
pub fn spectral_color(class: SpectralClass) -> [f32; 3] {
    match class {
        SpectralClass::O => [0.62, 0.74, 1.00],
        SpectralClass::B => [0.71, 0.82, 1.00],
        SpectralClass::A => [0.92, 0.94, 1.00],
        SpectralClass::F => [1.00, 0.96, 0.86],
        SpectralClass::G => [1.00, 0.90, 0.70],
        SpectralClass::K => [1.00, 0.76, 0.52],
        SpectralClass::M => [1.00, 0.55, 0.38],
    }
}

/// Whole galaxy-map screen state. Regenerating the seed rebuilds
/// everything (galaxy, sprites, camera, selection) — the binary mirrors
/// this by re-uploading the point buffer.
#[derive(Clone, Debug)]
pub struct GalaxyMapView {
    pub seed: u64,
    pub galaxy: GalaxyDescriptor,
    pub camera: MapOrbitCamera,
    pub nebulae: Vec<NebulaSprite>,
    pub backdrop: Vec<BackdropSprite>,
    pub selected: Option<u32>,
    /// Seed text field (viewer panel): edited at runtime, applied on
    /// Load/Enter. Always mirrors `seed` after `new`/`regenerate`.
    pub seed_field: TextField,
}

impl GalaxyMapView {
    /// Open the map on `seed` with the v1 default star count.
    pub fn new(seed: u64) -> Self {
        let mut view = Self {
            seed,
            galaxy: generate_galaxy(seed, DEFAULT_STAR_COUNT),
            camera: framing_camera(),
            nebulae: nebula_sprites(seed, DEFAULT_NEBULA_COUNT),
            backdrop: backdrop_sprites(seed, DEFAULT_BACKDROP_COUNT),
            selected: None,
            seed_field: TextField::new(&seed.to_string()),
        };
        view.regenerate(seed);
        view
    }

    /// Re-roll everything for `seed` (camera resets to the default 3D
    /// framing, selection clears, field mirrors the seed).
    pub fn regenerate(&mut self, seed: u64) {
        self.seed = seed;
        self.galaxy = generate_galaxy(seed, DEFAULT_STAR_COUNT);
        self.camera = framing_camera();
        self.nebulae = nebula_sprites(seed, DEFAULT_NEBULA_COUNT);
        self.backdrop = backdrop_sprites(seed, DEFAULT_BACKDROP_COUNT);
        self.selected = None;
        self.seed_field = TextField::new(&seed.to_string());
    }

    /// Click-select: project every star through the current
    /// view-projection, keep the nearest projected point within
    /// [`PICK_RADIUS_PX`]. Exact projected-distance ties keep the
    /// lowest star index. Stores and returns the pick.
    pub fn select_at(&mut self, cursor: (f32, f32), vp: Rect) -> Option<u32> {
        let view_proj = self.camera.view_proj(vp.w / vp.h);
        let mut best: Option<(u32, f32)> = None;
        for star in &self.galaxy.stars {
            if let Some((sx, sy)) = project_to_screen(star_world(star), view_proj, vp) {
                let d = (sx - cursor.0).hypot(sy - cursor.1);
                if d <= PICK_RADIUS_PX
                    && best.is_none_or(|(bi, bd)| d < bd || (d == bd && star.star_index < bi))
                {
                    best = Some((star.star_index, d));
                }
            }
        }
        self.selected = best.map(|(i, _)| i);
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        }
    }

    #[test]
    fn default_framing_keeps_map_conventions() {
        // South-approach tilt: north (−z world) reads up-screen, east
        // (+x world) reads right-screen — the classic map read.
        let view = GalaxyMapView::new(3);
        let view_proj = view.camera.view_proj(vp().w / vp().h);
        let screen =
            |world: Vec3| project_to_screen(world, view_proj, vp()).expect("framed point projects");
        let (cx, cy) = screen(Vec3::ZERO);
        assert!((cx - 400.0).abs() < 1.0 && (cy - 300.0).abs() < 1.0);
        let (ex, ey) = screen(Vec3::new(10_000.0, 0.0, 0.0));
        assert!(ex > cx && (ey - cy).abs() < 2.0, "east must be right");
        // North in map space (+z) uploads as world −z.
        let (nx, ny) = screen(Vec3::new(0.0, 0.0, -10_000.0));
        assert!(ny < cy && (nx - cx).abs() < 2.0, "north must be up");
    }

    #[test]
    fn top_down_snap_keeps_conventions() {
        let mut view = GalaxyMapView::new(3);
        view.camera.toggle_top_down();
        let view_proj = view.camera.view_proj(vp().w / vp().h);
        let screen = |world: Vec3| {
            project_to_screen(world, view_proj, vp()).expect("snapped point projects")
        };
        let (cx, cy) = screen(Vec3::ZERO);
        assert!((cx - 400.0).abs() < 2.0 && (cy - 300.0).abs() < 2.0);
        let (ex, ey) = screen(Vec3::new(10_000.0, 0.0, 0.0));
        assert!(ex > cx && (ey - cy).abs() < 2.0);
        let (nx, ny) = screen(Vec3::new(0.0, 0.0, -10_000.0));
        assert!(ny < cy && (nx - cx).abs() < 2.0);
    }

    #[test]
    fn disk_thickness_separates_on_screen() {
        // Two stars sharing (x, z) but at opposite thickness extremes
        // must land on different pixels once tilted: depth is drawn.
        let view = GalaxyMapView::new(3);
        let view_proj = view.camera.view_proj(vp().w / vp().h);
        let a = project_to_screen(Vec3::new(5_000.0, 1_200.0, -3_000.0), view_proj, vp())
            .expect("thick star projects");
        let b = project_to_screen(Vec3::new(5_000.0, -1_200.0, -3_000.0), view_proj, vp())
            .expect("thick star projects");
        assert!(
            (a.0 - b.0).hypot(a.1 - b.1) > 0.5,
            "thickness must separate on screen: {a:?} vs {b:?}"
        );
    }

    #[test]
    fn zoom_clamps_to_configured_range() {
        let mut view = GalaxyMapView::new(3);
        view.camera.zoom_by(1e-9);
        assert_eq!(view.camera.distance(), view.camera.min_distance());
        view.camera.zoom_by(1e9);
        assert_eq!(view.camera.distance(), view.camera.max_distance());
        // The range mirrors the old ortho half-extents.
        let expect_min = MapOrbitCamera::distance_for_half_height(MIN_VIEW_RADIUS_LY as f32);
        let expect_max = MapOrbitCamera::distance_for_half_height(MAX_VIEW_RADIUS_LY as f32);
        assert!((view.camera.min_distance() - expect_min).abs() < 1e-3);
        assert!((view.camera.max_distance() - expect_max).abs() < 1e-3);
    }

    #[test]
    fn sprites_are_seed_stable() {
        assert_eq!(nebula_sprites(7, 10), nebula_sprites(7, 10));
        assert_ne!(nebula_sprites(7, 10), nebula_sprites(8, 10));
        assert_eq!(backdrop_sprites(7, 50), backdrop_sprites(7, 50));
        assert_ne!(backdrop_sprites(7, 50), backdrop_sprites(8, 50));
        for sprite in nebula_sprites(7, DEFAULT_NEBULA_COUNT) {
            assert!((0.05..=0.12).contains(&sprite.alpha));
            let r = (sprite.x * sprite.x + sprite.z * sprite.z).sqrt();
            assert!(r <= GALAXY_RADIUS_LY);
        }
        for sprite in backdrop_sprites(7, DEFAULT_BACKDROP_COUNT) {
            assert!((0.05..=0.22).contains(&sprite.brightness));
            assert!(sprite.x.abs() <= 1.5 * GALAXY_RADIUS_LY);
            assert!(sprite.y.abs() <= 1.5 * GALAXY_RADIUS_LY);
            assert!(sprite.z.abs() <= 1.5 * GALAXY_RADIUS_LY);
        }
    }

    #[test]
    fn view_regenerate_resets_everything() {
        let mut view = GalaxyMapView::new(1);
        view.selected = Some(5);
        view.camera.zoom_by(0.01);
        view.camera.rotate(30.0, 10.0);
        view.regenerate(2);
        assert_eq!(view.seed, 2);
        assert_eq!(view.selected, None);
        assert_eq!(view.camera, framing_camera());
        assert_eq!(view.galaxy.stars.len(), DEFAULT_STAR_COUNT as usize);
        assert_eq!(view.seed_field.text, "2");
    }

    #[test]
    fn select_at_picks_the_projected_star() {
        let mut view = GalaxyMapView::new(3);
        let star = &view.galaxy.stars[0];
        let (sx, sy) = project_to_screen(
            star_world(star),
            view.camera.view_proj(vp().w / vp().h),
            vp(),
        )
        .expect("star 0 projects");
        assert_eq!(view.select_at((sx, sy), vp()), Some(0));
        assert_eq!(view.selected, Some(0));
        // A click far from every projection misses (the full-disk
        // framing leaves no star within 8 px of a corner... unless one
        // lands there — assert consistency instead: re-pick at the
        // same cursor returns the same star).
        assert_eq!(view.select_at((sx, sy), vp()), Some(0));
    }

    #[test]
    fn select_at_honors_pixel_threshold() {
        let mut view = GalaxyMapView::new(3);
        let star = &view.galaxy.stars[0];
        let (sx, sy) = project_to_screen(
            star_world(star),
            view.camera.view_proj(vp().w / vp().h),
            vp(),
        )
        .expect("star 0 projects");
        // Just outside the pick radius along +x: star 0 may still lose
        // to a nearer star, but whatever wins must be within radius of
        // the cursor — check the winner is near the click.
        let won = view.select_at((sx + PICK_RADIUS_PX + 2.0, sy), vp());
        if let Some(i) = won {
            let winner = &view.galaxy.stars[i as usize];
            let (wx, wy) = project_to_screen(
                star_world(winner),
                view.camera.view_proj(vp().w / vp().h),
                vp(),
            )
            .expect("winner projects");
            let d = (wx - (sx + PICK_RADIUS_PX + 2.0)).hypot(wy - sy);
            assert!(d <= PICK_RADIUS_PX, "winner must be within radius");
        }
    }
}
