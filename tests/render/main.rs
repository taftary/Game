//! Renderer-neutral geometry contracts.

use planet_crafter_engine::render::{MeshData, Vertex};

#[test]
fn new_mesh_is_empty() {
    let mesh = MeshData::default();
    assert!(mesh.is_empty());
    assert_eq!(mesh.vertex_count(), 0);
}

#[test]
fn mesh_retains_vertices() {
    let vertices = vec![Vertex::new([0.0, 0.0, 0.0]), Vertex::new([1.0, 0.0, 0.0])];
    let mesh = MeshData::new(vertices.clone());
    assert_eq!(mesh.vertex_count(), 2);
    assert_eq!(mesh.vertices(), vertices.as_slice());
}
