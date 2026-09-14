//! Headless visibility culling: per-pass outcomes without camera policy.
//!
//! Camera selection policy is Planned. This module owns the outcome types so
//! the culling pass boundary is testable headlessly from the start.

/// Per-node visibility outcome for one culling pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// The node survived culling and needs draw data.
    Visible,
    /// The node was culled this pass.
    Culled,
}

/// Outcome counts of one headless culling pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CullStats {
    checked: u32,
    visible: u32,
}

impl CullStats {
    /// Records one pass outcome. `visible` saturates at `checked`.
    #[must_use]
    pub const fn new(checked: u32, visible: u32) -> Self {
        let visible = if visible > checked { checked } else { visible };
        Self { checked, visible }
    }

    /// Returns the number of checked nodes.
    #[must_use]
    pub const fn checked(self) -> u32 {
        self.checked
    }

    /// Returns the number of visible nodes.
    #[must_use]
    pub const fn visible(self) -> u32 {
        self.visible
    }

    /// Fraction of checked nodes that stayed visible, or [`None`] when
    /// nothing was checked.
    #[must_use]
    pub fn visible_ratio(self) -> Option<f32> {
        if self.checked == 0 {
            return None;
        }
        Some(self.visible as f32 / self.checked as f32)
    }
}
