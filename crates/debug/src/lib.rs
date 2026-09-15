//! Developer debug screens: monitor, debug, and check the running application.
//!
//! The Sphere Viewer screen (`app` + `sphere_viewer`) renders the current
//! `HexSphere` with orbit camera, wireframe and pentagon-highlight toggles,
//! validated subdivision/radius inputs and read-only stats; `fps` /
//! `console` / `inspector` remain placeholder stubs. All screen logic here
//! is window- and GPU-free — the `game_debug` binary owns the winit +
//! vulkano shell.
//! Never player-facing; strip or gate before any release.

pub mod app;
pub mod console;
pub mod fps;
pub mod inspector;
pub mod mesh;
pub mod params;
pub mod picking;
pub mod player_view;
pub mod sphere_viewer;
pub mod text;
pub mod ui;
