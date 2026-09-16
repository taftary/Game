//! Sphere Viewer screen state: validated parameters, display toggles,
//! current mesh buffers (plain data) and read-only stats. Regeneration is
//! synchronous: `generate` + both builders run on the calling thread and
//! the measured wall time is stored in [`ViewerStats::gen_ms`].

use std::time::Instant;

use game_engine::hexsphere::{ChunkId, DEFAULT_SUBDIVISIONS, HexSphere};
use game_engine::render::{FlatUnwrap, PlanetVertex, orbit_viewpoint};

use crate::mesh::{
    ChunkFlatVertex, FillDebug, ViewerStats, build_chunk_flat, build_chunk_flat_wireframe,
    build_fill, build_fill_debug, build_wireframe, viewer_stats,
};
use crate::params::{self, MAX_SUBDIVISIONS, MIN_SUBDIVISIONS, ParamsError, ValidParams};
use crate::player_view::PlayerViewState;
use crate::ui::{Checkbox, Slider, TextField};

/// Default radius shown in the panel.
pub const DEFAULT_RADIUS: f32 = 1.0;
/// Default radius field text.
pub const DEFAULT_RADIUS_TEXT: &str = "1.0";
/// Default checker density for the UV Checker debug mode.
pub const DEFAULT_CHECKER_DENSITY: u32 = 8;
/// Checker density slider range.
pub const MIN_CHECKER_DENSITY: u32 = 2;
/// Checker density slider range.
pub const MAX_CHECKER_DENSITY: u32 = 32;

/// Which view fills the main viewport; a secondary view renders in the
/// panel-top preview thumb. Click, `U` and the Swap button cycle it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewFocus {
    /// 3D sphere main, UV net in the thumb.
    #[default]
    SphereMain,
    /// UV net main, 3D sphere in the thumb.
    UvMain,
    /// Flat chunk map main, 3D sphere in the thumb.
    ChunkFlat,
}

impl ViewFocus {
    /// Cycle main focus: sphere → UV net → chunk flat → sphere.
    pub fn toggle(self) -> Self {
        match self {
            ViewFocus::SphereMain => ViewFocus::UvMain,
            ViewFocus::UvMain => ViewFocus::ChunkFlat,
            ViewFocus::ChunkFlat => ViewFocus::SphereMain,
        }
    }

    /// Short label for the left-dock focus readout.
    pub fn title(self) -> &'static str {
        match self {
            ViewFocus::SphereMain => "3D sphere",
            ViewFocus::UvMain => "UV net",
            ViewFocus::ChunkFlat => "chunk flat",
        }
    }
}

/// Debug fragment-shader visualization (panel selector, cycles on click).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DebugMode {
    /// Lambert + pentagon tint (existing look).
    #[default]
    Lit,
    /// World normal as color.
    Normal,
    /// Pentagon valence tint only.
    Tint,
    /// Cube-domain checker (Bourke cubemap + equiangular): only squares,
    /// globally consistent grid (4 matched fault edges), shared by both
    /// views.
    Checker,
    /// Seam highlight + island-id color.
    Seams,
    /// Longitude/latitude gradient (pinch detector).
    LonLat,
}

impl DebugMode {
    /// All modes in panel cycle order.
    pub const ALL: [DebugMode; 6] = [
        DebugMode::Lit,
        DebugMode::Normal,
        DebugMode::Tint,
        DebugMode::Checker,
        DebugMode::Seams,
        DebugMode::LonLat,
    ];

    /// Panel label.
    pub fn title(self) -> &'static str {
        match self {
            DebugMode::Lit => "Lit",
            DebugMode::Normal => "Normal",
            DebugMode::Tint => "Tint",
            DebugMode::Checker => "Checker",
            DebugMode::Seams => "Seams",
            DebugMode::LonLat => "LonLat",
        }
    }

    /// Push-constant id consumed by the debug fragment shader.
    pub fn index(self) -> u32 {
        match self {
            DebugMode::Lit => 0,
            DebugMode::Normal => 1,
            DebugMode::Tint => 2,
            DebugMode::Checker => 3,
            DebugMode::Seams => 4,
            DebugMode::LonLat => 5,
        }
    }

    /// Next mode in cycle order (wraps).
    pub fn cycle(self) -> Self {
        let i = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

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
    /// Seam-highlight checkbox (synced to [`SphereViewerState::seams`]).
    pub seam_cb: Checkbox,
    /// Wireframe-on-UV checkbox (synced to [`SphereViewerState::wire_on_uv`]).
    pub uvwire_cb: Checkbox,
    /// Checker density slider (synced to [`SphereViewerState::checker_density`]).
    pub density_slider: Slider,
    /// Which view fills the main viewport (default sphere).
    pub focus: ViewFocus,
    /// Active debug shader visualization (default lit).
    pub debug_mode: DebugMode,
    /// Checker tiling for [`DebugMode::Checker`] (default 8).
    pub checker_density: u32,
    /// Seam highlight toggle (default on).
    pub seams: bool,
    /// Wireframe overlay on the flat UV view (default on).
    pub wire_on_uv: bool,
    /// Last successfully applied subdivisions.
    pub subdiv: u32,
    /// Last successfully applied radius.
    pub radius: f32,
    /// Current mesh (hover picking reads centers/neighbors straight from
    /// it; the binary never re-derives topology from the GPU buffers).
    pub mesh: HexSphere,
    /// Wireframe overlay toggle (default on).
    pub wireframe: bool,
    /// Pentagon highlight toggle (default on).
    pub pentagons: bool,
    /// Fill vertices (cell centers first, then corners).
    pub fill_vertices: Vec<PlanetVertex>,
    /// Fill index buffer (triangle list).
    pub fill_indices: Vec<u32>,
    /// Debug sidecar (seam/island per fill vertex, same order).
    pub fill_debug: FillDebug,
    /// Flat-view buffers with seam duplication (`render::uv`): expanded
    /// uv/island/seam + dedicated index buffer where every triangle stays
    /// inside one island. `source` maps flat vertices back to
    /// `fill_vertices` for attributes.
    pub flat: FlatUnwrap,
    /// Wireframe position pairs (segment k = `lines[2k]` → `lines[2k+1]`).
    pub lines: Vec<[f32; 3]>,
    /// Flat-view wireframe pairs in icosa-net UV space.
    pub uv_lines: Vec<[f32; 2]>,
    /// Hemisphere viewpoint for the flat chunk map: a point on the
    /// sphere (length == radius). Arrow keys orbit it; the flat view
    /// shows the gnomonic projection of its half.
    pub chunk_flat_viewpoint: [f32; 3],
    /// Visible cell ids in the current hemisphere (picking map).
    pub chunk_flat_cells: Vec<u32>,
    /// Normalized `[0, 1]²` centers aligned with `chunk_flat_cells`.
    pub chunk_flat_centers: Vec<[f32; 2]>,
    /// Flat chunk-map fill vertices (cell-by-cell fans).
    pub chunk_flat_vertices: Vec<ChunkFlatVertex>,
    /// Flat chunk-map index buffer (triangle list).
    pub chunk_flat_indices: Vec<u32>,
    /// Flat chunk-map boundary wireframe pairs.
    pub chunk_flat_wire: Vec<[f32; 2]>,
    /// Chunk under the cursor right now (set by the binary on cursor
    /// moves over a sphere-rendered rect; `None` elsewhere).
    pub hovered: Option<ChunkId>,
    /// Click-pinned chunk: sticky highlight + info, survives mouse-leave
    /// until toggled off or dropped by regeneration.
    pub pinned: Option<ChunkId>,
    /// Neighbor count per chunk, rebuilt on regenerate (5 = pentagon,
    /// 6 = hexagon): the panel CHUNK section's type/neighbor source.
    pub cell_sides: Vec<u8>,
    /// Player overlay: keyboard-driven walker + player cameras + chunk
    /// streaming. Reset to the applied radius on regenerate (keeping the
    /// active flag); the binary renders through it while active.
    pub player: PlayerViewState,
    /// Read-only stats, refreshed after each regeneration.
    pub stats: ViewerStats,
}

impl SphereViewerState {
    /// Default panel (N=6, R=1.0, both toggles on) with a built mesh.
    /// This is also the headless default: N=6 matches the engine pin, so
    /// the headless stats cross-check the committed mesh hash.
    pub fn new() -> Self {
        SphereViewerState::with_values(DEFAULT_SUBDIVISIONS, DEFAULT_RADIUS)
    }

    /// Panel with explicit values (both toggles on) and a built mesh.
    /// `subdivisions` is clamped to the 0–8 panel range by the slider;
    /// `radius` must be `> 0` and finite (the generator panics otherwise).
    pub fn with_values(subdivisions: u32, radius: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "viewer radius must be positive and finite, got {radius}"
        );
        let mut state = SphereViewerState {
            // Placeholder mesh: replaced by `regenerate()` below before
            // anyone can observe it (N=0 builds 12 cells in microseconds).
            mesh: HexSphere::generate(0, 1.0),
            subdiv_field: TextField::new(&subdivisions.to_string()),
            radius_field: TextField::new(&radius.to_string()),
            subdiv_slider: Slider::new(MIN_SUBDIVISIONS, MAX_SUBDIVISIONS, subdivisions),
            wire_cb: Checkbox { checked: true },
            pent_cb: Checkbox { checked: true },
            seam_cb: Checkbox { checked: true },
            uvwire_cb: Checkbox { checked: true },
            density_slider: Slider::new(
                MIN_CHECKER_DENSITY,
                MAX_CHECKER_DENSITY,
                DEFAULT_CHECKER_DENSITY,
            ),
            focus: ViewFocus::SphereMain,
            debug_mode: DebugMode::Lit,
            checker_density: DEFAULT_CHECKER_DENSITY,
            seams: true,
            wire_on_uv: true,
            subdiv: DEFAULT_SUBDIVISIONS,
            radius: DEFAULT_RADIUS,
            wireframe: true,
            pentagons: true,
            fill_vertices: Vec::new(),
            fill_indices: Vec::new(),
            uv_lines: Vec::new(),
            fill_debug: FillDebug {
                seam: Vec::new(),
                island: Vec::new(),
            },
            chunk_flat_viewpoint: [0.0, 1.0, 0.0],
            chunk_flat_cells: Vec::new(),
            chunk_flat_centers: Vec::new(),
            chunk_flat_vertices: Vec::new(),
            chunk_flat_indices: Vec::new(),
            chunk_flat_wire: Vec::new(),
            flat: FlatUnwrap {
                uv: Vec::new(),
                island: Vec::new(),
                seam: Vec::new(),
                indices: Vec::new(),
                source: Vec::new(),
            },
            lines: Vec::new(),
            hovered: None,
            pinned: None,
            cell_sides: Vec::new(),
            player: PlayerViewState::new(radius),
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
            .expect("constructed values are valid and must build");
        state
    }

    /// Validate the field texts; on success rebuild the mesh and stats,
    /// on failure leave the current mesh untouched.
    pub fn regenerate(&mut self) -> Result<ValidParams, ParamsError> {
        let applied = params::validate(&self.subdiv_field.text, &self.radius_field.text)?;
        let started = Instant::now();
        let mesh = HexSphere::generate(applied.subdivisions, applied.radius);
        let (fill_vertices, fill_indices) = build_fill(&mesh);
        let fill_debug = build_fill_debug(&mesh);
        let flat = game_engine::render::build_flat_unwrap(&mesh);
        let lines = build_wireframe(&mesh);
        let uv_lines = game_engine::render::build_wireframe_uv_clipped(&mesh);
        let cell_sides = (0..mesh.cell_count() as u32)
            .map(|cell| mesh.cell_neighbor_count(cell) as u8)
            .collect();
        let gen_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.subdiv = applied.subdivisions;
        self.radius = applied.radius;
        self.fill_vertices = fill_vertices;
        self.fill_indices = fill_indices;
        self.fill_debug = fill_debug;
        self.flat = flat;
        self.lines = lines;
        self.uv_lines = uv_lines;
        self.cell_sides = cell_sides;
        // Fresh mesh, fresh pointing: hover never survives a rebuild,
        // and pins survive only if the id still names a real chunk.
        self.hovered = None;
        if self
            .pinned
            .is_some_and(|pin| (pin.index() as usize) >= mesh.cell_count())
        {
            self.pinned = None;
        }
        // Fresh mesh, fresh viewpoint: reset to the north pole, then
        // load its hemisphere. The player restarts on the new planet
        // (same mode) so streaming ids never go stale across meshes.
        self.stats = viewer_stats(&mesh, gen_ms);
        self.mesh = mesh;
        self.player.reset(applied.radius);
        self.chunk_flat_viewpoint = [0.0, self.radius, 0.0];
        self.rebuild_chunk_flat();
        Ok(applied)
    }

    /// Rebuild the flat hemisphere buffers for the current viewpoint:
    /// unloads the old half, loads the new one. Called by
    /// [`SphereViewerState::regenerate`] and every arrow-key orbit step.
    pub fn rebuild_chunk_flat(&mut self) {
        let (vertices, indices, cells, centers) =
            build_chunk_flat(&self.mesh, self.chunk_flat_viewpoint);
        self.chunk_flat_vertices = vertices;
        self.chunk_flat_indices = indices;
        self.chunk_flat_cells = cells;
        self.chunk_flat_centers = centers;
        self.chunk_flat_wire = build_chunk_flat_wireframe(&self.mesh, self.chunk_flat_viewpoint);
        // Hover never survives a load/unload cycle. Pins are chunk
        // identities, not loaded state: they survive even when the
        // pinned chunk is outside the current half (the flat highlight
        // simply has nothing to mark until it loads again).
        self.hovered = None;
    }

    /// Arrow-key orbit of the flat-map viewpoint: `yaw` turns around
    /// world Y (Left/Right), `pitch` around the local tangent-right
    /// axis (Up/Down). Reloads the hemisphere buffers.
    pub fn orbit_chunk_flat(&mut self, yaw: f32, pitch: f32) {
        self.chunk_flat_viewpoint = orbit_viewpoint(self.chunk_flat_viewpoint, yaw, pitch);
        // Keep the viewpoint exactly on the sphere (f32 drift).
        let len = (self.chunk_flat_viewpoint[0] * self.chunk_flat_viewpoint[0]
            + self.chunk_flat_viewpoint[1] * self.chunk_flat_viewpoint[1]
            + self.chunk_flat_viewpoint[2] * self.chunk_flat_viewpoint[2])
            .sqrt()
            .max(1e-6);
        let scale = self.radius / len;
        for v in self.chunk_flat_viewpoint.iter_mut() {
            *v *= scale;
        }
        self.rebuild_chunk_flat();
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
        self.seams = self.seam_cb.checked;
        self.wire_on_uv = self.uvwire_cb.checked;
    }

    /// After a density slider drag: mirror into the checker density.
    pub fn sync_density_from_slider(&mut self) {
        self.checker_density = self.density_slider.value;
    }

    /// Flip which view fills the main viewport.
    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.toggle();
    }

    /// Click routing for chunk pinning: clicking the pinned chunk unpins
    /// it, clicking another chunk pins it instead (replacing the old pin).
    pub fn toggle_pin(&mut self, chunk: ChunkId) {
        self.pinned = if self.pinned == Some(chunk) {
            None
        } else {
            Some(chunk)
        };
    }

    /// Chunk selection shown in the panel CHUNK section and highlighted
    /// in the viewport: the pin wins over a fleeting hover.
    pub fn shown_chunk(&self) -> Option<ChunkId> {
        self.pinned.or(self.hovered)
    }

    /// Neighbor count of a chunk (5 = pentagon, 6 = hexagon) from the
    /// regenerate-time sidecar. `None` for ids outside the current mesh
    /// (stale pins from a bigger mesh can never reach here — regenerate
    /// drops them — but callers pass ids across meshes cheaply).
    pub fn chunk_sides(&self, chunk: ChunkId) -> Option<u8> {
        self.cell_sides.get(chunk.index() as usize).copied()
    }

    /// Advance the debug shader visualization.
    pub fn cycle_debug_mode(&mut self) {
        self.debug_mode = self.debug_mode.cycle();
    }

    /// Preview thumb label for the current focus (`V` cycles focus;
    /// `U` toggles player mode).
    pub fn thumb_label(&self) -> &'static str {
        match self.focus {
            ViewFocus::SphereMain => "UV — click/V to expand",
            ViewFocus::UvMain => "3D — click/V to restore",
            ViewFocus::ChunkFlat => "3D — click/V to restore",
        }
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
        assert!(state.seams && state.wire_on_uv);
        assert_eq!(state.focus, ViewFocus::SphereMain);
        assert_eq!(state.debug_mode, DebugMode::Lit);
        assert_eq!(state.checker_density, DEFAULT_CHECKER_DENSITY);
        assert_eq!(state.fill_debug.seam.len(), state.fill_vertices.len());
        assert_eq!(state.fill_debug.island.len(), state.fill_vertices.len());
        assert!(state.hovered.is_none() && state.pinned.is_none());
        assert_eq!(state.cell_sides.len(), state.stats.cells);
        assert_eq!(state.cell_sides.iter().filter(|&&s| s == 5).count(), 12);
        assert!(state.cell_sides.iter().all(|&s| s == 5 || s == 6));
        assert_eq!(state.uv_lines.len() % 2, 0);
        assert!(state.uv_lines.len() >= state.lines.len());
        assert_eq!(state.flat.indices.len() % 3, 0);
        assert!(state.flat.uv.len() >= state.fill_vertices.len());
        assert_eq!(state.flat.source.len(), state.flat.uv.len());
        assert_eq!(state.chunk_flat_viewpoint, [0.0, 1.0, 0.0]);
        assert_eq!(state.chunk_flat_cells.len(), state.chunk_flat_centers.len());
        assert!(!state.chunk_flat_cells.is_empty());
        assert!(state.chunk_flat_cells.len() < state.stats.cells);
        assert_eq!(state.chunk_flat_indices.len() % 3, 0);
        assert_eq!(state.chunk_flat_wire.len() % 2, 0);
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
        state.seam_cb.checked = false;
        state.uvwire_cb.checked = false;
        state.sync_toggles();
        assert!(!state.seams && !state.wire_on_uv);
        state.density_slider.value = 16;
        state.sync_density_from_slider();
        assert_eq!(state.checker_density, 16);
    }

    #[test]
    fn pin_toggles_with_hover_precedence() {
        let mut state = SphereViewerState::with_values(1, 1.0);
        let a = HexSphere::generate(1, 1.0).chunk_id(5);
        let b = HexSphere::generate(1, 1.0).chunk_id(7);
        assert_eq!(state.shown_chunk(), None);
        state.hovered = Some(a);
        assert_eq!(state.shown_chunk(), Some(a));
        state.toggle_pin(a);
        assert_eq!(state.pinned, Some(a));
        assert_eq!(state.shown_chunk(), Some(a));
        // Pin wins over a different hover.
        state.hovered = Some(b);
        assert_eq!(state.shown_chunk(), Some(a));
        // Clicking the pinned chunk unpins; another chunk replaces.
        state.toggle_pin(a);
        assert_eq!(state.pinned, None);
        assert_eq!(state.shown_chunk(), Some(b));
        state.toggle_pin(b);
        assert_eq!(state.pinned, Some(b));
    }

    #[test]
    fn regenerate_clears_hover_and_stale_pin() {
        let mut state = SphereViewerState::new();
        let mesh = HexSphere::generate(6, 1.0);
        state.hovered = Some(mesh.chunk_id(11));
        state.pinned = Some(mesh.chunk_id(40_000));
        state.subdiv_field.text = "1".to_owned();
        state.regenerate().unwrap();
        assert_eq!(state.hovered, None);
        assert_eq!(state.pinned, None);
        assert_eq!(state.cell_sides.len(), 42);
        // In-range pins survive a rebuild.
        state.pinned = Some(mesh.chunk_id(5));
        state.subdiv_field.text = "1".to_owned();
        state.regenerate().unwrap();
        assert_eq!(state.pinned.map(ChunkId::index), Some(5));
    }

    #[test]
    fn chunk_sides_serves_panel_type_rows() {
        let state = SphereViewerState::with_values(1, 1.0);
        let mesh = HexSphere::generate(1, 1.0);
        for cell in 0..mesh.cell_count() as u32 {
            let chunk = mesh.chunk_id(cell);
            let expected = if mesh.is_pentagon(cell) { 5 } else { 6 };
            assert_eq!(state.chunk_sides(chunk), Some(expected), "cell {cell}");
        }
        // A chunk id from a bigger mesh is out of range here, never a panic.
        let big = SphereViewerState::with_values(2, 1.0);
        let stale = HexSphere::generate(2, 1.0).chunk_id(161);
        assert!(big.chunk_sides(stale).is_some());
        assert_eq!(state.chunk_sides(stale), None);
    }

    #[test]
    fn fill_centers_match_mesh_order() {
        // The hover/pin highlight reads `gl_VertexIndex` as the chunk id:
        // fan centers must upload first, in cell order. If `build_fill`
        // ever reorders vertices, the highlight (and the tint/seam/island
        // varyings riding the same provoking vertex) breaks silently.
        let state = SphereViewerState::with_values(2, 1.0);
        assert_eq!(state.mesh.cell_count(), state.stats.cells);
        for cell in 0..state.mesh.cell_count() as u32 {
            assert_eq!(
                state.fill_vertices[cell as usize].position,
                state.mesh.cell_center(cell),
                "center vertex {cell} moved"
            );
        }
    }

    #[test]
    fn orbit_reloads_a_new_hemisphere() {
        let mut state = SphereViewerState::with_values(2, 1.0);
        let before = state.chunk_flat_cells.clone();
        // Pitch PI flips north pole to south pole (yawing the pole
        // around Y is a no-op — same trap as rotating the pole itself).
        state.orbit_chunk_flat(0.0, std::f32::consts::PI);
        // Viewpoint stays on the sphere.
        let len = (state.chunk_flat_viewpoint[0] * state.chunk_flat_viewpoint[0]
            + state.chunk_flat_viewpoint[1] * state.chunk_flat_viewpoint[1]
            + state.chunk_flat_viewpoint[2] * state.chunk_flat_viewpoint[2])
            .sqrt();
        assert!((len - 1.0).abs() < 1e-5);
        // Half-turn loads the far half: mostly disjoint cells.
        let shared = before
            .iter()
            .filter(|c| state.chunk_flat_cells.contains(c))
            .count();
        assert!(shared * 4 < before.len(), "shared {shared}");
        assert_eq!(state.hovered, None);
    }

    #[test]
    fn focus_toggles_both_ways_with_labels() {
        let mut state = SphereViewerState::new();
        assert_eq!(state.thumb_label(), "UV — click/V to expand");
        state.toggle_focus();
        assert_eq!(state.focus, ViewFocus::UvMain);
        assert_eq!(state.thumb_label(), "3D — click/V to restore");
        state.toggle_focus();
        assert_eq!(state.focus, ViewFocus::ChunkFlat);
        assert_eq!(state.thumb_label(), "3D — click/V to restore");
        state.toggle_focus();
        assert_eq!(state.focus, ViewFocus::SphereMain);
    }

    #[test]
    fn debug_mode_cycles_all_six_with_stable_ids() {
        let mut state = SphereViewerState::new();
        let mut seen = Vec::new();
        for _ in 0..DebugMode::ALL.len() {
            seen.push(state.debug_mode.index());
            state.cycle_debug_mode();
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(state.debug_mode, DebugMode::Lit);
        for mode in DebugMode::ALL {
            assert!(!mode.title().is_empty());
        }
    }
}
