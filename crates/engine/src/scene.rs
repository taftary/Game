//! Renderer-neutral scene data: the set of live nodes.
//!
//! The scene owns domain data only. GPU resources derived from it live in
//! [`crate::render`].

use crate::node::NodeId;

/// Renderer-neutral scene: an unordered set of live node identities.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    nodes: Vec<NodeId>,
}

impl Scene {
    /// Creates an empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Inserts a node identity. Duplicate inserts are ignored.
    pub fn insert(&mut self, id: NodeId) {
        if !self.nodes.contains(&id) {
            self.nodes.push(id);
        }
    }

    /// Returns the number of live nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true when there are no live nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns true when the identity is part of the scene.
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains(&id)
    }
}
