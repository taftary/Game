//! Cosmic Web dimension tab: read-only inspector over the shared stage-0
//! descriptor (WS5 — the fourth absorbed view).
//!
//! [`CosmicWebInspector`] owns an orbit/pan/zoom camera (Mpc units,
//! `MapOrbitCamera` policy) plus a node selection. It never mutates
//! journey, transit, or viewer state — selection is local readout state
//! (pinned by `selection_is_read_only` in `app.rs`).
//!
//! This module also owns the CPU-side vertex layout both 3D surfaces
//! share ([`node_point_cloud`], [`link_segments`], [`glow_point_cloud`]):
//! one source of truth for positions (origin-relative Mpc f32), colors,
//! and sprite sizes. The binary maps the tuples onto its GPU vertex
//! types at upload.

use super::map_camera::{DEFAULT_PITCH, DEFAULT_YAW, MapOrbitCamera};
use super::picking::project_to_screen;
use super::ui::Rect;
use game_engine::universe::WebDescriptor;
use glam::{DVec3, Mat4, Vec3};

/// Click-selection radius in px (the map-tab precedent).
pub const COSMIC_PICK_RADIUS_PX: f32 = 8.0;

/// Inspector default eye distance: the 500 Mpc sphere framed with
/// margin under the shared 60° FOV.
pub const INSPECTOR_DISTANCE_MPC: f32 = 430.0;
/// Inspector zoom band, Mpc.
pub const INSPECTOR_MIN_DISTANCE_MPC: f32 = 10.0;
/// Inspector zoom band, Mpc.
pub const INSPECTOR_MAX_DISTANCE_MPC: f32 = 800.0;

/// Read-only inspector state for the Cosmic Web dimension tab.
pub struct CosmicWebInspector {
    /// Orbit/pan/zoom camera in Mpc (inspector owns its view; the demo
    /// player camera is untouched).
    pub camera: MapOrbitCamera,
    /// Selected node index (readout only — never fed back anywhere).
    pub selected: Option<u32>,
}

impl CosmicWebInspector {
    /// Fresh inspector: whole-web framing, nothing selected.
    pub fn new() -> Self {
        Self {
            camera: MapOrbitCamera::new(
                Vec3::ZERO,
                INSPECTOR_DISTANCE_MPC,
                DEFAULT_YAW,
                DEFAULT_PITCH,
                INSPECTOR_MIN_DISTANCE_MPC,
                INSPECTOR_MAX_DISTANCE_MPC,
                250.0,
            ),
            selected: None,
        }
    }

    /// Click-select: project every node through the current
    /// view-projection, keep the nearest projected point within
    /// [`COSMIC_PICK_RADIUS_PX`]. Exact projected-distance ties keep the
    /// lowest node index (the map-tab rule). Stores and returns the
    /// pick. Read-only: touches nothing but `self.selected`.
    pub fn select_at(
        &mut self,
        web: &WebDescriptor,
        origin: DVec3,
        cursor: (f32, f32),
        vp: Rect,
    ) -> Option<u32> {
        let view_proj = self.camera.view_proj(vp.w / vp.h);
        let mut best: Option<(u32, f32)> = None;
        for node in &web.nodes {
            let world = Vec3::new(
                (node.position_mpc[0] - origin.x) as f32,
                (node.position_mpc[1] - origin.y) as f32,
                (node.position_mpc[2] - origin.z) as f32,
            );
            if let Some((sx, sy)) = project_to_screen(world, view_proj, vp) {
                let d = (sx - cursor.0).hypot(sy - cursor.1);
                if d <= COSMIC_PICK_RADIUS_PX
                    && best.is_none_or(|(bi, bd)| d < bd || (d == bd && node.node_index < bi))
                {
                    best = Some((node.node_index, d));
                }
            }
        }
        self.selected = best.map(|(i, _)| i);
        self.selected
    }

    /// Combined matrix the renderer pushes (test seam).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.camera.view_proj(aspect)
    }
}

impl Default for CosmicWebInspector {
    fn default() -> Self {
        Self::new()
    }
}

/// Mass-graded node tint: blue-white dwarfs → yellow-white giants.
/// Shared by the demo and inspector uploads (one palette, two surfaces).
pub fn node_color(mass_msun: f64) -> [f32; 3] {
    let l = ((mass_msun.log10() - 12.7) / (15.4 - 12.7)).clamp(0.0, 1.0) as f32;
    [0.62 + 0.38 * l, 0.70 + 0.23 * l, 1.00 - 0.28 * l]
}

/// Node pixel size from mass (2–5 px sprite floor for legibility).
pub fn node_size_px(mass_msun: f64) -> f32 {
    let l = ((mass_msun.log10() - 12.7) / (15.4 - 12.7)).clamp(0.0, 1.0) as f32;
    2.0 + 3.0 * l
}

/// Halo nodes as `(position, color, misc)` tuples in origin-relative
/// Mpc f32: `misc = (pixel size, alpha, kind 0)`.
pub fn node_point_cloud(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    web.nodes
        .iter()
        .map(|node| {
            (
                [
                    (node.position_mpc[0] - origin.x) as f32,
                    (node.position_mpc[1] - origin.y) as f32,
                    (node.position_mpc[2] - origin.z) as f32,
                ],
                node_color(node.mass_msun),
                [node_size_px(node.mass_msun), 1.0, 0.0],
            )
        })
        .collect()
}

/// Dwarf glow points as tuples (`misc = (1.5 px, 0.45 alpha, kind 0)`).
pub fn glow_point_cloud(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    web.glow_mpc
        .iter()
        .map(|g| {
            (
                [
                    (f64::from(g[0]) - origin.x) as f32,
                    (f64::from(g[1]) - origin.y) as f32,
                    (f64::from(g[2]) - origin.z) as f32,
                ],
                [0.58, 0.55, 0.88],
                [1.5, 0.45, 0.0],
            )
        })
        .collect()
}

/// Filament links as flattened endpoint pairs (origin-relative Mpc f32)
/// for the `LineList` pipeline.
pub fn link_segments(web: &WebDescriptor, origin: DVec3) -> Vec<[f32; 3]> {
    let pos = |i: u32| {
        let p = web.nodes[i as usize].position_mpc;
        [
            (p[0] - origin.x) as f32,
            (p[1] - origin.y) as f32,
            (p[2] - origin.z) as f32,
        ]
    };
    let mut out = Vec::with_capacity(web.links.len() * 2);
    for link in &web.links {
        out.push(pos(link.a));
        out.push(pos(link.b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{CosmicWebParams, generate_cosmic_web};

    fn web() -> WebDescriptor {
        generate_cosmic_web(1234, &CosmicWebParams::nominal())
    }

    fn vp() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        }
    }

    #[test]
    fn inspector_opens_framed_and_unselected() {
        let inspector = CosmicWebInspector::new();
        assert_eq!(inspector.selected, None);
        assert!((inspector.camera.distance() - INSPECTOR_DISTANCE_MPC).abs() < 1e-5);
    }

    #[test]
    fn select_at_picks_the_projected_node() {
        let web = web();
        let mut inspector = CosmicWebInspector::new();
        // Click exactly on the home node's projection: some node wins,
        // and re-clicking off-sky clears nothing but may reselect.
        let home = web.home();
        let world = Vec3::new(
            home.position_mpc[0] as f32,
            home.position_mpc[1] as f32,
            home.position_mpc[2] as f32,
        );
        let view_proj = inspector.view_proj(800.0 / 600.0);
        let (sx, sy) = project_to_screen(world, view_proj, vp()).expect("home node must project");
        let picked = inspector.select_at(&web, DVec3::ZERO, (sx, sy), vp());
        assert!(picked.is_some(), "click on a node must select");
        let idx = picked.expect("checked above");
        assert!(idx < web.nodes.len() as u32);
    }

    #[test]
    fn select_at_empty_web_selects_nothing() {
        let web = WebDescriptor::new(7, Vec::new(), Vec::new(), Vec::new(), 0, 0.0);
        let mut inspector = CosmicWebInspector::new();
        assert_eq!(
            inspector.select_at(&web, DVec3::ZERO, (400.0, 300.0), vp()),
            None
        );
        assert_eq!(inspector.selected, None);
    }

    #[test]
    fn clouds_cover_nodes_links_and_glow() {
        let web = web();
        let origin = DVec3::ZERO;
        assert_eq!(node_point_cloud(&web, origin).len(), web.nodes.len());
        assert_eq!(glow_point_cloud(&web, origin).len(), web.glow_mpc.len());
        assert_eq!(link_segments(&web, origin).len(), web.links.len() * 2);
        // Mass grading: the heaviest node is yellower and bigger than
        // the lightest.
        let mut by_mass = web.nodes.clone();
        by_mass.sort_by(|a, b| a.mass_msun.total_cmp(&b.mass_msun));
        let light = node_color(by_mass[0].mass_msun);
        let heavy = node_color(by_mass[by_mass.len() - 1].mass_msun);
        assert!(heavy[0] > light[0] && heavy[2] < light[2]);
        assert!(
            node_size_px(by_mass[by_mass.len() - 1].mass_msun) > node_size_px(by_mass[0].mass_msun)
        );
    }

    #[test]
    fn clouds_are_origin_relative() {
        // Layout invariant both surfaces rely on: shifting the origin
        // shifts every point by exactly the delta (rebase-safe).
        let web = web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = node_point_cloud(&web, a);
        let pb = node_point_cloud(&web, b);
        for (p, q) in pa.iter().zip(pb.iter()).take(50) {
            for axis in 0..3 {
                let delta = f64::from(p.0[axis]) - f64::from(q.0[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
    }
}
