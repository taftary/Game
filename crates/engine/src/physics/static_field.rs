//! Cosmic web / supercluster row (spec §5): static by design.
//!
//! Nothing moves in real time at these scales — the model is a static
//! procedural density field with no live gravity. Structure itself
//! regenerates from [`crate::physics`] seeds via `hierarchical-seeding`
//! (next feature); this type exists so the spec §5 table has one named
//! implementation per row, and so future code can branch on staticness
//! instead of re-deriving it.
//!
//! ```
//! use game_engine::physics::StaticDensityField;
//!
//! assert!(StaticDensityField::is_static());
//! ```

/// Marker for the static cosmic density field. No live gravity, no
/// integration — enforced by having no dynamics API at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticDensityField;

impl StaticDensityField {
    /// Always true: documents the static contract in code.
    pub fn is_static() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmic_scales_have_no_live_gravity() {
        assert!(StaticDensityField::is_static());
    }
}
