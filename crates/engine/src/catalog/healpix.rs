//! HEALPix NESTED spatial index, orders 0–14 (spec §4, ADR-017).
//!
//! Equal-area, iso-latitude, hierarchical tiling of the sphere: order 12
//! (`nside` 4096, 2.0×10⁸ pixels, 51.5″) is the default catalog tier,
//! order 13 covers the dense galactic plane, order 14 premium close-ups.
//! Nested ids are hierarchical by construction — the subtree of a pixel
//! is one contiguous id range — so the Phase-3 scheduler works with
//! range sets, never pixel enumeration.
//!
//! Algorithm reference: Górski et al. 2005 ("HEALPix: A Framework for
//! High-Resolution Discretization…", ApJ 622:759) and Calabretta &
//! Roukema 2007 (MNRAS 381:865) for the projection; the
//! base-cell/edge-index formulation follows the CDS `cdshealpix`
//! implementation (MIT/Apache-2.0). This file is an original
//! implementation from that math, not a port: `(lon, lat)` maps to a
//! base cell plus local diamond coordinates, which fold to the
//! `(i, j)` grid each base cell subdivides into; the nested id is the
//! base cell plus the Morton-interleaved `(i, j)` bits (`i` in even bit
//! positions, `j` in odd — the standard Morton order).
//!
//! Conventions: angles in radians; `ra` (longitude) normalized to
//! `[0, 2π)`, `dec` (latitude) in `[-π/2, π/2]`; pixel ids are nested
//! ids at the stated order (`face << 2·order | morton(i, j)`).
//!
//! Cross-checks against independent implementations (see tests):
//! the CDS doctest vector (depth 12, lon 12.5°, lat 89.99999° →
//! `nside² − 1`), base-cell centers/vertices from the CDS projection
//! doctests, and nested↔ring agreement at depth 1 via the published
//! `to_ring` table plus standard ring geometry (pins the Morton bit
//! orientation: pixel 17 is the east child of base cell 4).
//!
//! ```
//! use game_engine::catalog::healpix::{ang2pix, npix, pix2ang};
//!
//! assert_eq!(npix(12), Some(201_326_592));
//! // The north-polar corner of base cell 0 round-trips.
//! let pixel = ang2pix(1, 0.0, 1.0).expect("valid coords");
//! let (ra, dec) = pix2ang(1, pixel).expect("valid pixel");
//! assert_eq!(ang2pix(1, ra, dec), Some(pixel));
//! ```

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::ops::Range;

/// Largest supported order: 14 (`nside` 16384, 3.2×10⁹ pixels).
/// Higher orders exist in the HEALPix standard but exceed the catalog
/// tiers in ADR-017; the id math stays in `u64` throughout.
pub const MAX_ORDER: u8 = 14;

/// Latitude of the equatorial/polar transition, `asin(2/3)`
/// (≈ 41.81°; Eq. (1) in Górski et al. 2005).
pub const TRANSITION_LATITUDE: f64 = 0.729_727_656_226_966_3;
/// `|sin(dec)|` at the equatorial/polar transition (2/3).
pub const TRANSITION_Z: f64 = 2.0 / 3.0;
/// Arcseconds per radian (pixel-resolution reporting).
pub const ARCSEC_PER_RAD: f64 = 206_264.806_247_096_36;

/// Cells per base-cell edge at `order` (`2^order`).
///
/// Returns `None` for `order > MAX_ORDER`.
pub fn nside(order: u8) -> Option<u32> {
    if order > MAX_ORDER {
        return None;
    }
    Some(1u32 << order)
}

/// Total pixel count at `order` (`12 · nside²`).
///
/// Returns `None` for `order > MAX_ORDER`.
///
/// ```
/// use game_engine::catalog::healpix::npix;
///
/// assert_eq!(npix(0), Some(12));
/// assert_eq!(npix(12), Some(201_326_592));
/// ```
pub fn npix(order: u8) -> Option<u64> {
    nside(order).map(|n| 12u64 * n as u64 * n as u64)
}

/// Solid angle of one pixel at `order` (steradians, `π / 3·nside²`).
///
/// Returns `None` for `order > MAX_ORDER`.
pub fn pixel_area_sr(order: u8) -> Option<f64> {
    nside(order).map(|n| PI / (3.0 * n as f64 * n as f64))
}

/// Nominal pixel resolution at `order`: square root of the pixel solid
/// angle, in arcseconds (≈ 51.5″ at order 12 per the spec §4 table).
///
/// Returns `None` for `order > MAX_ORDER`.
pub fn resolution_arcsec(order: u8) -> Option<f64> {
    pixel_area_sr(order).map(|area| area.sqrt() * ARCSEC_PER_RAD)
}

/// Nested pixel id of (`ra_rad`, `dec_rad`) at `order`.
///
/// Returns `None` for out-of-range orders, non-finite inputs, or
/// `|dec| > π/2`. The poles map deterministically into the base cell of
/// the `ra` quarter (no branch on `ra` needed downstream).
///
/// ```
/// use game_engine::catalog::healpix::ang2pix;
///
/// // Equator at ra = 0 sits in base cell 4.
/// assert_eq!(ang2pix(0, 0.0, 0.0), Some(4));
/// ```
pub fn ang2pix(order: u8, ra_rad: f64, dec_rad: f64) -> Option<u64> {
    let n = nside(order)? as f64;
    if !ra_rad.is_finite() || !dec_rad.is_finite() {
        return None;
    }
    if dec_rad.abs() > FRAC_PI_2 {
        return None;
    }
    let (face, x, y) = base_and_local(ra_rad.rem_euclid(2.0 * PI), dec_rad);
    // Local diamond coords → grid indices (CDS `hash`: `i` tracks
    // `(y + x) / 2`, `j` tracks `(y − x) / 2`, scaled by `nside`).
    // Boundary float dust clamps into range; saturation (never wrap)
    // keeps edge points in their edge cell.
    let i = (((y + x) * 0.5 * n).floor() as i64).clamp(0, n as i64 - 1) as u32;
    let j = (((y - x) * 0.5 * n).floor() as i64).clamp(0, n as i64 - 1) as u32;
    Some((u64::from(face) << (2 * order)) | morton_interleave(order, i, j))
}

/// Center (`ra_rad`, `dec_rad`) of `pixel` at `order`, `ra ∈ [0, 2π)`.
///
/// Returns `None` for out-of-range orders or `pixel ≥ npix(order)`.
///
/// ```
/// use game_engine::catalog::healpix::pix2ang;
/// use std::f64::consts::FRAC_PI_4;
///
/// // Base cell 0 centers on (π/4, asin(2/3)) — CDS projection doctest.
/// let (ra, dec) = pix2ang(0, 0).expect("valid pixel");
/// assert!((ra - FRAC_PI_4).abs() < 1e-12);
/// assert!((dec - (2.0f64 / 3.0).asin()).abs() < 1e-12);
/// // Base cell 4 centers on the equator at ra = 0.
/// let (ra4, dec4) = pix2ang(0, 4).expect("valid pixel");
/// assert!(ra4.abs() < 1e-12 && dec4.abs() < 1e-12);
/// ```
pub fn pix2ang(order: u8, pixel: u64) -> Option<(f64, f64)> {
    let (x, y) = projected_center(order, pixel)?;
    unproj(x, y)
}

/// South, east, north, west corner coordinates of `pixel` at `order`
/// (each `(ra_rad, dec_rad)`, `ra ∈ [0, 2π)`).
///
/// Returns `None` for out-of-range orders or `pixel ≥ npix(order)`.
pub fn pixel_corners(order: u8, pixel: u64) -> Option<[(f64, f64); 4]> {
    let [s, e, nn, w] = projected_corners(order, pixel)?;
    Some([
        unproj(s.0, s.1)?,
        unproj(e.0, e.1)?,
        unproj(nn.0, nn.1)?,
        unproj(w.0, w.1)?,
    ])
}

/// Projected-plane corners (S, E, N, W) in the CDS 8×3 grid: the
/// scheduler's quad tests work here before unprojecting (edge midpoints
/// are exact in this space).
pub(crate) fn projected_corners(order: u8, pixel: u64) -> Option<[(f64, f64); 4]> {
    let n = nside(order)? as f64;
    let (x, y) = projected_center(order, pixel)?;
    let t = 1.0 / n;
    // Vertices sit one center-to-vertex step off the center along each
    // axis (CDS `vertices`); west wraps through `rem_euclid` in `unproj`.
    Some([(x, y - t), (x + t, y), (x, y + t), (x - t, y)])
}

/// Unit vector for (`ra_rad`, `dec_rad`) on the celestial sphere
/// (shared by the cone query and the scheduler's quad tests).
pub(crate) fn equatorial_unit(ra_rad: f64, dec_rad: f64) -> [f64; 3] {
    let (sra, cra) = ra_rad.sin_cos();
    let (sdec, cdec) = dec_rad.sin_cos();
    [cdec * cra, cdec * sra, sdec]
}

/// Direct parent id of `pixel` (order-agnostic: nested hierarchy packs
/// each finer level into the low bit pairs).
///
/// ```
/// use game_engine::catalog::healpix::{children, parent};
///
/// let kids = children(17);
/// assert!(kids.iter().all(|&k| parent(k) == 17));
/// ```
pub fn parent(pixel: u64) -> u64 {
    pixel >> 2
}

/// Four child ids of `pixel` at the next finer order.
pub fn children(pixel: u64) -> [u64; 4] {
    let base = pixel << 2;
    [base, base | 1, base | 2, base | 3]
}

/// Contiguous id range covering the whole subtree of `pixel`
/// `delta_order` levels down (nested ordering keeps subtrees
/// contiguous — the scheduler's range-set primitive).
///
/// Returns `None` for `delta_order > MAX_ORDER`.
///
/// ```
/// use game_engine::catalog::healpix::child_range;
///
/// assert_eq!(child_range(5, 1), Some(20..24));
/// ```
pub fn child_range(pixel: u64, delta_order: u8) -> Option<Range<u64>> {
    if delta_order > MAX_ORDER {
        return None;
    }
    let shift = 2 * delta_order;
    Some((pixel << shift)..((pixel + 1) << shift))
}

/// Nested id ranges at `order` covering the cone around
/// (`ra_rad`, `dec_rad`) with angular radius `radius_rad`.
///
/// Top-down subdivision from the 12 base cells with triangle-inequality
/// pruning (center distance ± corner bound): no false negatives by
/// construction, mild over-cover at ragged edges. Ranges come out
/// sorted and merged. `radius_rad ≥ π` short-circuits to the whole sky.
///
/// Returns `None` for invalid orders, coordinates, or non-positive /
/// non-finite radii.
///
/// ```
/// use game_engine::catalog::healpix::{ang2pix, tiles_in_cone};
///
/// let ranges = tiles_in_cone(4, 1.0, 0.2, 0.05).expect("valid cone");
/// // Every range is in-bounds, and the cone center's own pixel is hit.
/// let center = ang2pix(4, 1.0, 0.2).expect("valid coords");
/// assert!(ranges.iter().any(|r| r.contains(&center)));
/// ```
pub fn tiles_in_cone(
    order: u8,
    ra_rad: f64,
    dec_rad: f64,
    radius_rad: f64,
) -> Option<Vec<Range<u64>>> {
    let npix_count = npix(order)?;
    if !ra_rad.is_finite() || !dec_rad.is_finite() || !radius_rad.is_finite() {
        return None;
    }
    if dec_rad.abs() > FRAC_PI_2 || radius_rad <= 0.0 {
        return None;
    }
    if radius_rad >= PI {
        return Some(core::iter::once(0..npix_count).collect());
    }
    let center = unit_vector(ra_rad, dec_rad);
    // Seed with the 12 base cells; subdivide intersecting ones.
    let mut ranges = Vec::new();
    let mut stack: Vec<(u64, u8)> = (0..12).map(|p| (p, 0)).collect();
    while let Some((pixel, depth)) = stack.pop() {
        let (pra, pdec) = pix2ang(depth, pixel)?;
        let pcenter = unit_vector(pra, pdec);
        let corners = pixel_corners(depth, pixel)?;
        // Corner bound: farthest corner from the cell center.
        let mut bound = 0.0f64;
        for (cra, cdec) in &corners {
            bound = bound.max(ang_dist_vec(pcenter, unit_vector(*cra, *cdec)));
        }
        let dist = ang_dist_vec(center, pcenter);
        if dist + bound <= radius_rad {
            // Quad provably inside the cone: take the whole subtree.
            let shift = 2 * (order - depth);
            ranges.push((pixel << shift)..((pixel + 1) << shift));
        } else if dist > radius_rad + bound {
            // Provably outside: prune.
        } else if depth == order {
            ranges.push(pixel..(pixel + 1));
        } else {
            let base = pixel << 2;
            stack.push((base, depth + 1));
            stack.push((base | 1, depth + 1));
            stack.push((base | 2, depth + 1));
            stack.push((base | 3, depth + 1));
        }
    }
    ranges.sort_by_key(|r| r.start);
    // Merge contiguous ranges (nested order makes siblings adjacent).
    let mut merged: Vec<Range<u64>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && last.end >= range.start
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    Some(merged)
}

/// Base cell (0–11) plus local diamond coordinates for (`lon`, `lat`),
/// `lon ∈ [0, 2π)`. Local frame: origin at the base cell's south
/// vertex, x west-to-east in `[-1, 1]`, y south-to-north in `[0, 2]`
/// (CDS `d0h_lh_in_d0c`).
fn base_and_local(lon: f64, lat: f64) -> (u8, f64, f64) {
    // Longitude quarter decomposition (CDS `xpm1_and_q`, lon ≥ 0 path):
    // `lon = (x_pm1 + 1) · π/4 + q · π/2`.
    let x = lon * 4.0 / PI;
    let q_raw = x as u64 | 1;
    let x_pm1 = x - q_raw as f64;
    let q = ((q_raw & 7) >> 1) as u8;
    if lat > TRANSITION_LATITUDE {
        // North polar cap (Collignon).
        let s = 2.449_489_742_783_178 * (0.5 * lat + FRAC_PI_4).cos();
        (q, x_pm1 * s, 2.0 - s)
    } else if lat < -TRANSITION_LATITUDE {
        // South polar cap (Collignon).
        let s = 2.449_489_742_783_178 * (0.5 * lat - FRAC_PI_4).cos();
        (q + 8, x_pm1 * s, s)
    } else {
        // Equatorial belt (cylindrical equal-area). The branch-free
        // diamond fold picks the sub-quadrant origin so the S→E and
        // S→W edges belong to the cell (CDS `d0h_lh_in_d0c`).
        let y_pm1 = lat.sin() * 1.5;
        let q01 = u8::from(x_pm1 > y_pm1);
        let q12 = u8::from(x_pm1 >= -y_pm1);
        let q1 = q01 & q12;
        let q013 = q01 + (1 - q12);
        let x_proj = x_pm1 - (i16::from(q01 + q12) - 1) as f64;
        let y_proj = y_pm1 + f64::from(q013);
        let face = (q013 << 2) + ((q + q1) & 3);
        (face, x_proj, y_proj)
    }
}

/// Projected-plane center of `pixel` at `order`, in the CDS 8×3 grid
/// (`x ∈ [0, 8]`, `y ∈ [-2, 2]`).
pub(crate) fn projected_center(order: u8, pixel: u64) -> Option<(f64, f64)> {
    let n = nside(order)? as f64;
    if pixel >= npix(order)? {
        return None;
    }
    let face = (pixel >> (2 * order)) as u8;
    let local = pixel & ((1u64 << (2 * order)) - 1);
    let (i, j) = morton_deinterleave(order, local);
    // 45° rotation into diamond axes, recentered on the base cell
    // (CDS `center_of_projected_cell`).
    let hx = (i as i64 - j as i64) as f64 / n;
    let hy = (i as i64 + j as i64 - (n as i64 - 1)) as f64 / n;
    // Base-cell center offsets in the 8×3 grid.
    let div4 = face >> 2;
    let offset_y = 1 - div4 as i8;
    let mut offset_x = (face & 3) << 1;
    offset_x |= (offset_y & 1) as u8;
    let mut x = hx + f64::from(offset_x);
    if x < 0.0 {
        x += 8.0;
    }
    Some((x, hy + f64::from(offset_y)))
}

/// Inverse HEALPix projection: `(x, y)` in the 8×3 grid → (`ra`, `dec`)
/// with `ra ∈ [0, 2π)` (CDS `unproj`).
pub(crate) fn unproj(x: f64, y: f64) -> Option<(f64, f64)> {
    if !(-2.0..=2.0).contains(&y) {
        return None;
    }
    let x = x.rem_euclid(8.0);
    let floor = x.floor() as u8;
    let odd = floor | 1;
    let offset = odd & 7;
    let mut lon = x - f64::from(odd);
    let mut lat = y.abs();
    if lat <= 1.0 {
        // Cylindrical equal-area: `y = sin(lat) · 3/2`.
        lat = (lat * TRANSITION_Z).asin();
    } else {
        // Collignon: `y = 2 − t`, `x = lon_pm1 · t`.
        let t = 2.0 - lat;
        if t > 1e-13 {
            lon = (lon / t).clamp(-1.0, 1.0);
        } else {
            lon = 0.0;
        }
        lat = 2.0 * (t * 0.408_248_290_463_863).acos() - FRAC_PI_2;
    }
    lon += f64::from(offset);
    if y.is_sign_negative() {
        lat = -lat;
    }
    Some((lon * FRAC_PI_4, lat))
}

/// Morton interleave of the `order`-bit `i` (even positions) and `j`
/// (odd positions) grid coordinates.
fn morton_interleave(order: u8, i: u32, j: u32) -> u64 {
    let mut out = 0u64;
    for k in 0..order {
        out |= (u64::from((i >> k) & 1)) << (2 * k);
        out |= (u64::from((j >> k) & 1)) << (2 * k + 1);
    }
    out
}

/// Inverse of [`morton_interleave`].
fn morton_deinterleave(order: u8, code: u64) -> (u32, u32) {
    let mut i = 0u32;
    let mut j = 0u32;
    for k in 0..order {
        i |= (((code >> (2 * k)) & 1) as u32) << k;
        j |= (((code >> (2 * k + 1)) & 1) as u32) << k;
    }
    (i, j)
}

/// Unit vector for (`ra`, `dec`) on the celestial sphere.
fn unit_vector(ra: f64, dec: f64) -> [f64; 3] {
    equatorial_unit(ra, dec)
}

/// Angular distance between two unit vectors (haversine form).
fn ang_dist_vec(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    // Haversine-equivalent via dot, guarded at the endpoints.
    if dot >= 1.0 {
        0.0
    } else if dot <= -1.0 {
        PI
    } else {
        dot.acos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_8;

    fn assert_ang_close(a: f64, b: f64, tol: f64) {
        assert!(
            (a - b).abs() <= tol,
            "angles differ: {a} vs {b} (tol {tol})"
        );
    }

    #[test]
    fn counts_and_scales_match_spec_table() {
        assert_eq!(npix(12), Some(201_326_592));
        assert_eq!(npix(13), Some(805_306_368));
        assert_eq!(npix(14), Some(3_221_225_472));
        // Spec §4 resolution column: 51.5″ / 25.8″ / 12.9″.
        for (order, expected) in [(12, 51.5), (13, 25.8), (14, 12.9)] {
            let got = resolution_arcsec(order).expect("valid order");
            assert!(
                (got - expected).abs() / expected < 0.01,
                "order {order}: {got}″ vs {expected}″"
            );
        }
        assert_eq!(nside(15), None);
        assert_eq!(npix(99), None);
    }

    #[test]
    fn base_cell_centers_match_published_geometry() {
        let trans = TRANSITION_Z.asin();
        // North polar faces center on (π/4 + k·π/2, +transition).
        for k in 0..4u64 {
            let (ra, dec) = pix2ang(0, k).expect("valid pixel");
            assert_ang_close(ra, FRAC_PI_4 + k as f64 * FRAC_PI_2, 1e-12);
            assert_ang_close(dec, trans, 1e-12);
        }
        // Equatorial faces center on (k·π/2, 0).
        for k in 0..4u64 {
            let (ra, dec) = pix2ang(0, 4 + k).expect("valid pixel");
            assert_ang_close(ra, k as f64 * FRAC_PI_2, 1e-12);
            assert_ang_close(dec, 0.0, 1e-12);
        }
        // South polar faces center on (π/4 + k·π/2, −transition).
        for k in 0..4u64 {
            let (ra, dec) = pix2ang(0, 8 + k).expect("valid pixel");
            assert_ang_close(ra, FRAC_PI_4 + k as f64 * FRAC_PI_2, 1e-12);
            assert_ang_close(dec, -trans, 1e-12);
        }
    }

    #[test]
    fn base_cell_vertices_match_cds_projection_doctests() {
        // CDS `vertices` doctest: south vertex of base cell 0 is
        // (π/4, 0); `base_cell_from_proj_coo` pins the grid layout.
        let [s, e, n, w] = pixel_corners(0, 0).expect("valid pixel");
        assert_ang_close(s.0, FRAC_PI_4, 1e-12);
        assert_ang_close(s.1, 0.0, 1e-12);
        assert_ang_close(e.0, FRAC_PI_2, 1e-12);
        assert_ang_close(n.1, FRAC_PI_2, 1e-9);
        assert_ang_close(w.0, 0.0, 1e-12);
    }

    #[test]
    fn golden_pole_vector_matches_cdshealpix() {
        // CDS README example: depth 12, lon 12.5°, lat 89.99999° sits in
        // the last sub-pixel of base cell 0.
        let pixel =
            ang2pix(12, 12.5_f64.to_radians(), 89.99999_f64.to_radians()).expect("valid coords");
        assert_eq!(pixel, 4096u64 * 4096 - 1);
    }

    #[test]
    fn depth1_bit_orientation_matches_ring_crosscheck() {
        // CDS `to_ring` table at depth 1: nested 17 → ring 20, nested 16
        // → ring 28, nested 0 → ring 13. Standard ring geometry at
        // nside 2 (5 equatorial rings of 8 over z ∈ [−2/3, 2/3]) puts
        // ring 20–27 on z ∈ [−2/15, 2/15], 28–35 on [−0.4, −0.133],
        // 12–19 on [0.133, 0.4]: the east child of base cell 4
        // (lon π/8, lat 0) must be nested 17, its south child
        // (lon 0, lat −asin(1/3)) nested 16, and the south child of
        // base cell 0 (lon π/4, lat asin(1/3)) nested 0. This pins the
        // Morton bit orientation (i even, j odd).
        assert_eq!(ang2pix(1, FRAC_PI_8, 0.0), Some(17));
        assert_eq!(ang2pix(1, 0.0, -(1.0f64 / 3.0).asin()), Some(16));
        assert_eq!(ang2pix(1, FRAC_PI_4, (1.0f64 / 3.0).asin()), Some(0));
        // And the pole corner of base cell 0 is the all-ones child.
        assert_eq!(ang2pix(1, FRAC_PI_4, FRAC_PI_2), Some(3));
    }

    #[test]
    fn centers_round_trip_at_low_orders() {
        for order in 0..=3u8 {
            let count = npix(order).expect("valid order");
            for pixel in 0..count {
                let (ra, dec) = pix2ang(order, pixel).expect("valid pixel");
                assert_eq!(
                    ang2pix(order, ra, dec),
                    Some(pixel),
                    "round-trip failed at order {order} pixel {pixel}"
                );
            }
        }
    }

    #[test]
    fn centers_round_trip_strided_at_high_orders() {
        // Full sweep is 200M pixels at order 12: stride the space.
        for order in [4u8, 8, 12, 14] {
            let count = npix(order).expect("valid order");
            let stride = (count / 4096).max(1);
            let mut tested = 0u64;
            let mut pixel = 0u64;
            while pixel < count {
                let (ra, dec) = pix2ang(order, pixel).expect("valid pixel");
                assert_eq!(ang2pix(order, ra, dec), Some(pixel));
                tested += 1;
                pixel += stride;
            }
            assert!(tested >= 1000, "order {order}: only {tested} samples");
        }
    }

    #[test]
    fn hierarchy_ranges_partition_cleanly() {
        assert_eq!(parent(17), 4);
        assert_eq!(children(4), [16, 17, 18, 19]);
        assert_eq!(child_range(5, 1), Some(20..24));
        // Subtree ranges nest: order-1 cell 4 spans 16 order-2 cells.
        let sub = child_range(4, 1).expect("valid range");
        assert_eq!(sub.end - sub.start, 4);
        for pixel in sub {
            assert_eq!(parent(pixel), 4);
        }
        // Base cell 4's order-12 subtree is one contiguous range.
        let wide = child_range(4, 12).expect("valid range");
        assert_eq!(wide.end - wide.start, 4u64.pow(12));
        assert_eq!(child_range(0, 99), None);
    }

    #[test]
    fn adjacent_cells_share_vertices_exactly() {
        // Base cell 4 at depth 1: the south child's north vertex is the
        // base center, shared bitwise with the north child's south vertex.
        let s_child = pixel_corners(1, 16).expect("valid pixel");
        let n_child = pixel_corners(1, 19).expect("valid pixel");
        assert_eq!(s_child[2], n_child[0]);
        // Centers sit strictly inside their corner quad.
        for pixel in [16u64, 17, 18, 19] {
            let (ra, dec) = pix2ang(1, pixel).expect("valid pixel");
            let corners = pixel_corners(1, pixel).expect("valid pixel");
            let c = unit_vector(ra, dec);
            for (cra, cdec) in &corners {
                let d = ang_dist_vec(c, unit_vector(*cra, *cdec));
                assert!(d > 1e-9 && d < 0.6, "pixel {pixel}: corner distance {d}");
            }
        }
    }

    #[test]
    fn cone_query_hits_center_and_covers_samples() {
        // Small cone: center pixel present, ranges in-bounds.
        let ranges = tiles_in_cone(4, 1.0, 0.2, 0.05).expect("valid cone");
        let count = npix(4).expect("valid order");
        assert!(!ranges.is_empty());
        for r in &ranges {
            assert!(r.start < r.end && r.end <= count);
        }
        let center = ang2pix(4, 1.0, 0.2).expect("valid coords");
        assert!(ranges.iter().any(|r| r.contains(&center)));
        // No false negatives: a grid over the cone hashes into the set.
        let flat: Vec<u64> = ranges.iter().flat_map(|r| r.clone()).collect();
        assert!(flat.len() < 2000, "over-cover blowup: {}", flat.len());
        for k in 0..40 {
            let frac = k as f64 / 40.0;
            let ra = 1.0 + (frac - 0.5) * 0.09;
            let dec = 0.2 + ((k * 7) % 40) as f64 / 40.0 * 0.09 - 0.045;
            // Keep the sample inside the cone.
            let c = unit_vector(1.0, 0.2);
            let s = unit_vector(ra, dec);
            if ang_dist_vec(c, s) > 0.05 {
                continue;
            }
            let hit = ang2pix(4, ra, dec).expect("valid coords");
            assert!(flat.contains(&hit), "missed sample pixel {hit}");
        }
        // Whole-sky short-circuit: one range covering every pixel.
        let whole = tiles_in_cone(2, 0.0, 0.0, PI).expect("valid cone");
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0], 0..npix(2).expect("valid order"));
        // Invalid inputs rejected.
        assert_eq!(tiles_in_cone(99, 0.0, 0.0, 0.1), None);
        assert_eq!(tiles_in_cone(4, 0.0, 0.0, 0.0), None);
        assert_eq!(tiles_in_cone(4, 0.0, 2.0, 0.1), None);
        assert_eq!(tiles_in_cone(4, f64::NAN, 0.0, 0.1), None);
    }

    #[test]
    fn invalid_inputs_rejected() {
        assert_eq!(ang2pix(15, 0.0, 0.0), None);
        assert_eq!(ang2pix(4, f64::NAN, 0.0), None);
        assert_eq!(ang2pix(4, 0.0, f64::INFINITY), None);
        assert_eq!(ang2pix(4, 0.0, FRAC_PI_2 + 0.1), None);
        assert_eq!(pix2ang(4, npix(4).expect("valid")), None);
        assert_eq!(pix2ang(99, 0), None);
        assert_eq!(pixel_corners(4, npix(4).expect("valid")), None);
        // Poles are valid and deterministic per ra quarter.
        assert!(ang2pix(12, 0.0, FRAC_PI_2).is_some());
        assert!(ang2pix(12, 0.0, -FRAC_PI_2).is_some());
    }
}
