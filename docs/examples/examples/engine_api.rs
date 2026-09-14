//! Game code consumes engine contracts without backend details.
//!
//! Demonstrates [crate boundaries](../../book/architecture/crate-boundaries.md):
//! the game builds a scene and drives the runtime through engine APIs only.

use planet_crafter_engine::{node::NodeId, runtime::Runtime, scene::Scene};

fn main() {
    let mut scene = Scene::new();
    scene.insert(NodeId::new(0));
    scene.insert(NodeId::new(1));

    let mut runtime = Runtime::new(scene);
    runtime.tick();

    assert_eq!(runtime.scene().node_count(), 2);
    println!("engine_api: frame {} over two nodes", runtime.frame().get());
}
