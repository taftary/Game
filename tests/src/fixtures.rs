//! Fixtures shared across integration-test targets.

use planet_crafter_engine::{
    node::NodeId,
    runtime::Runtime,
    scene::Scene,
    text::{FontSize, TextRun},
};

/// Deterministic root identity shared by tests.
#[must_use]
pub const fn root_id() -> NodeId {
    NodeId::new(0)
}

/// Scene with two live nodes.
#[must_use]
pub fn sample_scene() -> Scene {
    let mut scene = Scene::new();
    scene.insert(root_id());
    scene.insert(NodeId::new(1));
    scene
}

/// Valid text run shared by tests.
#[must_use]
pub fn sample_run() -> TextRun {
    TextRun::new("hello", FontSize::new(16).expect("16 is a valid size"))
        .expect("sample content is not empty")
}

/// Runtime over the sample scene, advanced by two ticks.
#[must_use]
pub fn ticked_runtime() -> Runtime {
    let mut runtime = Runtime::new(sample_scene());
    runtime.tick();
    runtime.tick();
    runtime
}
