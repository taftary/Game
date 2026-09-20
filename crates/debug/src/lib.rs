//! Developer debug screens: monitor, debug, and check the running application.
//!
//! One OS window, owned by the `game_debug` binary: [`app::Screen`] —
//! the Game Demo tab, the ten dimension tabs behind the Dimensions
//! dropdown, and Settings — over the shared content state (journey,
//! galaxy/system maps, planet viewer). The dev widget
//! ([`fps`]/[`console`]/[`inspector`]) floats over every tab. All
//! screen logic here is window- and GPU-free — the `game_debug`
//! binary owns the winit + vulkano shell.
//! Never player-facing; strip or gate before any release.

pub mod actions;
pub mod app;
pub mod console;
pub mod cosmic_camera;
pub mod cosmic_capture;
pub mod cosmic_demo;
pub mod cosmic_player;
pub mod cosmic_web;
pub mod fps;
pub mod fx;
pub mod galaxy_map;
pub mod inspector;
pub mod loader;
pub mod map_camera;
pub mod mesh;
pub mod params;
pub mod picking;
pub mod planet_viewer;
pub mod player_view;
pub mod sky;
pub mod system_map;
pub mod text;
pub mod ui;
