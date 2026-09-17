//! Streaming scheduler: view → prioritized tile ranges (spec §4).
//!
//! Pure planning, no threads, no I/O — the `bands.rs` precedent
//! ("binaries just execute the plan"): [`Scheduler::plan`] turns a
//! [`SkyView`] into sorted [`PlannedRange`]s, and the caller expands
//! ranges into loader requests within its per-frame I/O budget, inserts
//! completions into the cache, and ranks residents via
//! `note_priorities`. Replan cadence is the caller's choice (on view
//! change or throttled); `plan` itself carries no timing state besides
//! the generation counter.
//!
//! Priority order follows the spec: view-frustum tiles (3), predicted
//! travel-direction margin (2). Within a priority, ranges stay in nested
//! id order (spatially coherent load order); true angular-distance sort
//! would need per-pixel centers — i.e. enumeration — and is rejected.
//!
//! The view→sky seam: the frustum arrives in world space and rotates
//! into equatorial space through `world_to_equatorial` (a pure
//! rotation; Phase 5, `render::stars`, owns the true value — tests use
//! identity, i.e. world ≡ equatorial).
//!
//! ```
//! use game_engine::catalog::scheduler::{Scheduler, SkyView};
//! use glam::{DMat3, DMat4, DVec3};
//! use glam::dcamera::rh::proj::directx::perspective;
//! use glam::dcamera::rh::view::look_at_mat4;
//!
//! // Camera on −X looking at the origin: sees equatorial ra ≈ 0.
//! let eye = DVec3::new(-5.0, 0.0, 0.0);
//! let view = look_at_mat4(eye, DVec3::ZERO, DVec3::Y);
//! let proj = perspective(60.0_f64.to_radians(), 16.0 / 9.0, 0.05, 1000.0);
//! let mut scheduler = Scheduler::new(4, "tiles".into()).expect("valid order");
//! let plan = scheduler
//!     .plan(&SkyView {
//!         view_proj: proj * view,
//!         cam_forward_world: DVec3::X,
//!         fov_y_rad: 60.0_f64.to_radians(),
//!         aspect: 16.0 / 9.0,
//!         world_to_equatorial: DMat3::IDENTITY,
//!         velocity_world: DVec3::ZERO,
//!     })
//!     .expect("valid view");
//! assert!(!plan.ranges.is_empty());
//! // The ra ≈ 0 center tile loads at frustum priority.
//! let center = game_engine::catalog::healpix::ang2pix(4, 0.0, 0.0).expect("coords");
//! assert!(plan.ranges.iter().any(|r| r.priority == 3 && r.range.contains(&center)));
//! ```

use crate::catalog::cache::TileKey;
use crate::catalog::frustum::Frustum;
use crate::catalog::healpix::{
    MAX_ORDER, equatorial_unit, projected_center, projected_corners, resolution_arcsec,
    tiles_in_cone, unproj,
};
use glam::{DMat3, DMat4, DVec3};
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Scheduler rank of view-frustum tiles (highest).
pub const PRIORITY_FRUSTUM: u8 = 3;
/// Scheduler rank of the predicted travel-direction margin.
pub const PRIORITY_TRAVEL: u8 = 2;
/// Blend of the view axis toward the velocity heading for the travel
/// margin (0 = concentric, 1 = full heading).
const TRAVEL_BLEND: f64 = 0.25;
/// Wide-cone radius multiplier over the tight frustum cone.
const WIDE_MULTIPLIER: f64 = 1.5;

/// World-space view description for one scheduling pass.
#[derive(Clone, Copy, Debug)]
pub struct SkyView {
    /// Combined world→clip matrix (engine pipeline convention).
    pub view_proj: DMat4,
    /// Normalized world-space view direction.
    pub cam_forward_world: DVec3,
    /// Vertical field of view (rad).
    pub fov_y_rad: f64,
    /// Width over height (> 0).
    pub aspect: f64,
    /// Rotation mapping world directions to equatorial J2000
    /// directions (transpose of the Phase-5 frame map).
    pub world_to_equatorial: DMat3,
    /// World-space velocity in frame units/s (travel hint; zero
    /// disables the heading bias, keeping a concentric margin).
    pub velocity_world: DVec3,
}

/// One prioritized tile range from [`Scheduler::plan`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedRange {
    /// Nested ids at the plan order (caller expands within budget).
    pub range: Range<u64>,
    /// [`PRIORITY_FRUSTUM`] or [`PRIORITY_TRAVEL`].
    pub priority: u8,
}

/// A scheduling pass: generation-stamped, priority-sorted ranges.
#[derive(Clone, Debug)]
pub struct CatalogPlan {
    /// Scheduler generation (matches loader request echoes).
    pub generation: u64,
    /// Ranges, frustum priority first, nested order within a priority.
    pub ranges: Vec<PlannedRange>,
}

/// Cooker-compatible tile path: `<root>/order<NN>/<pixel>.tile`.
pub fn tile_path(root: &Path, order: u8, pixel: u64) -> PathBuf {
    root.join(format!("order{order}"))
        .join(format!("{pixel}.tile"))
}

/// View → prioritized tile ranges at one HEALPix order.
pub struct Scheduler {
    order: u8,
    tile_root: PathBuf,
    generation: u64,
}

impl Scheduler {
    /// New scheduler for `order` over the tile tree at `tile_root`.
    /// Returns `None` for `order > MAX_ORDER`.
    pub fn new(order: u8, tile_root: PathBuf) -> Option<Self> {
        if order > MAX_ORDER {
            return None;
        }
        Some(Self {
            order,
            tile_root,
            generation: 0,
        })
    }

    /// Tile order this scheduler plans for.
    pub fn order(&self) -> u8 {
        self.order
    }

    /// Cooker-layout path for `key`.
    pub fn tile_path(&self, key: &TileKey) -> PathBuf {
        tile_path(&self.tile_root, key.order, key.pixel)
    }

    /// Plan one pass over `view`. Returns `None` for invalid views
    /// (non-finite/degenerate inputs); otherwise bumps the generation
    /// and returns frustum (3) then travel-margin (2) ranges.
    pub fn plan(&mut self, view: &SkyView) -> Option<CatalogPlan> {
        if !view.cam_forward_world.is_finite()
            || !view.velocity_world.is_finite()
            || !(0.0 < view.fov_y_rad && view.fov_y_rad < std::f64::consts::PI)
            || !(view.aspect.is_finite() && view.aspect > 0.0)
        {
            return None;
        }
        let forward = view.cam_forward_world.try_normalize()?;
        let half_diag = (0.5 * view.fov_y_rad).tan() * (1.0 + view.aspect * view.aspect).sqrt();
        let half_diag = half_diag.atan();
        // One tile diagonal of slack so edge tiles are kept, not clipped.
        let margin =
            resolution_arcsec(self.order).map(|arcsec| (arcsec / 3600.0).to_radians())? + 0.002;
        let radius_tight = half_diag + margin;
        // Travel margin: widen and bias the cone toward the heading.
        let wide_dir_world = if view.velocity_world.length() < 1e-12 {
            forward
        } else {
            (forward + TRAVEL_BLEND * view.velocity_world.normalize_or_zero()).try_normalize()?
        };
        let radius_wide = (half_diag * WIDE_MULTIPLIER + margin).min(std::f64::consts::PI);
        let rotation = &view.world_to_equatorial;
        let axis_tight = equatorial_of(rotation * forward)?;
        let axis_wide = equatorial_of(rotation * wide_dir_world)?;
        let frustum_eq = Frustum::from_view_proj(view.view_proj).rotated(rotation);
        // Single cone + filter pass over the wide cone (the tight query
        // would re-walk the same subtree): priority splits at emission
        // time, by ancestor-center membership in the tight cone
        // (generously bounded, so edge cells prioritize up, never down).
        // Priority is load order only — coverage is identical either way.
        let wide_ranges = tiles_in_cone(self.order, axis_wide.0, axis_wide.1, radius_wide)?;
        let tight_unit = equatorial_unit(axis_tight.0, axis_tight.1);
        let tight_center = DVec3::new(tight_unit[0], tight_unit[1], tight_unit[2]);
        let mut ranges = filter_ranges(
            self.order,
            &wide_ranges,
            &frustum_eq,
            tight_center,
            radius_tight,
        );
        // Frustum priority first, nested order within a priority (the
        // filter emits in subdivision order; this partition is stable).
        ranges.sort_by_key(|planned| std::cmp::Reverse(planned.priority));
        self.generation = self.generation.wrapping_add(1);
        Some(CatalogPlan {
            generation: self.generation,
            ranges,
        })
    }
}

/// World→equatorial unit direction to (`ra`, `dec`).
fn equatorial_of(dir: DVec3) -> Option<(f64, f64)> {
    let unit = dir.try_normalize()?;
    let dec = unit.z.clamp(-1.0, 1.0).asin();
    Some((
        unit.y.atan2(unit.x).rem_euclid(2.0 * std::f64::consts::PI),
        dec,
    ))
}

/// Nine quad sample points (corners, edge midpoints, center) as
/// equatorial unit vectors. The midpoint samples keep huge coarse cells
/// honest against great-circle edge crossings; a residual pathological
/// miss degrades to the deterministic fallback, never to blank sky.
fn quad_samples(depth: u8, ancestor: u64) -> Option<[DVec3; 9]> {
    let (cx, cy) = projected_center(depth, ancestor)?;
    let [s, e, nn, w] = projected_corners(depth, ancestor)?;
    let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    let pts = [
        s,
        e,
        nn,
        w,
        mid(s, e),
        mid(e, nn),
        mid(nn, w),
        mid(w, s),
        (cx, cy),
    ];
    let mut out = [DVec3::ZERO; 9];
    for (index, (x, y)) in pts.iter().enumerate() {
        let (ra, dec) = unproj(*x, *y)?;
        let u = equatorial_unit(ra, dec);
        out[index] = DVec3::new(u[0], u[1], u[2]);
    }
    Some(out)
}

/// Hierarchical frustum filter over cone ranges: descend aligned
/// subtrees, emit fully-inside ranges with priority, prune
/// fully-outside ones. Priority splits at emission: the ancestor
/// center inside the tight cone (plus its own quad bound, so edge
/// cells prioritize up, never down) ⇒ frustum rank, else travel rank.
fn filter_ranges(
    order: u8,
    ranges: &[Range<u64>],
    frustum: &Frustum,
    tight_center: DVec3,
    tight_radius: f64,
) -> Vec<PlannedRange> {
    let mut out = Vec::new();
    for range in ranges {
        let mut cursor = range.start;
        while cursor < range.end {
            // Largest aligned subtree block at the cursor.
            let remaining = range.end - cursor;
            let mut size = 1u64;
            let mut levels = 0u8;
            while levels < order && size.saturating_mul(4) <= remaining && cursor % (size * 4) == 0
            {
                size *= 4;
                levels += 1;
            }
            let depth = order - levels;
            filter_block(
                order,
                cursor >> (2 * levels),
                depth,
                frustum,
                tight_center,
                tight_radius,
                &mut out,
            );
            cursor += size;
        }
    }
    merge_planned(&mut out);
    out
}

fn filter_block(
    order: u8,
    ancestor: u64,
    depth: u8,
    frustum: &Frustum,
    tight_center: DVec3,
    tight_radius: f64,
    out: &mut Vec<PlannedRange>,
) {
    let samples = match quad_samples(depth, ancestor) {
        Some(samples) => samples,
        None => return,
    };
    if frustum.contains_all(&samples) {
        // Ancestor quad bound: farthest sample from the center.
        let center = samples[8];
        let mut bound = 0.0f64;
        for sample in &samples[..8] {
            bound = bound.max(center.angle_between(*sample));
        }
        let priority = if center.angle_between(tight_center) <= tight_radius + bound {
            PRIORITY_FRUSTUM
        } else {
            PRIORITY_TRAVEL
        };
        let shift = 2 * (order - depth);
        out.push(PlannedRange {
            range: (ancestor << shift)..((ancestor + 1) << shift),
            priority,
        });
    } else if frustum.excludes_all(&samples) {
        // Pruned.
    } else if depth == order {
        let center = samples[8];
        let priority = if center.angle_between(tight_center) <= tight_radius {
            PRIORITY_FRUSTUM
        } else {
            PRIORITY_TRAVEL
        };
        out.push(PlannedRange {
            range: ancestor..(ancestor + 1),
            priority,
        });
    } else {
        let base = ancestor << 2;
        filter_block(
            order,
            base,
            depth + 1,
            frustum,
            tight_center,
            tight_radius,
            out,
        );
        filter_block(
            order,
            base | 1,
            depth + 1,
            frustum,
            tight_center,
            tight_radius,
            out,
        );
        filter_block(
            order,
            base | 2,
            depth + 1,
            frustum,
            tight_center,
            tight_radius,
            out,
        );
        filter_block(
            order,
            base | 3,
            depth + 1,
            frustum,
            tight_center,
            tight_radius,
            out,
        );
    }
}

/// Merge contiguous same-priority ranges (nested order makes siblings
/// adjacent; the sort keeps priorities grouped for the merge).
fn merge_planned(planned: &mut Vec<PlannedRange>) {
    planned.sort_by_key(|p| (p.priority, p.range.start));
    let mut merged: Vec<PlannedRange> = Vec::with_capacity(planned.len());
    for item in planned.drain(..) {
        if let Some(last) = merged.last_mut()
            && last.priority == item.priority
            && last.range.end >= item.range.start
        {
            last.range.end = last.range.end.max(item.range.end);
            continue;
        }
        merged.push(item);
    }
    *planned = merged;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::healpix::{ang2pix, npix, pix2ang};
    use glam::dcamera::rh::proj::directx::perspective;
    use glam::dcamera::rh::view::look_at_mat4;
    use glam::{DMat3, DVec3};
    use std::f64::consts::PI;

    fn sky_view(forward: DVec3) -> SkyView {
        let eye = -forward * 5.0;
        let view = look_at_mat4(eye, DVec3::ZERO, DVec3::Y);
        let proj = perspective(60.0_f64.to_radians(), 16.0 / 9.0, 0.05, 1000.0);
        SkyView {
            view_proj: proj * view,
            cam_forward_world: forward,
            fov_y_rad: 60.0_f64.to_radians(),
            aspect: 16.0 / 9.0,
            world_to_equatorial: DMat3::IDENTITY,
            velocity_world: DVec3::ZERO,
        }
    }

    #[test]
    fn plan_hits_view_center_at_frustum_priority() {
        // Looking along +X with identity frame map: the ra ≈ 0 center
        // tile is frustum priority, and every planned id is in-bounds.
        let mut scheduler = Scheduler::new(4, "tiles".into()).expect("valid order");
        let view = sky_view(DVec3::X);
        let plan = scheduler.plan(&view).expect("valid view");
        assert_eq!(plan.generation, 1);
        assert!(!plan.ranges.is_empty());
        let count = npix(4).expect("valid order");
        for planned in &plan.ranges {
            assert!(planned.range.end <= count);
            assert!(planned.priority == PRIORITY_FRUSTUM || planned.priority == PRIORITY_TRAVEL);
        }
        assert!(plan.ranges.first().expect("ranges").priority == PRIORITY_FRUSTUM);
        let center = ang2pix(4, 0.0, 0.0).expect("coords");
        assert!(
            plan.ranges
                .iter()
                .any(|r| r.priority == PRIORITY_FRUSTUM && r.range.contains(&center)),
            "view-center tile planned"
        );
        // A second plan bumps the generation.
        let plan2 = scheduler.plan(&view).expect("valid view");
        assert_eq!(plan2.generation, 2);
    }

    #[test]
    fn probe_order12_plan_size() {
        // 60°-fov order-12 plan: tens of millions of pixels in
        // ~10k ranges; the tight cone takes frustum priority with the
        // bulk of the pixels (whole base-cell subtrees where covered),
        // the wide ring takes travel priority.
        let mut scheduler = Scheduler::new(12, "tiles".into()).expect("valid order");
        let view = sky_view(DVec3::X);
        let plan = scheduler.plan(&view).expect("valid view");
        let pixels = |priority: u8| {
            plan.ranges
                .iter()
                .filter(|r| r.priority == priority)
                .map(|r| r.range.end - r.range.start)
                .sum::<u64>()
        };
        let total = pixels(PRIORITY_FRUSTUM) + pixels(PRIORITY_TRAVEL);
        assert!(total > 1_000_000, "frustum plan covers millions of pixels");
        let p3 = pixels(PRIORITY_FRUSTUM);
        assert!(
            p3 * 2 > total,
            "tight cone holds the bulk: p3={p3} total={total}"
        );
        let center = ang2pix(12, 0.0, 0.0).expect("coords");
        assert!(
            plan.ranges
                .iter()
                .any(|r| r.priority == PRIORITY_FRUSTUM && r.range.contains(&center)),
            "view-center tile loads first"
        );
    }

    #[test]
    fn plan_rejects_invalid_views() {
        let mut scheduler = Scheduler::new(4, "tiles".into()).expect("valid order");
        let bad = SkyView {
            cam_forward_world: DVec3::ZERO,
            ..sky_view(DVec3::X)
        };
        assert!(scheduler.plan(&bad).is_none());
        let bad = SkyView {
            fov_y_rad: -0.5,
            ..sky_view(DVec3::X)
        };
        assert!(scheduler.plan(&bad).is_none());
        assert!(Scheduler::new(99, "tiles".into()).is_none());
    }

    #[test]
    fn tile_path_matches_cooker_layout() {
        let scheduler = Scheduler::new(12, PathBuf::from("/data/sky")).expect("valid order");
        let key = TileKey {
            order: 12,
            pixel: 42,
        };
        assert_eq!(
            scheduler.tile_path(&key),
            PathBuf::from("/data/sky/order12/42.tile")
        );
    }

    #[test]
    fn filter_keeps_visible_drops_hidden() {
        // Order-2 sky, camera along +X: kept ranges cover ra ≈ 0 and
        // never the antipode (ra ≈ π).
        let mut scheduler = Scheduler::new(2, "tiles".into()).expect("valid order");
        let plan = scheduler.plan(&sky_view(DVec3::X)).expect("valid view");
        let flat: Vec<u64> = plan.ranges.iter().flat_map(|r| r.range.clone()).collect();
        assert!(!flat.is_empty());
        let hidden = ang2pix(2, PI, 0.0).expect("coords");
        assert!(!flat.contains(&hidden), "antipode filtered out");
        let shown = ang2pix(2, 0.0, 0.0).expect("coords");
        assert!(flat.contains(&shown), "view center kept");
        // Every kept pixel center is inside the wide cone plus one
        // quad half-diagonal (1.56 rad wide cone at this fov/order +
        // ~0.19 rad quad extent): the filter never invents tiles, it
        // only trims the cone pre-pass.
        for pixel in flat {
            let (ra, dec) = pix2ang(2, pixel).expect("valid pixel");
            let u = equatorial_unit(ra, dec);
            let dir = DVec3::new(u[0], u[1], u[2]);
            assert!(dir.angle_between(DVec3::X) < 1.75, "pixel {pixel} in cone");
        }
    }
}
