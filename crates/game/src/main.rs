//! PlanetCrafter game binary: application entry point and content policy.
//!
//! Target: windowed Vulkan viewer. Current: headless runtime summary over a
//! single-node scene.

use planet_crafter_engine::{node::NodeId, runtime::Runtime, scene::Scene};

fn main() {
    let mut scene = Scene::new();
    scene.insert(NodeId::new(0));

    let mut runtime = Runtime::new(scene);
    runtime.tick();

    println!(
        "PlanetCrafter: frame {} with {} live node(s).",
        runtime.frame().get(),
        runtime.scene().node_count()
    );
}
