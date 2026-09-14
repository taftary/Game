//! Headless LOD scheduler: detail-level stepping without selection policy.
//!
//! Distance-to-level policy is Planned. This module owns the level type so
//! call sites are ready when the policy lands.

/// Detail level of a node, zero being the coarsest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LodLevel(u8);

impl LodLevel {
    /// Coarsest detail level.
    pub const MIN: Self = Self(0);

    /// Finest representable detail level.
    pub const MAX: Self = Self(u8::MAX);

    /// Creates a detail level from its raw value.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns one level finer, or [`None`] at [`LodLevel::MAX`].
    #[must_use]
    pub const fn refined(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns one level coarser, or [`None`] at [`LodLevel::MIN`].
    #[must_use]
    pub const fn coarsened(self) -> Option<Self> {
        match self.0.checked_sub(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw level value.
    ///
    /// Test-only whitebox accessor behind the `test-internals` feature.
    #[cfg(feature = "test-internals")]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}
