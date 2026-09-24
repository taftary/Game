//! Cosmic quality-tier selection (`cosmic-device-tier`, ADR-027).
//!
//! Window- and GPU-free: pure mapping from device type (a plain
//! enum — no GPU handle) or the `GAME_DEBUG_TIER` override to the
//! three engine-approved tier contracts (`SplatK::for_tier`,
//! `VeilMode::for_tier_*`, `MipBloomParams::for_tier`). The binary
//! owns device access and env reads; this module owns the vocabulary.

use crate::cosmic_splat::{SplatK, SplatTier};
use crate::cosmic_veil::VeilMode;
use game_engine::render::tier::QualityTier;
use vulkano::device::physical::PhysicalDeviceType;

/// Active cosmic tier + where it came from (the FPS widget shows
/// e.g. `tier: medium (auto)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CosmicTier {
    pub tier: QualityTier,
    pub source: TierSource,
}

/// Where the active tier came from: device mapping, env override, or
/// the `F4` session cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TierSource {
    Auto,
    Env,
    Manual,
}

impl TierSource {
    pub const fn name(self) -> &'static str {
        match self {
            TierSource::Auto => "auto",
            TierSource::Env => "env",
            TierSource::Manual => "manual",
        }
    }
}

impl CosmicTier {
    /// Boot default before the device is known: High, matching the
    /// pre-tier shell (headless/tests never draw).
    pub const fn boot_default() -> Self {
        CosmicTier {
            tier: QualityTier::High,
            source: TierSource::Auto,
        }
    }
}

impl Default for CosmicTier {
    fn default() -> Self {
        CosmicTier::boot_default()
    }
}

/// `GAME_DEBUG_TIER` parsing follows the engine
/// [`QualityTier::from_str`] contract (`low|medium|med|high`,
/// ASCII-case-insensitive; garbage errors with a clear message and
/// never silently regrades a capture — the `GAME_DEBUG_COSMIC_VEIL`
/// precedent). Tier display names come from [`QualityTier::name`];
/// there is deliberately no second vocabulary here.
///
/// Boot mapping from device type (ADR-027): `Cpu`/`VirtualGpu` run
/// Low anywhere, `IntegratedGpu` runs Medium, `DiscreteGpu` runs
/// High, anything else (future types via the non-exhaustive
/// wildcard) runs Medium — never crash on an unknown device.
pub fn auto_tier(device_type: PhysicalDeviceType) -> QualityTier {
    match device_type {
        PhysicalDeviceType::Cpu | PhysicalDeviceType::VirtualGpu => QualityTier::Low,
        PhysicalDeviceType::IntegratedGpu => QualityTier::Medium,
        PhysicalDeviceType::DiscreteGpu => QualityTier::High,
        _ => QualityTier::Medium,
    }
}

/// `QualityTier` → the splat tier contract (`SplatK::for_tier`:
/// 1/2/8).
pub const fn splat_tier_for(tier: QualityTier) -> SplatTier {
    match tier {
        QualityTier::Low => SplatTier::Low,
        QualityTier::Medium => SplatTier::Medium,
        QualityTier::High => SplatTier::High,
    }
}

/// Sub-samples per cell for a tier (the `SplatK` contract).
pub fn splat_k_for(tier: QualityTier) -> u8 {
    SplatK::for_tier(splat_tier_for(tier)).0
}

/// Veil body for a tier (the `VeilMode::for_tier_*` contract).
pub fn veil_for(tier: QualityTier) -> VeilMode {
    match tier {
        QualityTier::Low => VeilMode::for_tier_low(),
        QualityTier::Medium => VeilMode::for_tier_medium(),
        QualityTier::High => VeilMode::for_tier_high(),
    }
}

/// Cycle order for the `F4` handler over [`QualityTier::all`]
/// (Low → Medium → High → Low).
pub fn next_tier(tier: QualityTier) -> QualityTier {
    let all = QualityTier::all();
    let i = all.iter().position(|&t| t == tier).unwrap_or(2);
    all[(i + 1) % all.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_mapping_matches_adr() {
        assert_eq!(auto_tier(PhysicalDeviceType::Cpu), QualityTier::Low);
        assert_eq!(auto_tier(PhysicalDeviceType::VirtualGpu), QualityTier::Low);
        assert_eq!(
            auto_tier(PhysicalDeviceType::IntegratedGpu),
            QualityTier::Medium
        );
        assert_eq!(
            auto_tier(PhysicalDeviceType::DiscreteGpu),
            QualityTier::High
        );
        assert_eq!(auto_tier(PhysicalDeviceType::Other), QualityTier::Medium);
    }

    #[test]
    fn knob_contracts_hold_per_tier() {
        assert_eq!(splat_k_for(QualityTier::Low), 1);
        assert_eq!(splat_k_for(QualityTier::Medium), 2);
        assert_eq!(splat_k_for(QualityTier::High), 8);
        assert_eq!(veil_for(QualityTier::Low), VeilMode::Sprites);
        assert_eq!(veil_for(QualityTier::Medium), VeilMode::March { steps: 32 });
        assert_eq!(veil_for(QualityTier::High), VeilMode::March { steps: 48 });
    }

    #[test]
    fn cycle_wraps_around() {
        assert_eq!(next_tier(QualityTier::Low), QualityTier::Medium);
        assert_eq!(next_tier(QualityTier::Medium), QualityTier::High);
        assert_eq!(next_tier(QualityTier::High), QualityTier::Low);
        // The cycle walks `QualityTier::all` in order.
        let mut tier = QualityTier::Low;
        for _ in 0..3 {
            tier = next_tier(tier);
        }
        assert_eq!(tier, QualityTier::Low);
    }
}
