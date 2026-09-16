//! Developer debug screens: monitor, debug, and check the running application.
//!
//! Two OS windows, owned by the `game_debug` binary: the viewer window
//! (`app::MainScreen` — Sphere Viewer + UV Net over `sphere_viewer`
//! state: orbit camera, wireframe and pentagon-highlight toggles,
//! validated subdivision/radius inputs and read-only stats) and the
//! tools window (`app::ToolsScreen` — `fps` live frame-health plus the
//! `console` / `inspector` placeholder stubs). All screen logic here is
//! window- and GPU-free — the `game_debug` binary owns the winit +
//! vulkano shell.
//! Never player-facing; strip or gate before any release.

pub mod app;
pub mod console;
pub mod fps;
pub mod fx;
pub mod galaxy_map;
pub mod inspector;
pub mod mesh;
pub mod params;
pub mod picking;
pub mod player_view;
pub mod sphere_viewer;
pub mod system_map;
pub mod text;
pub mod ui;
