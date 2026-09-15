//! Cell-chunk picking: cursor → world ray → sphere hit → nearest chunk.
//!
//! Pure math over `glam` — no window, no GPU handle — so the windowed
//! binary turns cursor events into hover/pin state through these
//! functions and every behavior stays unit-testable (see
//! `plans/cell-chunks`). Determinism: indexed loops over the stable
//! mesh buffers only, exact ties keep the lowest chunk id.

use game_engine::hexsphere::{ChunkId, HexSphere};
use glam::{Mat4, Vec3, Vec4};

use crate::ui::Rect;

/// World-space ray: origin plus normalized direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

/// Unproject a cursor position (y-down pixels, same space as [`Rect`])
/// into a world-space ray.
///
/// `viewport` is the rect the sphere fills (main viewport or thumb —
/// the caller hit-tests the cursor first); `view_proj` is
/// `projection * view` built with the exact matrices the renderer draws
/// with (Vulkan NDC: z ∈ [0, 1]). Degenerate inputs (zero-size rect,
/// singular matrix) yield a zero-direction ray that
/// [`intersect_sphere`] rejects.
///
/// NDC-y convention (the one this app renders with — see `ortho_matrix`
/// in the binary, whose pixel-row-0 ↔ NDC-y-+1 mapping is pinned by its
/// test and proven daily by the working UI): NDC y = +1 is the TOP row,
/// so the cursor's y-down `v` maps as `1 − 2·v`. Getting this sign wrong
/// mirrors hover top↔bottom (issue-2026-09-15-1144-hover-y-inverted).
pub fn ray_from_cursor(cursor: (f32, f32), viewport: Rect, view_proj: Mat4) -> Ray {
    let u = (cursor.0 - viewport.x) / viewport.w.max(f32::EPSILON);
    let v = (cursor.1 - viewport.y) / viewport.h.max(f32::EPSILON);
    let inverse = view_proj.inverse();
    let unproject = |z: f32| {
        let clip = Vec4::new(2.0 * u - 1.0, 1.0 - 2.0 * v, z, 1.0);
        let world = inverse * clip;
        Vec3::new(world.x, world.y, world.z) / world.w
    };
    let near = unproject(0.0);
    let far = unproject(1.0);
    let dir = (far - near).try_normalize().unwrap_or(Vec3::ZERO);
    Ray {
        origin: if near.is_finite() { near } else { Vec3::ZERO },
        dir,
    }
}

/// Analytic ray hit on the origin-centered sphere of `radius`: the
/// nearest positive-`t` point, or `None` on a miss (cursor over empty
/// space), a degenerate ray, or an invalid radius.
pub fn intersect_sphere(ray: Ray, radius: f32) -> Option<Vec3> {
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    if !ray.origin.is_finite() || ray.dir.length_squared() <= 0.0 {
        return None;
    }
    // `dir` is unit length by construction, so |o + t·d|² = R² reduces
    // to t² + 2(o·d)t + (|o|² − R²) = 0.
    let along = ray.origin.dot(ray.dir);
    let surface = ray.origin.length_squared() - radius * radius;
    let discriminant = along * along - surface;
    if !discriminant.is_finite() || discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = -along - root;
    let hit = if near > 0.0 { near } else { -along + root };
    if !hit.is_finite() || hit <= 0.0 {
        return None;
    }
    Some(ray.origin + ray.dir * hit)
}

/// Nearest chunk to `point` (a sphere-surface hit from
/// [`intersect_sphere`]): maximizes `dot(center, point)`, i.e. minimizes
/// angular distance — no square roots anywhere.
///
/// From `hint` (usually the last hovered chunk) the neighbor graph is
/// hill-climbed to the local maximum — O(1) amortized per cursor move;
/// `None` (cold start) scans every cell once. Only strict improvements
/// move, so exact ties keep the lowest chunk id and repeated calls with
/// the same inputs agree bit-for-bit.
///
/// Greedy ascent with strict improvement always terminates (finite
/// cells, scores bounded). It stops at a cell no neighbor strictly
/// beats in f32 — a global f32 maximizer on well-formed meshes (the
/// neighbor ring surrounds each cell; the convergence test below pins
/// maximizer-equality against the full scan on dense samples, from
/// adversarial hints).
///
/// Exact ties live at symmetry points (dual corners, antipodes) where
/// adjacent cells are equidistant below f32 resolution: the cold scan
/// then keeps the lowest id while a climb keeps the first-reached tied
/// cell. Both maximize the f32 score, so either answer borders the
/// query point — real cursors never rest exactly on these measure-zero
/// points, and adjacent pixels have strict winners both paths agree on.
pub fn pick_cell(mesh: &HexSphere, point: Vec3, hint: Option<ChunkId>) -> ChunkId {
    debug_assert!(point.is_finite(), "pick_cell needs a finite sphere hit");
    let score = |cell: u32| {
        let center = mesh.cell_center(cell);
        center[0] * point.x + center[1] * point.y + center[2] * point.z
    };
    // Strict `>`: the incumbent (lowest id on ties) survives.
    let mut best = match hint {
        Some(chunk) if (chunk.index() as usize) < mesh.chunk_count() => chunk.index(),
        _ => 0,
    };
    if hint.is_some_and(|chunk| (chunk.index() as usize) < mesh.chunk_count()) {
        loop {
            let mut next = best;
            for neighbor in mesh.cell_neighbors(best) {
                if score(neighbor) > score(next) {
                    next = neighbor;
                }
            }
            if next == best {
                break;
            }
            best = next;
        }
    } else {
        for cell in 1..mesh.cell_count() as u32 {
            if score(cell) > score(best) {
                best = cell;
            }
        }
    }
    mesh.chunk_id(best)
}

/// Nearest *visible* chunk to a flat-map `point` (in `[0, 1]²`
/// hemisphere space): minimizes squared 2D distance to the visible
/// centers. `cells`/`centers` are the aligned visible set from the mesh
/// builder. Full scan over the half (~20k cheap 2D ops at N=6 — well
/// within the one-frame hover budget); strict `>` keeps the lowest cell
/// id on exact ties, bit-for-bit repeatable.
///
/// # Panics
///
/// Panics if the visible set is empty or misaligned (caller invariant).
pub fn pick_flat_visible(
    mesh: &HexSphere,
    cells: &[u32],
    centers: &[[f32; 2]],
    point: [f32; 2],
) -> ChunkId {
    assert!(!cells.is_empty(), "flat pick needs a non-empty visible set");
    assert_eq!(cells.len(), centers.len(), "cells/centers must align");
    let score = |slot: usize| {
        let p = centers[slot];
        let (dx, dy) = (p[0] - point[0], p[1] - point[1]);
        -(dx * dx + dy * dy)
    };
    let mut best = 0;
    for slot in 1..cells.len() {
        if score(slot) > score(best) {
            best = slot;
        }
    }
    mesh.chunk_id(cells[best])
}

/// Inverse of the aspect-fit `flat_mvp` used by the viewer binary: maps
/// a y-down cursor pixel back to `[0, 1]²` layout space. Returns `None`
/// when the cursor is outside the rect, the rect is degenerate, or the
/// point falls outside the letterboxed map square.
pub fn flat_point_from_cursor(cursor: (f32, f32), viewport: Rect) -> Option<[f32; 2]> {
    if viewport.w < 1.0 || viewport.h < 1.0 {
        return None;
    }
    if !viewport.contains(cursor.0, cursor.1) {
        return None;
    }
    // Cursor → Vulkan NDC (y-down pixels: top row = +1, as rendered).
    let ndc_x = 2.0 * (cursor.0 - viewport.x) / viewport.w - 1.0;
    let ndc_y = 1.0 - 2.0 * (cursor.1 - viewport.y) / viewport.h;
    let aspect = viewport.w / viewport.h;
    // Inverse of `flat_mvp`: NDC = (sx·u + tx, sy·v + ty).
    let (sx, tx, sy, ty) = if aspect >= 1.0 {
        (2.0 / aspect, -1.0 / aspect, -2.0, 1.0)
    } else {
        (2.0, -1.0, -2.0 * aspect, aspect)
    };
    let (u, v) = ((ndc_x - tx) / sx, (ndc_y - ty) / sy);
    if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
        Some([u, v])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui;

    fn viewport() -> Rect {
        Rect {
            x: 0.0,
            y: 28.0,
            w: 1020.0,
            h: 692.0,
        }
    }

    #[test]
    fn center_ray_hits_origin_sphere() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, 5.0),
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(intersect_sphere(ray, 1.0), Some(Vec3::new(0.0, 0.0, 1.0)));
    }

    #[test]
    fn grazing_ray_misses() {
        let ray = Ray {
            origin: Vec3::new(0.0, 2.0, 5.0),
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(intersect_sphere(ray, 1.0), None);
    }

    #[test]
    fn degenerate_rays_hit_nothing() {
        let zero_dir = Ray {
            origin: Vec3::new(0.0, 0.0, 5.0),
            dir: Vec3::ZERO,
        };
        assert_eq!(intersect_sphere(zero_dir, 1.0), None);
        let nan_origin = Ray {
            origin: Vec3::NAN,
            dir: Vec3::NEG_Z,
        };
        assert_eq!(intersect_sphere(nan_origin, 1.0), None);
        let fine = Ray {
            origin: Vec3::new(0.0, 0.0, 5.0),
            dir: Vec3::NEG_Z,
        };
        assert_eq!(intersect_sphere(fine, 0.0), None);
        assert_eq!(intersect_sphere(fine, f32::NAN), None);
    }

    #[test]
    fn unproject_roundtrips_through_view_proj() {
        use game_engine::render::OrbitCamera;
        let camera = OrbitCamera::framing_planet(1.0);
        let vp = viewport();
        let view_proj = camera.projection_matrix(vp.w / vp.h) * camera.view_matrix();
        // Sphere center projects to the viewport middle: the middle-pixel
        // ray must strike the sphere at the near-side point facing the
        // camera (origin + eye-direction * (distance − R)).
        let middle = (vp.x + vp.w / 2.0, vp.y + vp.h / 2.0);
        let ray = ray_from_cursor(middle, vp, view_proj);
        let hit = intersect_sphere(ray, 1.0).expect("center pixel must hit");
        let eye = camera.eye();
        let expected = eye + (Vec3::ZERO - eye).normalize() * (camera.distance() - 1.0);
        assert!(
            hit.distance_squared(expected) < 1e-6,
            "hit {hit:?} vs near-side {expected:?}"
        );
        // Off-center anchor (issue-2026-09-15-1144-hover-y-inverted): the
        // old `2·v − 1` mapping mirrored Y yet stayed green here, because
        // the anchors above are symmetric (middle pixel, corner miss). A
        // surface point above the facing point must round-trip through
        // NDC→cursor→ray→hit; the NDC→cursor step uses the app convention
        // (NDC +1 = top row, as `ortho_matrix` renders it).
        let above = (expected + Vec3::Y * 0.3).normalize();
        let clip = view_proj * Vec4::new(above.x, above.y, above.z, 1.0);
        let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
        let cursor = (
            vp.x + (ndc.x + 1.0) / 2.0 * vp.w,
            vp.y + (1.0 - ndc.y) / 2.0 * vp.h,
        );
        let ray = ray_from_cursor(cursor, vp, view_proj);
        let hit = intersect_sphere(ray, 1.0).expect("off-center cursor must hit");
        assert!(
            hit.distance_squared(above) < 1e-6,
            "hit {hit:?} vs {above:?}"
        );
        // Viewport corner rays look past the limb: no hit.
        let corner = ray_from_cursor((vp.x + 2.0, vp.y + 2.0), vp, view_proj);
        assert_eq!(intersect_sphere(corner, 1.0), None);
    }

    #[test]
    fn projected_cells_cursor_roundtrip_to_self() {
        // Full cursor→ray→hit→pick loop for every camera-facing cell
        // (regression for issue-2026-09-15-1144-hover-y-inverted: the old
        // NDC-y sign mirrored every off-axis pick top↔bottom). Covered on
        // the main viewport rect and a thumb-like rect — both consumer
        // rects of `ray_from_cursor`.
        use game_engine::render::OrbitCamera;
        let mesh = HexSphere::generate(3, 1.0);
        let camera = OrbitCamera::framing_planet(1.0);
        let eye = camera.eye();
        let rects = [
            viewport(),
            Rect {
                x: 1036.0,
                y: 36.0,
                w: 244.0,
                h: 144.0,
            },
        ];
        for rect in rects {
            let view_proj = camera.projection_matrix(rect.w / rect.h) * camera.view_matrix();
            let mut visible = 0;
            for cell in 0..mesh.cell_count() as u32 {
                let center = Vec3::from_array(mesh.cell_center(cell));
                // Only truly visible centers round-trip. Facing the
                // camera's hemisphere (dot > 0) is NOT enough: centers
                // past the limb on that hemisphere still project onto the
                // disk, but their pixel ray strikes a nearer cell first
                // (diagnosed on cell 3 at N=3). Visible ⟺ eye·center > R².
                if center.dot(eye) <= mesh.radius() * mesh.radius() {
                    continue;
                }
                let clip = view_proj * Vec4::new(center.x, center.y, center.z, 1.0);
                if clip.w <= 0.0 {
                    continue;
                }
                let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
                if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 || !(0.0..=1.0).contains(&ndc.z) {
                    continue;
                }
                visible += 1;
                let cursor = (
                    rect.x + (ndc.x + 1.0) / 2.0 * rect.w,
                    rect.y + (1.0 - ndc.y) / 2.0 * rect.h,
                );
                let ray = ray_from_cursor(cursor, rect, view_proj);
                let hit = intersect_sphere(ray, mesh.radius())
                    .unwrap_or_else(|| panic!("cell {cell} cursor must hit {rect:?}"));
                assert_eq!(
                    pick_cell(&mesh, hit, None).index(),
                    cell,
                    "cell {cell} {rect:?}"
                );
            }
            assert!(visible > 100, "must cover the disk, got {visible}");
        }
    }

    #[test]
    fn pick_at_cell_centers_returns_self() {
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            for cell in 0..mesh.cell_count() as u32 {
                let center = Vec3::from_array(mesh.cell_center(cell));
                assert_eq!(pick_cell(&mesh, center, None).index(), cell, "N={n}");
                assert_eq!(
                    pick_cell(&mesh, center, Some(mesh.chunk_id(cell))).index(),
                    cell,
                    "N={n} hinted"
                );
            }
        }
    }

    /// The maximized objective, exposed for tests.
    fn score_of(mesh: &HexSphere, point: Vec3, cell: u32) -> f32 {
        let center = mesh.cell_center(cell);
        center[0] * point.x + center[1] * point.y + center[2] * point.z
    }

    /// Global f32 maximum of the objective (what the cold scan attains).
    fn max_score(mesh: &HexSphere, point: Vec3) -> f32 {
        let mut best = f32::NEG_INFINITY;
        for cell in 0..mesh.cell_count() as u32 {
            best = best.max(score_of(mesh, point, cell));
        }
        best
    }

    #[test]
    fn corner_pick_finds_global_maximizer() {
        // A dual corner is equidistant to its adjacent cells below f32
        // resolution: the cold scan keeps the lowest id while a climb
        // keeps the first-reached tied cell. Both must attain the global
        // f32 max score, and repeats must agree bit-for-bit.
        let mesh = HexSphere::generate(2, 1.0);
        for corner in 0..mesh.corner_count() as u32 {
            let point = Vec3::from_array(mesh.corner_position(corner));
            let top = max_score(&mesh, point);
            for hint in [None, Some(mesh.chunk_id(0)), Some(mesh.chunk_id(160))] {
                let picked = pick_cell(&mesh, point, hint);
                assert_eq!(
                    score_of(&mesh, point, picked.index()),
                    top,
                    "corner {corner} hint {hint:?}"
                );
                assert_eq!(pick_cell(&mesh, point, hint), picked, "corner {corner}");
            }
        }
    }

    #[test]
    fn antipodal_pick_finds_far_side() {
        let mesh = HexSphere::generate(2, 1.0);
        let near = Vec3::from_array(mesh.cell_center(0));
        let far = pick_cell(&mesh, -near, None);
        assert!(far.index() != 0);
        // The far cell's center points away from cell 0's center.
        let far_center = Vec3::from_array(mesh.cell_center(far.index()));
        assert!(far_center.dot(near) < 0.0);
    }

    #[test]
    fn hill_climb_matches_full_scan_on_dense_samples() {
        // Adversarial hunts for hill-climbing traps: a lat/lon grid at
        // N=2..3 plus every cell center and corner, each climbed from the
        // antipodal cell (longest possible walk). The invariant is
        // maximizer-equality: the climb's score must equal the cold
        // scan's max score bit-exactly. Argmax ids may differ on f32-tied
        // symmetry points — both border the query point.
        for n in 2..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let radius = mesh.radius();
            let mut samples = Vec::new();
            for cell in 0..mesh.cell_count() as u32 {
                samples.push(Vec3::from_array(mesh.cell_center(cell)));
            }
            for corner in 0..mesh.corner_count() as u32 {
                samples.push(Vec3::from_array(mesh.corner_position(corner)));
            }
            for lat in 0..24 {
                for lon in 0..48 {
                    let theta = std::f32::consts::PI * (lat as f32 + 0.5) / 24.0;
                    let phi = 2.0 * std::f32::consts::PI * lon as f32 / 48.0;
                    samples.push(Vec3::new(
                        radius * theta.sin() * phi.cos(),
                        radius * theta.cos(),
                        radius * theta.sin() * phi.sin(),
                    ));
                }
            }
            // Antipodal cell of the cold answer: the longest possible walk
            // (the antipode's nearest cell can never be `cold` itself —
            // its center is the globally farthest point from `away`).
            for point in &samples {
                let cold = pick_cell(&mesh, *point, None);
                let away = Vec3::from_array(mesh.cell_center(cold.index())) * -1.0;
                let worst_hint = pick_cell(&mesh, away, None);
                assert_ne!(worst_hint, cold, "N={n} degenerate antipode");
                let climbed = pick_cell(&mesh, *point, Some(worst_hint));
                assert_eq!(
                    score_of(&mesh, *point, climbed.index()),
                    score_of(&mesh, *point, cold.index()),
                    "N={n} trap at {point:?}"
                );
            }
        }
    }

    #[test]
    fn stale_hint_from_another_mesh_still_picks() {
        // A hint id valid for a bigger mesh but out of range here must
        // fall back to the full scan, never panic.
        let small = HexSphere::generate(1, 1.0);
        let big = HexSphere::generate(2, 1.0);
        let stale = big.chunk_id(big.cell_count() as u32 - 1);
        assert!((stale.index() as usize) >= small.chunk_count());
        let point = Vec3::from_array(small.cell_center(5));
        assert_eq!(pick_cell(&small, point, Some(stale)).index(), 5);
    }

    #[test]
    fn zero_mesh_viewport_rejects() {
        use game_engine::render::OrbitCamera;
        let camera = OrbitCamera::framing_planet(1.0);
        let view_proj = camera.projection_matrix(1.0) * camera.view_matrix();
        let flat = Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        let ray = ray_from_cursor((0.0, 0.0), flat, view_proj);
        assert_eq!(intersect_sphere(ray, 1.0), None);
    }

    #[test]
    fn flat_pick_at_centers_returns_self() {
        use crate::mesh::build_chunk_flat;
        for n in 1..=2 {
            let mesh = HexSphere::generate(n, 1.0);
            let (_, _, cells, centers) = build_chunk_flat(&mesh, [0.0, 1.0, 0.0]);
            for (slot, &cell) in cells.iter().enumerate() {
                assert_eq!(
                    pick_flat_visible(&mesh, &cells, &centers, centers[slot]).index(),
                    cell,
                    "N={n}"
                );
            }
        }
    }

    #[test]
    fn flat_cursor_inverse_roundtrips_map_points() {
        let rect = Rect {
            x: 100.0,
            y: 50.0,
            w: 800.0,
            h: 600.0,
        };
        // Map interior points survive cursor → layout → cursor
        // (exact corners sit on the exclusive rect edge, so inset them).
        for (u, v) in [(0.001, 0.001), (0.999, 0.999), (0.5, 0.5), (0.25, 0.75)] {
            // Forward: layout → cursor through `flat_mvp` conventions
            // (aspect ≥ 1 branch: NDC = (2u−1)/aspect … mirrored here).
            let aspect = rect.w / rect.h;
            let (ndc_x, ndc_y) = if aspect >= 1.0 {
                ((2.0 * u - 1.0) / aspect, 1.0 - 2.0 * v)
            } else {
                (2.0 * u - 1.0, aspect - 2.0 * aspect * v)
            };
            let cursor = (
                rect.x + (ndc_x + 1.0) / 2.0 * rect.w,
                rect.y + (1.0 - ndc_y) / 2.0 * rect.h,
            );
            let back = flat_point_from_cursor(cursor, rect).expect("must invert");
            assert!((back[0] - u).abs() < 1e-5 && (back[1] - v).abs() < 1e-5);
        }
        // Outside the rect: no pick.
        assert_eq!(flat_point_from_cursor((0.0, 0.0), rect), None);
        assert_eq!(
            flat_point_from_cursor(
                (0.0, 0.0),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0
                }
            ),
            None
        );
    }

    #[test]
    fn layout_viewport_type_flows() {
        // Compile-time proof the binary can pass `ui::layout` rects
        // straight into `ray_from_cursor`.
        let layout = ui::layout(1280.0, 720.0);
        let _: Rect = layout.viewport;
    }
}
