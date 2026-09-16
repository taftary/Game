//! Galaxy-map view state: camera, seed-driven sprites, selection.
//!
//! Pure + headless — f64 map math lives here, GPU upload in the binary.
//! Top-down ENU map: east (+x) → screen right, north (+z) → screen up,
//! NDC +1 = the top row (same convention as
//! [`picking::ray_from_cursor`](crate::picking::ray_from_cursor)).
//!
//! The journey machine (`game::journey`) is NOT wired here yet: this
//! screen owns a local selection; UMAP-014/015 (SystemMap + travel)
//! promote selection into journey events.

use game_engine::core::SeededRng;
use game_engine::universe::{
    DEFAULT_STAR_COUNT, GALAXY_RADIUS_LY, GalaxyDescriptor, SpectralClass, generate_galaxy,
};

use crate::ui::TextField;

/// Seed the map opens on (overridden by `--seed`; the viewer panel
/// field edits it at runtime).
pub const DEFAULT_GALAXY_SEED: u64 = 1234;

/// Closest the camera may zoom: half-extent in compressed ly.
pub const MIN_VIEW_RADIUS_LY: f64 = 500.0;

/// Widest the camera may zoom out: the full disk plus a margin.
pub const MAX_VIEW_RADIUS_LY: f64 = GALAXY_RADIUS_LY * 1.1;

/// Nebula impostor count (OQ-2 billboards; drawn as soft points).
pub const DEFAULT_NEBULA_COUNT: usize = 28;

/// L1 cosmic-web backdrop sprite count (generated sprite field, OQ-5).
pub const DEFAULT_BACKDROP_COUNT: usize = 400;

/// Click-select radius in screen pixels (converted to ly at pick time).
pub const PICK_RADIUS_PX: f32 = 8.0;

/// f64 map camera: center (x, z) + half-height extent. Zoom is
/// multiplicative on the extent; pan is in map units. Both clamp.
#[derive(Clone, Debug, PartialEq)]
pub struct GalaxyCamera {
    pub center_x: f64,
    pub center_z: f64,
    /// Half-extent of the visible z range; x half-extent scales by
    /// viewport aspect so light-years stay square.
    pub view_radius: f64,
}

impl GalaxyCamera {
    /// Whole disk in view.
    pub fn full_galaxy() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            view_radius: MAX_VIEW_RADIUS_LY,
        }
    }

    /// Zoom: `factor < 1` zooms in. Clamped to
    /// [`MIN_VIEW_RADIUS_LY`]..=[`MAX_VIEW_RADIUS_LY`].
    pub fn zoom_by(&mut self, factor: f64) {
        self.view_radius =
            (self.view_radius * factor).clamp(MIN_VIEW_RADIUS_LY, MAX_VIEW_RADIUS_LY);
    }

    /// Pan by map-unit deltas. The center clamps to the disk plus a
    /// half-view margin, so the galaxy can never be lost off-screen.
    pub fn pan_by(&mut self, dx: f64, dz: f64) {
        let margin = GALAXY_RADIUS_LY + self.view_radius;
        self.center_x = (self.center_x + dx).clamp(-margin, margin);
        self.center_z = (self.center_z + dz).clamp(-margin, margin);
    }

    /// Compressed ly per screen pixel (x and z agree by construction:
    /// the x half-extent scales by viewport aspect).
    pub fn ly_per_pixel(&self, _vp_w: f32, vp_h: f32) -> f64 {
        2.0 * self.view_radius / f64::from(vp_h.max(1.0))
    }

    /// Map (x, z) → y-down viewport pixels. `vp` is (x, y, w, h).
    /// North (+z) maps up — the y-down flip lives in this one line.
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
    /// [`GalaxyCamera::map_to_screen`] (round-trip tested).
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

    /// Ortho MVP over map space for the point pipeline, column-major
    /// `[[f32; 4]; 4]` (matches `Mat4::to_cols_array_2d` uploads
    /// elsewhere): map (x, z) → NDC with +1 = top (north up). Depth
    /// passes through; points carry no z.
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

/// Nearest star to map point (x, z) within `threshold` ly. Exact ties
/// keep the lowest star index (same rule as chunk picking).
pub fn pick_star(
    stars: &[game_engine::universe::StarDescriptor],
    x: f64,
    z: f64,
    threshold: f64,
) -> Option<u32> {
    let mut best: Option<(u32, f64)> = None;
    for star in stars {
        let dx = star.position_ly[0] - x;
        let dz = star.position_ly[2] - z;
        let d = dx * dx + dz * dz;
        if d <= threshold * threshold
            && best.is_none_or(|(bi, bd)| d < bd || (d == bd && star.star_index < bi))
        {
            best = Some((star.star_index, d));
        }
    }
    best.map(|(i, _)| i)
}

/// One nebula impostor (OQ-2 billboard): map-space disk, tint, alpha.
/// Derived from the galaxy seed — no baked assets, no hardcoding.
#[derive(Clone, Debug, PartialEq)]
pub struct NebulaSprite {
    pub x: f64,
    pub z: f64,
    pub radius: f64,
    pub tint: [f32; 3],
    pub alpha: f32,
}

/// L1 cosmic-web backdrop sprite (OQ-5 generated sprite field): dim far
/// points over an extended field. Map-space like everything else — the
/// "far" read comes from dimness, not a separate projection (L1 is
/// backdrop-only; motion fidelity is not a v1 requirement).
#[derive(Clone, Debug, PartialEq)]
pub struct BackdropSprite {
    pub x: f64,
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

/// Derive `count` L1 backdrop sprites from `seed` over a 3× field.
pub fn backdrop_sprites(seed: u64, count: usize) -> Vec<BackdropSprite> {
    let mut rng = SeededRng::stream(seed, "galaxy/backdrop");
    (0..count)
        .map(|_| BackdropSprite {
            x: rng.range_f64(-1.5, 1.5) * GALAXY_RADIUS_LY,
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
    pub camera: GalaxyCamera,
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
            camera: GalaxyCamera::full_galaxy(),
            nebulae: nebula_sprites(seed, DEFAULT_NEBULA_COUNT),
            backdrop: backdrop_sprites(seed, DEFAULT_BACKDROP_COUNT),
            selected: None,
            seed_field: TextField::new(&seed.to_string()),
        };
        view.regenerate(seed);
        view
    }

    /// Re-roll everything for `seed` (camera resets to full disk,
    /// selection clears, field mirrors the seed).
    pub fn regenerate(&mut self, seed: u64) {
        self.seed = seed;
        self.galaxy = generate_galaxy(seed, DEFAULT_STAR_COUNT);
        self.camera = GalaxyCamera::full_galaxy();
        self.nebulae = nebula_sprites(seed, DEFAULT_NEBULA_COUNT);
        self.backdrop = backdrop_sprites(seed, DEFAULT_BACKDROP_COUNT);
        self.selected = None;
        self.seed_field = TextField::new(&seed.to_string());
    }

    /// Click-select: viewport cursor → map point → nearest star within
    /// [`PICK_RADIUS_PX`]. Stores and returns the pick.
    pub fn select_at(&mut self, cursor: (f32, f32), vp: (f32, f32, f32, f32)) -> Option<u32> {
        let (x, z) = self.camera.screen_to_map(cursor.0, cursor.1, vp);
        let threshold = f64::from(PICK_RADIUS_PX) * self.camera.ly_per_pixel(vp.2, vp.3);
        self.selected = pick_star(&self.galaxy.stars, x, z, threshold);
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VP: (f32, f32, f32, f32) = (0.0, 0.0, 800.0, 600.0);

    #[test]
    fn screen_map_round_trips() {
        let cam = GalaxyCamera::full_galaxy();
        // Through f32 pixels: the honest bound is one pixel in ly.
        let tol = cam.ly_per_pixel(VP.2, VP.3);
        for (x, z) in [(0.0, 0.0), (12_000.0, -4_500.0), (-49_000.0, 49_000.0)] {
            let (sx, sy) = cam.map_to_screen(x, z, VP);
            let (rx, rz) = cam.screen_to_map(sx, sy, VP);
            assert!((rx - x).abs() < tol, "{x} -> {rx}");
            assert!((rz - z).abs() < tol, "{z} -> {rz}");
        }
    }

    #[test]
    fn north_is_up_east_is_right() {
        let cam = GalaxyCamera::full_galaxy();
        let (cx, cy) = cam.map_to_screen(0.0, 0.0, VP);
        assert_eq!((cx, cy), (400.0, 300.0));
        let (ex, ey) = cam.map_to_screen(10_000.0, 0.0, VP);
        assert!(ex > cx && ey == cy);
        let (nx, ny) = cam.map_to_screen(0.0, 10_000.0, VP);
        assert!(nx == cx && ny < cy);
    }

    #[test]
    fn mvp_agrees_with_screen_math() {
        // MVP maps the camera center to NDC origin and north to +y.
        let cam = GalaxyCamera::full_galaxy();
        let m = cam.mvp(800.0, 600.0);
        let apply = |x: f32, z: f32| (m[0][0] * x + m[3][0], m[1][1] * z + m[3][1]);
        let (nx, ny) = apply(0.0, 0.0);
        assert!(nx.abs() < 1e-6 && ny.abs() < 1e-6);
        let (_, py) = apply(0.0, 10_000.0);
        assert!(py > 0.0, "north must be +y NDC");
        let (px, _) = apply(10_000.0, 0.0);
        assert!(px > 0.0, "east must be +x NDC");
        // Square ly: a 10k-ly east step and a 10k-ly north step cover
        // the same NDC distance on a square viewport.
        let sq = GalaxyCamera::full_galaxy().mvp(600.0, 600.0);
        let ex = (sq[0][0] * 10_000.0 + sq[3][0]).abs();
        let no = (sq[1][1] * 10_000.0 + sq[3][1]).abs();
        assert!((ex - no).abs() < 1e-6, "{ex} vs {no}");
    }

    #[test]
    fn zoom_and_pan_clamp() {
        let mut cam = GalaxyCamera::full_galaxy();
        cam.zoom_by(0.0);
        assert_eq!(cam.view_radius, MIN_VIEW_RADIUS_LY);
        cam.zoom_by(1e9);
        assert_eq!(cam.view_radius, MAX_VIEW_RADIUS_LY);
        cam.pan_by(1e12, -1e12);
        assert!(cam.center_x <= GALAXY_RADIUS_LY + cam.view_radius);
        assert!(cam.center_z >= -GALAXY_RADIUS_LY - cam.view_radius);
    }

    #[test]
    fn pick_nearest_lowest_index_wins_ties_and_misses() {
        let galaxy = generate_galaxy(42, 3);
        let a = &galaxy.stars[0];
        // Exact hit on star 0's position.
        assert_eq!(
            pick_star(&galaxy.stars, a.position_ly[0], a.position_ly[2], 1.0),
            Some(0)
        );
        // Empty point far outside the disk misses.
        assert_eq!(pick_star(&galaxy.stars, 1e9, 1e9, 1_000.0), None);
    }

    #[test]
    fn sprites_are_seed_stable() {
        assert_eq!(nebula_sprites(7, 10), nebula_sprites(7, 10));
        assert_ne!(nebula_sprites(7, 10), nebula_sprites(8, 10));
        assert_eq!(backdrop_sprites(7, 50), backdrop_sprites(7, 50));
        for sprite in nebula_sprites(7, DEFAULT_NEBULA_COUNT) {
            assert!((0.05..=0.12).contains(&sprite.alpha));
            let r = (sprite.x * sprite.x + sprite.z * sprite.z).sqrt();
            assert!(r <= GALAXY_RADIUS_LY);
        }
    }

    #[test]
    fn view_regenerate_resets_everything() {
        let mut view = GalaxyMapView::new(1);
        view.selected = Some(5);
        view.camera.zoom_by(0.01);
        view.regenerate(2);
        assert_eq!(view.seed, 2);
        assert_eq!(view.selected, None);
        assert_eq!(view.camera, GalaxyCamera::full_galaxy());
        assert_eq!(view.galaxy.stars.len(), DEFAULT_STAR_COUNT as usize);
        assert_eq!(view.seed_field.text, "2");
    }

    #[test]
    fn select_at_picks_the_projected_star() {
        let mut view = GalaxyMapView::new(3);
        let star = &view.galaxy.stars[0];
        let (sx, sy) = view
            .camera
            .map_to_screen(star.position_ly[0], star.position_ly[2], VP);
        assert_eq!(view.select_at((sx, sy), VP), Some(0));
        assert_eq!(view.selected, Some(0));
        assert_eq!(view.select_at((0.0, 0.0), VP), view.selected);
    }
}
