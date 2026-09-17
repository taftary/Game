//! Catalog star layer for the Backdrop depth band (spec §4/§9,
//! ADR-017).
//!
//! Pure CPU-side expansion: resident tiles plus fallback slots become
//! point-sprite vertices in camera-relative `f32` (via [`recenter`],
//! ADR-013), so the GPU buffer only changes when the tile set changes —
//! camera motion rides the MVP push, never the buffer (the `upload_map`
//! pattern). The binaries draw these points through their Backdrop
//! `PointList` pipeline ahead of content passes.
//!
//! Sky frame (v0.2.0): world axes **are** J2000 equatorial axes — +X the
//! vernal equinox, +Y ra 90° eastward, +Z the north celestial pole
//! (right-handed: X × Y = Z). [`equatorial_to_world`] and
//! [`WORLD_TO_EQUATORIAL`] own the mapping `zodiacal.rs` deferred here;
//! aligning a future SolarSystem-frame ecliptic tilt is a parameter
//! change at this boundary, never a rewrite downstream.
//!
//! Presentation photometry ([`spectral_color`], [`mag_to_size_px`]) is
//! an approximation for readability; absolute calibration belongs to
//! `exposure-tone-mapping` (recorded handoff, same pattern as
//! `zodiacal-light`).
//!
//! ```
//! use game_engine::render::stars::{WORLD_TO_EQUATORIAL, equatorial_to_world};
//! use glam::DVec3;
//!
//! // Vernal equinox → +X, north celestial pole → +Z.
//! let x = equatorial_to_world(0.0, 0.0).expect("valid");
//! assert!((x - DVec3::X).length() < 1e-12);
//! let z = equatorial_to_world(0.0, std::f64::consts::FRAC_PI_2).expect("valid");
//! assert!((z - DVec3::Z).length() < 1e-12);
//! // The scheduler seam: rotation is orthonormal.
//! assert!((WORLD_TO_EQUATORIAL * WORLD_TO_EQUATORIAL.transpose()
//!     - glam::DMat3::IDENTITY).abs_diff_eq(glam::DMat3::ZERO, 1e-15));
//! ```

use crate::catalog::fallback::FallbackStar;
use crate::catalog::fallback::TileMeta;
use crate::catalog::format::{DecodedTile, StarRecord};
use crate::frames::recenter;
use glam::{DMat3, DVec3, Vec3};

/// World→equatorial rotation for the scheduler seam. Identity under the
/// v0.2.0 sky frame (world axes are equatorial axes); kept explicit so
/// a future frame alignment flows through one constant.
pub static WORLD_TO_EQUATORIAL: DMat3 = DMat3::IDENTITY;

/// Default star-shell radius in caller world units (inside the 1000.0
/// planet-view far plane, far outside planet content).
pub const SKY_SHELL_RADIUS: f64 = 900.0;
/// Fallback→catalog crossfade duration in milliseconds (UX consult:
/// hides the swap pop; reuses the `FxState` fade pattern).
pub const CROSSFADE_MS: f64 = 300.0;

/// J2000 equatorial (`ra_rad`, `dec_rad`) → v0.2.0 sky-frame world unit
/// direction. Returns `None` for non-finite or out-of-range inputs.
pub fn equatorial_to_world(ra_rad: f64, dec_rad: f64) -> Option<DVec3> {
    if !ra_rad.is_finite() || !dec_rad.is_finite() {
        return None;
    }
    if !(0.0..=2.0 * std::f64::consts::PI).contains(&ra_rad)
        || !(-std::f64::consts::FRAC_PI_2..=std::f64::consts::FRAC_PI_2).contains(&dec_rad)
    {
        return None;
    }
    let (sra, cra) = ra_rad.sin_cos();
    let (sdec, cdec) = dec_rad.sin_cos();
    Some(DVec3::new(cdec * cra, cdec * sra, sdec))
}

/// Sky-frame world unit direction → J2000 equatorial (`ra`, `dec`).
/// Inverse of [`equatorial_to_world`]; `None` for non-finite or
/// near-zero inputs.
pub fn world_to_equatorial(dir: DVec3) -> Option<(f64, f64)> {
    if !dir.is_finite() {
        return None;
    }
    let unit = dir.try_normalize()?;
    Some((
        unit.y.atan2(unit.x).rem_euclid(2.0 * std::f64::consts::PI),
        unit.z.clamp(-1.0, 1.0).asin(),
    ))
}

/// Approximate star color from Gaia BP–RP: blue-white → white → orange
/// → red ramp. Presentation only (see module docs).
pub fn spectral_color(bp_rp: f32) -> [f32; 3] {
    // Anchors: −0.5 blue-white, 0.8 white, 2.0 orange, 4.0 red.
    const STOPS: [(f32, [f32; 3]); 4] = [
        (-0.5, [0.70, 0.85, 1.00]),
        (0.8, [1.00, 1.00, 1.00]),
        (2.0, [1.00, 0.70, 0.40]),
        (4.0, [1.00, 0.40, 0.30]),
    ];
    let clamped = bp_rp.clamp(STOPS[0].0, STOPS[3].0);
    for window in STOPS.windows(2) {
        let (lo, chi) = (window[0].0, window[1].0);
        if clamped <= chi {
            let t = (clamped - lo) / (chi - lo);
            let (a, b) = (window[0].1, window[1].1);
            return [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ];
        }
    }
    STOPS[3].1
}

/// G magnitude → point-sprite pixel size (bright 3.5 px → faint 1 px).
pub fn mag_to_size_px(g_mag: f32) -> f32 {
    (3.5 - 0.18 * (g_mag - 6.0)).clamp(1.0, 3.5)
}

/// Crossfade alpha for a replacement of age `age_ms`: smoothstep 0→1
/// over [`CROSSFADE_MS`].
pub fn crossfade_alpha(age_ms: f64) -> f32 {
    if !age_ms.is_finite() || age_ms <= 0.0 {
        return 0.0;
    }
    let t = (age_ms / CROSSFADE_MS).min(1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

/// One camera-relative point sprite: shell-projected position, tint,
/// pixel size, alpha (maps 1:1 onto the debug `MapVertex` layout).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarPoint {
    /// Camera-relative `f32` position (shell around the camera).
    pub pos: [f32; 3],
    /// Tint from [`spectral_color`].
    pub color: [f32; 3],
    /// Pixel size from [`mag_to_size_px`].
    pub size_px: f32,
    /// Alpha (crossfade × caller dimming).
    pub alpha: f32,
}

fn expand(
    ra_rad: f64,
    dec_rad: f64,
    g_mag: f32,
    bp_rp: f32,
    camera_world: DVec3,
    shell: f64,
    alpha: f32,
) -> Option<StarPoint> {
    if !shell.is_finite() || shell <= 0.0 || !alpha.is_finite() {
        return None;
    }
    let dir = equatorial_to_world(ra_rad, dec_rad)?;
    let world = camera_world + dir * shell;
    let rel: Vec3 = recenter(world, camera_world);
    Some(StarPoint {
        pos: rel.into(),
        color: spectral_color(bp_rp),
        size_px: mag_to_size_px(g_mag),
        alpha: alpha.clamp(0.0, 1.0),
    })
}

/// Expand one catalog record to a point sprite.
pub fn expand_catalog_record(
    star: &StarRecord,
    camera_world: DVec3,
    shell: f64,
    alpha: f32,
) -> Option<StarPoint> {
    expand(
        star.ra_rad,
        star.dec_rad,
        star.g_mag,
        star.bp_rp,
        camera_world,
        shell,
        alpha,
    )
}

/// Expand one fallback lattice star to a point sprite.
pub fn expand_fallback_star(
    star: &FallbackStar,
    camera_world: DVec3,
    shell: f64,
    alpha: f32,
) -> Option<StarPoint> {
    expand(
        star.ra_rad,
        star.dec_rad,
        star.g_mag,
        star.bp_rp,
        camera_world,
        shell,
        alpha,
    )
}

/// Expand a whole resident tile (catalog path).
pub fn expand_tile_catalog(
    tile: &DecodedTile,
    camera_world: DVec3,
    shell: f64,
    alpha: f32,
) -> Vec<StarPoint> {
    tile.stars
        .iter()
        .filter_map(|star| expand_catalog_record(star, camera_world, shell, alpha))
        .collect()
}

/// Expand a whole fallback tile (lattice path over `meta.count`
/// slots). The full deterministic set — callers own budgets (the
/// debug `CatalogSky` strides huge model tiles; see its tier gate).
pub fn expand_tile_fallback(
    master_seed: u64,
    order: u8,
    pixel: u64,
    meta: &TileMeta,
    camera_world: DVec3,
    shell: f64,
    alpha: f32,
) -> Vec<StarPoint> {
    (0..meta.count)
        .filter_map(|slot| {
            crate::catalog::fallback::fallback_star(master_seed, order, pixel, slot, meta)
        })
        .filter_map(|star| expand_fallback_star(&star, camera_world, shell, alpha))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::format::{CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, TileHeader};
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

    #[test]
    fn frame_axes_match_equatorial_anchors() {
        // Equinox → +X, ra 90° → +Y, NCP → +Z (right-handed).
        let x = equatorial_to_world(0.0, 0.0).expect("valid");
        assert!((x - DVec3::X).length() < 1e-12);
        let y = equatorial_to_world(FRAC_PI_2, 0.0).expect("valid");
        assert!((y - DVec3::Y).length() < 1e-12);
        let z = equatorial_to_world(0.0, FRAC_PI_2).expect("valid");
        assert!((z - DVec3::Z).length() < 1e-12);
        assert!((x.cross(y) - z).length() < 1e-12);
        // Round-trip through the inverse.
        for (ra, dec) in [(0.3, 0.1), (5.9, -0.7), (3.0, 1.2)] {
            let dir = equatorial_to_world(ra, dec).expect("valid");
            let (ra2, dec2) = world_to_equatorial(dir).expect("valid");
            assert!((ra - ra2).abs() < 1e-12);
            assert!((dec - dec2).abs() < 1e-12);
        }
        // Invalid inputs rejected.
        assert_eq!(equatorial_to_world(f64::NAN, 0.0), None);
        assert_eq!(equatorial_to_world(0.0, 2.0), None);
        assert_eq!(equatorial_to_world(-0.1, 0.0), None);
        assert_eq!(world_to_equatorial(DVec3::ZERO), None);
    }

    #[test]
    fn expansion_sits_on_the_shell_around_the_camera() {
        let camera = DVec3::new(10.0, -3.0, 7.0);
        let tile = DecodedTile {
            header: TileHeader {
                order: 4,
                pixel: 9,
                mean_g_mag: 12.0,
                mean_bp_rp: 0.9,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            stars: vec![StarRecord {
                source_id: 1,
                ra_rad: FRAC_PI_4,
                dec_rad: 0.2,
                pm_ra_mas_yr: 0.0,
                pm_dec_mas_yr: 0.0,
                g_mag: 6.0,
                bp_rp: -0.4,
            }],
        };
        let points = expand_tile_catalog(&tile, camera, 900.0, 1.0);
        assert_eq!(points.len(), 1);
        let p = Vec3::from_array(points[0].pos);
        // Camera-relative shell radius, blue-white bright star.
        assert!((p.length() - 900.0).abs() < 0.01);
        assert_eq!(points[0].size_px, 3.5);
        assert!(points[0].color[2] > points[0].color[0]);
        // Far-from-origin cameras keep precision (recenter path).
        let far = expand_tile_catalog(&tile, DVec3::new(1e11, 0.0, 0.0), 900.0, 1.0);
        assert!((Vec3::from_array(far[0].pos).length() - 900.0).abs() < 0.01);
    }

    #[test]
    fn crossfade_ramps_over_300ms() {
        assert_eq!(crossfade_alpha(-1.0), 0.0);
        assert_eq!(crossfade_alpha(0.0), 0.0);
        let mid = crossfade_alpha(150.0);
        assert!((mid - 0.5).abs() < 1e-6);
        assert_eq!(crossfade_alpha(300.0), 1.0);
        assert_eq!(crossfade_alpha(9999.0), 1.0);
    }

    #[test]
    fn color_ramp_hits_its_anchors() {
        assert_eq!(spectral_color(-0.5), [0.70, 0.85, 1.00]);
        assert_eq!(spectral_color(0.8), [1.00, 1.00, 1.00]);
        assert_eq!(spectral_color(4.0), [1.00, 0.40, 0.30]);
        assert_eq!(spectral_color(99.0), spectral_color(4.0));
    }
}
