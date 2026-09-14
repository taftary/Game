//! Headless planet runtime: owns the scene and advances frames.

use crate::scene::Scene;

/// Frame counter of the headless runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frame(u64);

impl Frame {
    /// Returns the raw frame value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Headless planet runtime manager: owns the scene and advances one frame
/// per tick.
#[derive(Debug, Default)]
pub struct Runtime {
    scene: Scene,
    frame: Frame,
}

impl Runtime {
    /// Creates a runtime owning the given scene at frame zero.
    #[must_use]
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            frame: Frame(0),
        }
    }

    /// Advances the runtime by one frame, saturating instead of overflowing.
    pub fn tick(&mut self) {
        self.frame = Frame(self.frame.0.saturating_add(1));
    }

    /// Returns the current frame.
    #[must_use]
    pub const fn frame(&self) -> Frame {
        self.frame
    }

    /// Returns the owned scene.
    #[must_use]
    pub const fn scene(&self) -> &Scene {
        &self.scene
    }
}
