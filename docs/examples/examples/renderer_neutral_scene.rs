//! Nodes become GPU-independent vertex data without opening a window.
//!
//! Demonstrates the renderer-neutral boundary in
//! [crate boundaries](../../book/architecture/crate-boundaries.md): scene
//! content turns into draw data with no backend types involved.

use planet_crafter_engine::{
    render::{MeshData, Vertex},
    scene::Scene,
};

fn main() {
    let scene = Scene::new();
    assert!(scene.is_empty());

    let mesh = MeshData::new(vec![
        Vertex::new([0.0, 0.0, 0.0]),
        Vertex::new([1.0, 0.0, 0.0]),
        Vertex::new([0.0, 1.0, 0.0]),
    ]);
    assert_eq!(mesh.vertex_count(), 3);
    println!("renderer_neutral_scene: triangle expressed without a backend");
}
