//! Sphere Viewer screen state: validated parameters, display toggles,
//! current mesh buffers (plain data) and read-only stats. Regeneration is
//! synchronous: `generate` + both builders run on the calling thread and
//! the measured wall time is stored in [`ViewerStats::gen_ms`].

use std::time::Instant;

use game_engine::hexsphere::{DEFAULT_SUBDIVISIONS, HexSphere};
use game_engine::render::PlanetVertex;

use crate::mesh::{ViewerStats, build_fill, build_wireframe, viewer_stats};
use crate::params::{self, MAX_SUBDIVISIONS, MIN_SUBDIVISIONS, ParamsError, ValidParams};
use crate::ui::{Checkbox, Slider, TextField};

/// Default radius shown in the panel.
pub const DEFAULT_RADIUS: f32 = 1.0;
/// Default radius field text.
pub const DEFAULT_RADIUS_TEXT: &str = "1.0";

/// Owned Sphere Viewer state (mesh + camera inputs + panel texts).
pub struct SphereViewerState {
    /// Subdivisions field (validated on regenerate).
    pub subdiv_field: TextField,
    /// Radius field (validated on regenerate).
    pub radius_field: TextField,
    /// Subdivisions slider (synced with the field; slider drags clamp).
    pub subdiv_slider: Slider,
    /// Wireframe checkbox (synced to [`SphereViewerState::wireframe`]).
    pub wire_cb: Checkbox,
    /// Pentagon-highlight checkbox (synced to [`SphereViewerState::pentagons`]).
    pub pent_cb: Checkbox,
    /// Last successfully applied subdivisions.
    pub subdiv: u32,
    /// Last successfully applied radius.
    pub radius: f32,
    /// Wireframe overlay toggle (default on).
    pub wireframe: bool,
    /// Pentagon highlight toggle (default on).
    pub pentagons: bool,
    /// Fill vertices (cell centers first, then corners).
    pub fill_vertices: Vec<PlanetVertex>,
    /// Fill index buffer (triangle list).
    pub fill_indices: Vec<u32>,
    /// Wireframe position pairs (segment k = `lines[2k]` → `lines[2k+1]`).
    pub lines: Vec<[f32; 3]>,
    /// Read-only stats, refreshed after each regeneration.
    pub stats: ViewerStats,
}

impl SphereViewerState {
    /// Default panel (N=6, R=1.0, both toggles on) with a built mesh.
    pub fn new() -> Self {
        let mut state = SphereViewerState {
            subdiv_field: TextField::new(&DEFAULT_SUBDIVISIONS.to_string()),
            radius_field: TextField::new(DEFAULT_RADIUS_TEXT),
            subdiv_slider: Slider::new(MIN_SUBDIVISIONS, MAX_SUBDIVISIONS, DEFAULT_SUBDIVISIONS),
            wire_cb: Checkbox { checked: true },
            pent_cb: Checkbox { checked: true },
            subdiv: DEFAULT_SUBDIVISIONS,
            radius: DEFAULT_RADIUS,
            wireframe: true,
            pentagons: true,
            fill_vertices: Vec::new(),
            fill_indices: Vec::new(),
            lines: Vec::new(),
            stats: ViewerStats {
                cells: 0,
                corners: 0,
                pentagons: 0,
                hash8: String::new(),
                gen_ms: 0.0,
            },
        };
        state
            .regenerate()
            .expect("defaults are valid and must build");
        state
    }

    /// Validate the field texts; on success rebuild the mesh and stats,
    /// on failure leave the current mesh untouched.
    pub fn regenerate(&mut self) -> Result<ValidParams, ParamsError> {
        let applied = params::validate(&self.subdiv_field.text, &self.radius_field.text)?;
        let started = Instant::now();
        let mesh = HexSphere::generate(applied.subdivisions, applied.radius);
        let (fill_vertices, fill_indices) = build_fill(&mesh);
        let lines = build_wireframe(&mesh);
        let gen_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.subdiv = applied.subdivisions;
        self.radius = applied.radius;
        self.fill_vertices = fill_vertices;
        self.fill_indices = fill_indices;
        self.lines = lines;
        self.stats = viewer_stats(&mesh, gen_ms);
        Ok(applied)
    }

    /// Current field texts: Regenerate enabled only when both validate.
    pub fn can_regenerate(&self) -> bool {
        params::can_regenerate(&self.subdiv_field.text, &self.radius_field.text)
    }

    /// After typing in the subdivisions field: snap the slider knob when
    /// the text parses (slider drags clamp by construction).
    pub fn sync_slider_from_field(&mut self) {
        if let Ok(n) = params::parse_subdivisions(&self.subdiv_field.text) {
            self.subdiv_slider.value = n;
        }
    }

    /// After a slider drag: mirror the value back into the field text.
    pub fn sync_field_from_slider(&mut self) {
        self.subdiv_field.text = self.subdiv_slider.value.to_string();
    }

    /// After checkbox clicks: mirror into the draw toggles.
    pub fn sync_toggles(&mut self) {
        self.wireframe = self.wire_cb.checked;
        self.pentagons = self.pent_cb.checked;
    }
}

impl Default for SphereViewerState {
    fn default() -> Self {
        SphereViewerState::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::SubdivError;

    #[test]
    fn defaults_build_high_tier_stats() {
        let state = SphereViewerState::new();
        assert_eq!(state.subdiv, 6);
        assert_eq!(state.radius, 1.0);
        assert!(state.wireframe && state.pentagons);
        assert_eq!(state.stats.cells, 40962);
        assert_eq!(state.stats.corners, 81920);
        assert_eq!(state.stats.pentagons, 12);
        assert_eq!(state.fill_indices.len() % 3, 0);
        assert_eq!(state.lines.len() % 2, 0);
        assert!(state.stats.gen_ms >= 0.0);
    }

    #[test]
    fn regenerate_applies_valid_edits() {
        let mut state = SphereViewerState::new();
        state.subdiv_field.text = "4".to_owned();
        state.radius_field.text = "2.0".to_owned();
        assert!(state.can_regenerate());
        let applied = state.regenerate().unwrap();
        assert_eq!(
            applied,
            ValidParams {
                subdivisions: 4,
                radius: 2.0
            }
        );
        assert_eq!(state.stats.cells, 2562);
        assert_eq!(state.stats.corners, 5120);
        assert_eq!(state.stats.pentagons, 12);
    }

    #[test]
    fn regenerate_rejects_without_touching_mesh() {
        let mut state = SphereViewerState::new();
        let before = state.stats.clone();
        state.subdiv_field.text = "9".to_owned();
        state.radius_field.text = "0".to_owned();
        assert!(!state.can_regenerate());
        assert_eq!(
            state.regenerate(),
            Err(ParamsError::Subdiv(SubdivError::OutOfRange))
        );
        assert_eq!(state.stats, before);
        assert_eq!(state.subdiv, 6);
    }

    #[test]
    fn stats_hash_changes_with_params() {
        let mut state = SphereViewerState::new();
        let h6 = state.stats.hash8.clone();
        state.subdiv_field.text = "3".to_owned();
        state.regenerate().unwrap();
        assert_ne!(state.stats.hash8, h6);
        assert_eq!(state.stats.hash8.len(), 8);
    }

    #[test]
    fn widget_sync_roundtrips() {
        let mut state = SphereViewerState::new();
        state.subdiv_field.text = "4".to_owned();
        state.sync_slider_from_field();
        assert_eq!(state.subdiv_slider.value, 4);
        state.subdiv_field.text = "99".to_owned();
        state.sync_slider_from_field();
        assert_eq!(state.subdiv_slider.value, 4);
        state.subdiv_slider.value = 2;
        state.sync_field_from_slider();
        assert_eq!(state.subdiv_field.text, "2");
        state.wire_cb.checked = false;
        state.sync_toggles();
        assert!(!state.wireframe && state.pentagons);
    }
}
