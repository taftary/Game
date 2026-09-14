//! `planet_crafter_engine`: reusable geometry, scene data, text,
//! the headless planet runtime, LOD scheduling, visibility culling, and
//! (Planned) the Vulkan viewer.
//!
//! The engine exposes capabilities and data contracts, never backend handles.
//! Game content and tools depend on this crate; it depends on neither.
//!
//! ```rust
//! use planet_crafter_engine::{node::NodeId, runtime::Runtime, scene::Scene};
//!
//! let mut scene = Scene::new();
//! scene.insert(NodeId::new(0));
//! let mut runtime = Runtime::new(scene);
//! runtime.tick();
//! assert_eq!(runtime.frame().get(), 1);
//! assert_eq!(runtime.scene().node_count(), 1);
//! ```

pub mod lod;
pub mod node;
pub mod render;
pub mod runtime;
pub mod scene;
pub mod text;
pub mod visibility;

/// Test-only whitebox surface behind the `test-internals` feature.
///
/// This is a test-only surface, not a public API contract: builds without the
/// feature expose nothing extra.
#[cfg(feature = "test-internals")]
pub mod testing {
    use crate::node::NodeId;

    /// Deterministic node identity for tests.
    #[must_use]
    pub const fn deterministic_id(value: u64) -> NodeId {
        NodeId::new(value)
    }
}
