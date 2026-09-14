//! Geometric nodes: identity and subdivision generations.
//!
//! Nodes form the hierarchical triangle subdivision addressed by the
//! surrounding architecture. This module owns node identity; subdivision
//! behavior lands in later migration stages.

/// Opaque identity of a geometric node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// Creates a node identity from its raw value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw identity value.
    ///
    /// Test-only whitebox accessor behind the `test-internals` feature.
    #[cfg(feature = "test-internals")]
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Subdivision generation of a node. The root assembly is generation zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u32);

impl Generation {
    /// Generation of the undivided root assembly.
    pub const ROOT: Self = Self(0);

    /// Largest representable generation.
    pub const MAX: Self = Self(u32::MAX);

    /// Returns the subdivided generation, or [`None`] on overflow.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw generation value.
    ///
    /// Test-only whitebox accessor behind the `test-internals` feature.
    #[cfg(feature = "test-internals")]
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
