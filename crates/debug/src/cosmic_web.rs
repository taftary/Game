//! Cosmic Web dimension tab: read-only inspector over the shared stage-0
//! descriptor (WS5 — the fourth absorbed view).
//!
//! [`CosmicWebInspector`] owns an orbit/pan/zoom camera (Mpc units,
//! `MapOrbitCamera` policy) plus a node selection. It never mutates
//! journey, transit, or viewer state — selection is local readout state
//! (pinned by `selection_is_read_only` in `app.rs`).
//!
//! This module also owns the CPU-side vertex layout both 3D surfaces
//! share ([`link_segments`]): one source of truth for positions
//! (origin-relative Mpc f32). The binary maps the tuples onto its GPU
//! vertex types at upload. (The link-graph decoration layer — grain,
//! beads, braids, smoke, impostors, glow veil — retired across v0.3.3
//! by the field render; see `cosmic-gas-veil-v2` CGV-009.)
//!
//! The enrichment layer is render-only: every value derives
//! deterministically from the descriptor + seed (never fed back into
//! selection, flight, or saves), so the stage-0 descriptor, its hash,
//! and `UNIVERSE_VERSION` are untouched.

use super::cosmic_window::SlabState;
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
/// Inspector zoom band, Mpc. Sized so the slab-mode 20° FOV rescale
/// (×3.27 distance at the default framing) never clamps:
/// 430 × 3.27 ≈ 1407 < 1600.
pub const INSPECTOR_MAX_DISTANCE_MPC: f32 = 1600.0;

/// Read-only inspector state for the Cosmic Web dimension tab.
pub struct CosmicWebInspector {
    /// Orbit/pan/zoom camera in Mpc (inspector owns its view; the demo
    /// player camera is untouched).
    pub camera: MapOrbitCamera,
    /// Selected node index (readout only — never fed back anywhere).
    pub selected: Option<u32>,
    /// Slab window state (`cosmic-depth-window`): thin-slice view.
    pub slab: SlabState,
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
            slab: SlabState::off(),
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

// ---------------------------------------------------------------------------
// Retired enrichment layer (`cosmic-gas-veil-v2` CGV-009): the braid /
// spine / strand / smoke helpers and the descriptor-glow veil are
// gone — the field render (tracer splats + hub impostors + grid veil)
// replaces the link-graph decoration. What remains in this module is
// the inspector (camera + selection), link segments, and the shared
// redshift knob.
// ---------------------------------------------------------------------------

/// Exaggerated Hubble redshift strength per Mpc of view depth for the
/// cosmic glow shaders (spec §9.1 depth cue, artistically boosted: at
/// 250 Mpc the exaggerated depth is 0.5 — distant filaments redden
/// gently). Halved in update-2026-09-19-1933 (was 0.004, saturating
/// at 125 Mpc): the softer ramp keeps the depth cue readable while
/// letting golden hubs survive at depth. Single tuning knob, shared
/// by both cosmic surfaces.
pub const COSMIC_REDSHIFT_PER_MPC: f32 = 0.002;

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
    fn select_at_round_trips_at_both_slab_fovs() {
        // CDW A-2: picking and drawing share one matrix at any FOV —
        // clicking a node's projection selects it at 60° and at 20°.
        let web = web();
        let home = web.home();
        let world = Vec3::new(
            home.position_mpc[0] as f32,
            home.position_mpc[1] as f32,
            home.position_mpc[2] as f32,
        );
        for fov in [60.0, 20.0] {
            let mut inspector = CosmicWebInspector::new();
            inspector.camera.set_fov_keep_framing(fov);
            let view_proj = inspector.view_proj(800.0 / 600.0);
            let (sx, sy) =
                project_to_screen(world, view_proj, vp()).expect("home node must project");
            let picked = inspector.select_at(&web, DVec3::ZERO, (sx, sy), vp());
            assert!(
                picked.is_some(),
                "click on the home node must select at {fov}°"
            );
        }
    }

    #[test]
    fn clouds_cover_nodes_links_and_glow() {
        let web = web();
        let origin = DVec3::ZERO;
        assert_eq!(link_segments(&web, origin).len(), web.links.len() * 2);
    }

    #[test]
    fn enrichment_layouts_are_finite_and_bounded() {
        // GPU-debug aid (visual-issue round 2): scans every emitted
        // vertex of the nominal web for non-finite or out-of-band
        // values. The shaders assume finite inputs with sane
        // magnitudes — Inf/NaN here would decorrelate color channels
        // through the additive chain into rainbow squares on screen.
        let web = web();
        let origin = DVec3::ZERO;
        let points = link_segments(&web, origin)
            .into_iter()
            .map(|p| (p, [1.0_f32; 3], [1.0_f32; 3]))
            .collect::<Vec<_>>();
        assert!(!points.is_empty());
        for (pos, color, misc) in &points {
            for axis in 0..3 {
                assert!(pos[axis].is_finite(), "point pos not finite");
                assert!(
                    color[axis].is_finite() && (0.0..=5.0).contains(&color[axis]),
                    "point color out of band: {color:?}"
                );
            }
            assert!(
                misc[0].is_finite() && (0.0..=300.0).contains(&misc[0]),
                "sprite size out of band: {misc:?}"
            );
            assert!(
                misc[1].is_finite() && (0.0..=1.0).contains(&misc[1]),
                "sprite alpha out of band: {misc:?}"
            );
            assert!(
                misc[2] == 0.0 || misc[2] == 1.0,
                "sprite kind must be 0 or 1: {misc:?}"
            );
        }
    }

    #[test]
    fn clouds_are_origin_relative() {
        // Layout invariant both surfaces rely on: shifting the origin
        // shifts every point by exactly the delta (rebase-safe).
        let web = web();
        let a = DVec3::ZERO;
        let b = DVec3::new(10.0, -4.0, 2.0);
        let pa = link_segments(&web, a);
        let pb = link_segments(&web, b);
        for (p, q) in pa.iter().zip(pb.iter()).take(50) {
            for axis in 0..3 {
                let delta = f64::from(p[axis]) - f64::from(q[axis]);
                let want = [b.x - a.x, b.y - a.y, b.z - a.z][axis];
                assert!(
                    (delta - want).abs() < 1e-3,
                    "axis {axis}: {delta} vs {want}"
                );
            }
        }
    }

    #[test]
    fn veil_sprite_contrast_subset_exists_and_stays_brighter() {
        // Density-contrast pass (target: brightness ~= mass density):
        // a deterministic subset of dense-link core puffs must read
        // near-white (all channels high, low saturation) while faint
        // smoke stays dim; dense-link alpha must clearly exceed
        // faint-link alpha so voids survive the far-field stack.
        // RETIRED (`cosmic-gas-veil-v2` CGV-009): smoke is gone — this
        // test now pins the veil sprite contrast instead (dense cells
        // outshine faint ones, faint alpha stays tiny).
        use super::super::cosmic_veil::{veil_alpha, veil_sprites};
        use game_engine::universe::{WebFieldBudget, generate_cosmic_web_with_field};
        let params = CosmicWebParams::nominal();
        let (_, field) = generate_cosmic_web_with_field(1234, &params, WebFieldBudget::Full);
        let sprites = veil_sprites(&field, DVec3::ZERO);
        assert!(!sprites.is_empty());
        let dense_a: f64 = sprites
            .iter()
            .map(|(_, _, misc)| f64::from(misc[1]))
            .sum::<f64>()
            / sprites.len() as f64;
        assert!(dense_a <= 0.05, "veil alpha must stay tiny: mean {dense_a}");
        assert!(
            veil_alpha(64.0) > veil_alpha(1.0),
            "dense cells must outshine faint ones"
        );
    }
}
