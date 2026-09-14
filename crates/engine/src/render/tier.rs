//! Quality tiers (Low / Medium / High).
//!
//! Every feature must run on Low to ship; High-only effects are never
//! load-bearing (see `docs/techstack/rendering.md`). The per-tier planet
//! subdivision level is the M1 consumer of the `HexSphere` mesh
//! (ADR-002 deferred the LOD choice here).

use std::fmt;
use std::str::FromStr;

/// Quality tier: the single knob M1+ features tier-gate against.
///
/// Tri counts below assume the dual-mesh fan triangulation
/// (`crate::render::planet`): every hexagon emits 6 triangles, every
/// pentagon 5, so `tris = 6 * (cells - 12) + 60`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QualityTier {
    /// Phones: 30 fps sustained, ≤1080p dynamic down to 0.6x.
    Low,
    /// Tablets / Deck-class / low desktop: 30–60 fps, 1080p–1440p dynamic.
    Medium,
    /// Desktop: 60 fps+, native.
    High,
}

/// Error for [`QualityTier::from_str`] on unknown input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseTierError(String);

impl fmt::Display for ParseTierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown quality tier {:?} (expected low, medium, or high)",
            self.0
        )
    }
}

impl std::error::Error for ParseTierError {}

impl FromStr for QualityTier {
    type Err = ParseTierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(ParseTierError(other.to_owned())),
        }
    }
}

impl QualityTier {
    /// All tiers, low to high.
    pub fn all() -> [Self; 3] {
        [Self::Low, Self::Medium, Self::High]
    }

    /// Short CLI/config name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// `HexSphere` subdivision level for the far-field planet mesh.
    ///
    /// - Low 3 → 642 cells, 3,840 tris (aggressive LOD, short view).
    /// - Medium 4 → 2,562 cells, 15,360 tris.
    /// - High 6 → 40,962 cells, 245,760 tris (inside the Low <500k
    ///   budget on its own; terrain + colony draw calls stack on top).
    pub fn subdivisions(self) -> u32 {
        match self {
            Self::Low => 3,
            Self::Medium => 4,
            Self::High => 6,
        }
    }

    /// Dynamic-resolution scale applied to the swapchain extent.
    pub fn resolution_scale(self) -> f32 {
        match self {
            Self::Low => 0.6,
            Self::Medium => 0.85,
            Self::High => 1.0,
        }
    }

    /// Whether shadow maps are enabled. Flag only in M1 — no shadow
    /// implementation exists yet; High-only effects stay non-load-bearing.
    pub fn shadows(self) -> bool {
        match self {
            Self::High => true,
            Self::Low | Self::Medium => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_names_case_insensitively() {
        assert_eq!("low".parse(), Ok(QualityTier::Low));
        assert_eq!("LOW".parse(), Ok(QualityTier::Low));
        assert_eq!("medium".parse(), Ok(QualityTier::Medium));
        assert_eq!("med".parse(), Ok(QualityTier::Medium));
        assert_eq!("High".parse(), Ok(QualityTier::High));
        assert!("ultra".parse::<QualityTier>().is_err());
    }

    #[test]
    fn tier_table_matches_documented_budgets() {
        // (tier, subdivisions, cells, tris, scale, shadows)
        let expected = [
            (QualityTier::Low, 3, 642, 3_840, 0.6, false),
            (QualityTier::Medium, 4, 2_562, 15_360, 0.85, false),
            (QualityTier::High, 6, 40_962, 245_760, 1.0, true),
        ];
        for (tier, subdiv, cells, tris, scale, shadows) in expected {
            assert_eq!(tier.subdivisions(), subdiv, "{tier:?}");
            assert_eq!(tier.resolution_scale(), scale, "{tier:?}");
            assert_eq!(tier.shadows(), shadows, "{tier:?}");
            // cells = 10·4^N + 2; tris = 6·(cells − 12) + 60.
            let check_cells = 10 * 4usize.pow(subdiv) + 2;
            assert_eq!(check_cells, cells);
            assert_eq!(6 * (cells - 12) + 60, tris);
            assert!(tris < 500_000, "Low-tier tri budget headroom");
        }
    }

    #[test]
    fn all_lists_every_tier_once() {
        assert_eq!(
            QualityTier::all(),
            [QualityTier::Low, QualityTier::Medium, QualityTier::High]
        );
    }
}
