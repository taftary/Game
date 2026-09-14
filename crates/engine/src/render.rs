//! Renderer-neutral draw data derived from scene content.
//!
//! These types describe geometry for any backend. Vulkan objects live behind
//! the viewer integration (Planned) and never appear here.

/// A single vertex position in model space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Model-space position.
    pub position: [f32; 3],
}

impl Vertex {
    /// Creates a vertex at the given model-space position.
    #[must_use]
    pub const fn new(position: [f32; 3]) -> Self {
        Self { position }
    }
}

/// Backend-independent triangle geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    vertices: Vec<Vertex>,
}

impl MeshData {
    /// Creates mesh data from vertices.
    #[must_use]
    pub fn new(vertices: Vec<Vertex>) -> Self {
        Self { vertices }
    }

    /// Returns the vertex count.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns true when there are no vertices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Returns the vertex slice.
    #[must_use]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }
}
