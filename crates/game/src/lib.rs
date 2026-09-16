//! `game` library: player movement, switchable cameras, tick-based
//! chunk streaming, and the camera-journey state machine — pure logic,
//! no window, no GPU.
//!
//! The `game` binary runs a headless scripted demo over these modules;
//! `game_debug` reuses them for the interactive player overlay.
//! `game` never touches `vulkano`: rendering lives in `engine::render`.

pub mod camera;
pub mod journey;
pub mod player;
pub mod streaming;
pub mod transit;
