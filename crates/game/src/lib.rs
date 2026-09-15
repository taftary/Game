//! `game` library: player movement, switchable cameras, and
//! tick-based chunk streaming — pure logic, no window, no GPU.
//!
//! The `game` binary runs a headless scripted demo over these modules;
//! `game_debug` reuses them for the interactive player overlay.

pub mod camera;
pub mod player;
pub mod streaming;
