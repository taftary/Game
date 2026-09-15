//! Flat hemisphere chunk map: tangent-plane projection (`plans/chunk-flat-view`).
//!
//! A whole sphere cannot be flattened without tearing it apart, so this
//! module does not try: it shows **one hemisphere at a time**. Given a
//! viewpoint on the sphere, every chunk lying fully on the viewpoint's
//! half is projected onto the tangent plane there with the Lambert
//! azimuthal equal-area map (equal spherical areas stay equal planar
//! areas, so all chunks keep the same size); partial rim chunks and the
//! far half are dropped (unloaded). Orbiting the viewpoint loads a new
//! half — the debug viewer rebuilds its flat GPU buffers from these
//! functions on every arrow-key step.
//!
//! All functions are pure + deterministic: same `(mesh, viewpoint)`
//! gives bit-identical output on every platform (fixed-order f32 math,
//! ordered iteration only).

use crate::hexsphere::HexSphere;

/// Cells lying fully on the viewpoint's half: every corner satisfies
/// `dot(corner, viewpoint) > 0` with both normalized. Rim cells whose
/// center is visible but some corner pokes over the horizon are
/// excluded, so the flat map never shows partial polygons.
/// Deterministic cell-id order.
///
/// ```
/// use game_engine::hexsphere::HexSphere;
/// use game_engine::render::visible_hemisphere;
///
/// let mesh = HexSphere::generate(1, 1.0);
/// let visible = visible_hemisphere(&mesh, [0.0, 1.0, 0.0]);
/// assert!(!visible.is_empty() && visible.len() < mesh.cell_count());
/// ```
pub fn visible_hemisphere(mesh: &HexSphere, viewpoint: [f32; 3]) -> Vec<u32> {
    let view = normalize(viewpoint);
    let mut cells = Vec::new();
    'cells: for cell in 0..mesh.cell_count() as u32 {
        for corner in mesh.cell_corner_ids(cell) {
            let p = mesh.corner_position(corner);
            if p[0] * view[0] + p[1] * view[1] + p[2] * view[2] <= 0.0 {
                continue 'cells;
            }
        }
        cells.push(cell);
    }
    cells
}

/// Orthonormal `(right, up)` basis of the tangent plane at `viewpoint`
/// (both unit length, both perpendicular to the viewpoint direction).
/// `right` is the world-up-free direction `normalize(cross(up0, view))`
/// with `up0 = [0, 1, 0]` (falling back to `[1, 0, 0]` at the poles);
/// `up = cross(view, right)`.
pub fn tangent_basis(viewpoint: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let view = normalize(viewpoint);
    let up0 = if view[1].abs() > 0.99 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let right = normalize(cross(up0, view));
    let up = cross(view, right);
    (right, up)
}

/// Lambert azimuthal equal-area projection of a surface `point` onto
/// the tangent plane at `viewpoint`, in the [`tangent_basis`]
/// coordinates scaled by the viewpoint's radius (world units). Equal
/// spherical areas map to equal planar areas, so every chunk keeps the
/// same size on the flat map. The viewpoint itself maps to `(0, 0)`;
/// the rim (`dot == 0`) maps to radius `√2·R` — finite, so no clamping
/// is needed. Callers only project strictly visible points.
pub fn project_to_tangent(point: [f32; 3], viewpoint: [f32; 3]) -> [f32; 2] {
    let view = normalize(viewpoint);
    let (right, up) = tangent_basis(view);
    let radius = length(viewpoint).max(1e-6);
    let unit = normalize(point);
    let cos_c = (unit[0] * view[0] + unit[1] * view[1] + unit[2] * view[2]).clamp(-1.0, 1.0);
    let k = (2.0 / (1.0 + cos_c).max(1e-6)).sqrt();
    let s = radius * k;
    let off = [
        s * (unit[0] - view[0] * cos_c),
        s * (unit[1] - view[1] * cos_c),
        s * (unit[2] - view[2] * cos_c),
    ];
    [
        off[0] * right[0] + off[1] * right[1] + off[2] * right[2],
        off[0] * up[0] + off[1] * up[1] + off[2] * up[2],
    ]
}

/// Orbits `viewpoint` by `yaw` radians around world Y, then by `pitch`
/// radians around the local tangent-right axis. Returns the moved point
/// renormalized to the original radius (stays on the sphere).
pub fn orbit_viewpoint(viewpoint: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let radius = length(viewpoint).max(1e-6);
    let mut view = normalize(viewpoint);
    view = rotate_around(view, [0.0, 1.0, 0.0], yaw);
    let (right, _) = tangent_basis(view);
    view = rotate_around(view, right, pitch);
    [view[0] * radius, view[1] * radius, view[2] * radius]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = length(v).max(1e-12);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn rotate_around(v: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    // Rodrigues' rotation formula (axis is unit length by construction).
    let (sin, cos) = angle.sin_cos();
    let dot = v[0] * axis[0] + v[1] * axis[1] + v[2] * axis[2];
    let cross = cross(axis, v);
    [
        v[0] * cos + cross[0] * sin + axis[0] * dot * (1.0 - cos),
        v[1] * cos + cross[1] * sin + axis[1] * dot * (1.0 - cos),
        v[2] * cos + cross[2] * sin + axis[2] * dot * (1.0 - cos),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hemisphere_holds_about_half_the_cells() {
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            let visible = visible_hemisphere(&mesh, [0.0, 1.0, 0.0]);
            assert!(!visible.is_empty(), "N={n}");
            assert!(visible.len() < mesh.cell_count(), "N={n}");
            // Roughly half once cells are small (N=0 cells span the
            // globe); the all-corners filter drops the partial rim, so
            // the set runs a little under half.
            if n >= 2 {
                let half = mesh.cell_count() as f32 / 2.0;
                assert!(
                    visible.len() as f32 > half * 0.75 && visible.len() as f32 <= half,
                    "N={n} visible {} of {}",
                    visible.len(),
                    mesh.cell_count()
                );
            }
            // Cell-id order, no duplicates.
            let mut sorted = visible.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted, visible, "N={n}");
        }
    }

    #[test]
    fn visible_cells_are_fully_inside() {
        // The all-corners filter: no visible cell may straddle the rim.
        for n in 0..=3 {
            let mesh = HexSphere::generate(n, 1.0);
            for view in [[0.0, 1.0, 0.0], normalize([0.3, 0.8, 0.5])] {
                let view = normalize(view);
                for cell in visible_hemisphere(&mesh, view) {
                    for corner in mesh.cell_corner_ids(cell) {
                        let p = mesh.corner_position(corner);
                        assert!(
                            p[0] * view[0] + p[1] * view[1] + p[2] * view[2] > 0.0,
                            "N={n} cell {cell} straddles the rim"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn projection_stays_inside_bounded_disk() {
        // Lambert maps the open hemisphere into the disk of radius √2·R
        // — the rim-clamp era is over.
        let mesh = HexSphere::generate(2, 1.0);
        let view = [0.0, 1.0, 0.0];
        let bound = std::f32::consts::SQRT_2 + 1e-4;
        for cell in visible_hemisphere(&mesh, view) {
            for corner in mesh.cell_corner_ids(cell) {
                let p = project_to_tangent(mesh.corner_position(corner), view);
                assert!(p[0].hypot(p[1]) <= bound, "cell {cell} {p:?}");
            }
        }
    }

    #[test]
    fn projected_chunks_keep_equal_size() {
        // Equal-area map: chunks of the same class project to roughly
        // the same planar area wherever they sit (shoelace over the
        // projected ring). Pentagons are intrinsically smaller than
        // hexagons on the sphere, so classes are compared separately —
        // the map preserves each chunk's true relative area instead of
        // stretching the rim.
        let mesh = HexSphere::generate(2, 1.0);
        let view = [0.0, 1.0, 0.0];
        let mut hex = (f32::INFINITY, f32::NEG_INFINITY);
        let mut pent = (f32::INFINITY, f32::NEG_INFINITY);
        for cell in visible_hemisphere(&mesh, view) {
            let ring: Vec<[f32; 2]> = mesh
                .cell_corner_ids(cell)
                .map(|corner| project_to_tangent(mesh.corner_position(corner), view))
                .collect();
            let mut area = 0.0;
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                area += a[0] * b[1] - b[0] * a[1];
            }
            let area = area.abs() / 2.0;
            let slot = if mesh.is_pentagon(cell) {
                &mut pent
            } else {
                &mut hex
            };
            slot.0 = slot.0.min(area);
            slot.1 = slot.1.max(area);
        }
        assert!(hex.0 > 0.0 && pent.0 > 0.0, "degenerate chunk");
        assert!(hex.1 / hex.0 < 1.2, "hex spread {hex:?}");
        assert!(pent.1 / pent.0 < 1.2, "pent spread {pent:?}");
    }

    #[test]
    fn viewpoint_projects_to_origin_with_orthonormal_basis() {
        let view = [0.3, 0.8, 0.5];
        let (right, up) = tangent_basis(view);
        let n = normalize(view);
        for v in [right, up, n] {
            assert!((length(v) - 1.0).abs() < 1e-6, "{v:?}");
        }
        assert!(right[0] * n[0] + right[1] * n[1] + right[2] * n[2] < 1e-6);
        assert!(up[0] * n[0] + up[1] * n[1] + up[2] * n[2] < 1e-6);
        assert!(right[0] * up[0] + right[1] * up[1] + right[2] * up[2] < 1e-6);
        let origin = project_to_tangent(n, view);
        assert!(origin[0].abs() < 1e-5 && origin[1].abs() < 1e-5);
    }

    #[test]
    fn orbit_keeps_radius_and_moves() {
        // Off-pole start: yawing the pole itself around Y is a no-op.
        let view = normalize([1.0, 1.0, 0.0]);
        let moved = orbit_viewpoint(view, 0.1, 0.0);
        assert!((length(moved) - 1.0).abs() < 1e-6);
        assert!(
            (moved[0] - view[0]).abs() > 1e-4 || (moved[2] - view[2]).abs() > 1e-4,
            "{moved:?}"
        );
        // Full turn returns home.
        let back = orbit_viewpoint(view, std::f32::consts::TAU, 0.0);
        assert!(
            (back[0] - view[0]).abs() < 1e-4
                && (back[1] - view[1]).abs() < 1e-4
                && (back[2] - view[2]).abs() < 1e-4,
            "{back:?}"
        );
    }

    #[test]
    fn projection_is_deterministic() {
        let mesh = HexSphere::generate(2, 1.0);
        let view = [0.0, 1.0, 0.0];
        assert_eq!(
            visible_hemisphere(&mesh, view),
            visible_hemisphere(&mesh, view)
        );
        let p = mesh.cell_center(7);
        assert_eq!(project_to_tangent(p, view), project_to_tangent(p, view));
    }
}
