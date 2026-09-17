//! View-frustum extraction for the tile scheduler (spec §4 priority:
//! view-frustum tiles first).
//!
//! Pure `f64` world-space math on a caller-supplied view-projection
//! matrix: Gribb/Hartmann plane extraction adapted to the engine's
//! Vulkan depth range (NDC z ∈ [0, 1] — the un-flipped
//! `directx::perspective` from `render::camera`, never
//! `vulkan::perspective`). Planes point inward and are normalized, so
//! tile-quad tests are sign comparisons. Never touches camera code —
//! the scheduler passes matrices in, planes come out (invariant-safe).
//!
//! ```
//! use game_engine::catalog::frustum::Frustum;
//! use game_engine::render::OrbitCamera;
//! use glam::{DMat4, DVec3};
//!
//! // Real pipeline matrices (f32) widened to the f64 frustum.
//! let camera = OrbitCamera::framing_planet(1.0);
//! let wide = camera.projection_matrix(16.0 / 9.0) * camera.view_matrix();
//! let vp = DMat4::from_cols_array(&wide.to_cols_array().map(|x| x as f64));
//! let frustum = Frustum::from_view_proj(vp);
//! // The view target sits inside its own frustum.
//! assert!(frustum.contains_point(DVec3::new(0.0, 0.0, 0.0)));
//! ```

use glam::{DMat3, DMat4, DVec3, DVec4};

/// Boundary tolerance for plane tests (absolute, on unit directions).
/// Biases toward keeping edge tiles: a boundary point counts as inside.
const PLANE_EPS: f64 = 1e-7;

/// One inward-facing, normalized frustum plane: `normal·p + dist ≥ 0`
/// inside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Unit inward normal (world space, or the rotated space).
    pub normal: DVec3,
    /// Plane offset.
    pub dist: f64,
}

impl Plane {
    fn test(&self, point: DVec3) -> f64 {
        self.normal.dot(point) + self.dist
    }
}

/// Six inward planes (left, right, bottom, top, near, far) extracted
/// from a world-space view-projection matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    /// Extract planes from `view_proj` (world → clip). Assumes the
    /// engine's NDC cube (`|x|,|y| ≤ w`, `0 ≤ z ≤ w`); a matrix from any
    /// other convention yields a valid extraction of the wrong volume —
    /// callers must pass engine pipeline matrices (see doctest).
    pub fn from_view_proj(view_proj: DMat4) -> Self {
        let row0 = view_proj.row(0);
        let row1 = view_proj.row(1);
        let row2 = view_proj.row(2);
        let row3 = view_proj.row(3);
        Self {
            planes: [
                plane(row3 + row0), // left: x + w ≥ 0
                plane(row3 - row0), // right: w − x ≥ 0
                plane(row3 + row1), // bottom: y + w ≥ 0
                plane(row3 - row1), // top: w − y ≥ 0
                plane(row2),        // near: z ≥ 0 (Vulkan [0, 1])
                plane(row3 - row2), // far: w − z ≥ 0
            ],
        }
    }

    /// Rotate the frustum by `rotation` (orthonormal): plane normals
    /// rotate by the transpose, offsets are unchanged. The scheduler
    /// uses this with the world→equatorial rotation so tile corners
    /// (equatorial unit vectors) test directly.
    pub fn rotated(&self, rotation: &DMat3) -> Self {
        let transpose = rotation.transpose();
        let mut planes = self.planes;
        for plane in &mut planes {
            plane.normal = transpose * plane.normal;
        }
        Self { planes }
    }

    /// True when `point` is inside every plane (boundary counts in).
    pub fn contains_point(&self, point: DVec3) -> bool {
        self.planes.iter().all(|p| p.test(point) >= -PLANE_EPS)
    }

    /// True when every point is inside every plane.
    pub fn contains_all(&self, points: &[DVec3]) -> bool {
        points.iter().all(|&p| self.contains_point(p))
    }

    /// True when every point lies outside at least one common plane
    /// (the quad is provably disjoint from the frustum).
    pub fn excludes_all(&self, points: &[DVec3]) -> bool {
        self.planes
            .iter()
            .any(|plane| points.iter().all(|&p| plane.test(p) < -PLANE_EPS))
    }

    /// Plane iterator (debug overlay + tests).
    pub fn planes(&self) -> impl Iterator<Item = &Plane> {
        self.planes.iter()
    }
}

fn plane(row: DVec4) -> Plane {
    let normal = DVec3::new(row.x, row.y, row.z);
    let length = normal.length();
    debug_assert!(length > 0.0, "degenerate frustum plane");
    let length = if length > 0.0 { length } else { 1.0 };
    Plane {
        normal: normal / length,
        dist: row.w / length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::OrbitCamera;

    fn pipeline_frustum() -> Frustum {
        let camera = OrbitCamera::framing_planet(1.0);
        let wide = camera.projection_matrix(16.0 / 9.0) * camera.view_matrix();
        let vp = DMat4::from_cols_array(&wide.to_cols_array().map(|x| x as f64));
        Frustum::from_view_proj(vp)
    }

    #[test]
    fn real_pipeline_frustum_contains_its_target() {
        let camera = OrbitCamera::framing_planet(1.0);
        let frustum = pipeline_frustum();
        // Target at the origin, eye out on the view axis: origin inside,
        // a point beyond the eye (behind the camera) outside.
        assert!(frustum.contains_point(DVec3::ZERO));
        assert!(!frustum.contains_point(camera.eye().as_dvec3() * 2.0));
        // All six planes normalized.
        for plane in frustum.planes() {
            assert!((plane.normal.length() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn rotation_moves_planes_with_points() {
        let frustum = pipeline_frustum();
        // 90° about Z: +X maps to +Y. A point inside stays inside iff
        // the frustum rotates with it.
        let rotation = DMat3::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let rotated = frustum.rotated(&rotation);
        let inside = DVec3::new(0.0, 0.0, 0.0);
        assert!(frustum.contains_point(inside));
        assert!(rotated.contains_point(rotation.transpose() * inside));
        // Normals stay unit under rotation.
        for plane in rotated.planes() {
            assert!((plane.normal.length() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn excludes_all_detects_disjoint_quads() {
        let frustum = pipeline_frustum();
        let behind = [
            DVec3::new(0.0, 0.0, 1e6),
            DVec3::new(1.0, 0.0, 1e6),
            DVec3::new(0.0, 1.0, 1e6),
        ];
        assert!(frustum.excludes_all(&behind));
        assert!(!frustum.contains_all(&behind));
        let straddling = [DVec3::ZERO, DVec3::new(0.0, 0.0, 1e6)];
        assert!(!frustum.excludes_all(&straddling));
        assert!(!frustum.contains_all(&straddling));
    }
}
