//! Physical depth cues for the scale regimes in spec section 9.1.
//!
//! This module is deliberately pure and headless-testable. It maps the
//! existing reference-frame chain to cue regimes, derives deterministic dust
//! and cosmic-web samples through ADR-019 domain-separated streams, and
//! exposes source terms in linear-radiance-friendly form. Renderer binaries
//! own any GPU resource or pass that consumes these values.

use crate::frames::FrameId;
use crate::render::tier::QualityTier;
use crate::seeding::{RegionId, region_seed};

/// Physical depth-cue regime from spec section 9.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CueRegime {
    /// Waypoint 6: interplanetary dust glow.
    Interplanetary,
    /// Waypoints 5-4: interstellar dust extinction and reddening.
    Interstellar,
    /// Waypoints 4-3: local dust plus peculiar velocity.
    LocalIntergalactic,
    /// Waypoints 2-1: Hubble flow and seeded cosmic-web structure.
    CosmicWeb,
    /// Waypoints 7-10: atmospheric Rayleigh and Mie scattering.
    Atmosphere,
}

/// Maps the currently active frame to its primary cue regime.
///
/// A frame is the stable engine boundary today; waypoint interpolation is
/// intentionally left to `waypoint-transitions`.
pub fn regime_for_frame(frame: FrameId) -> CueRegime {
    match frame {
        FrameId::Cosmological => CueRegime::CosmicWeb,
        FrameId::Galactocentric | FrameId::StellarNeighborhood => CueRegime::Interstellar,
        FrameId::LocalGroup => CueRegime::LocalIntergalactic,
        FrameId::SolarSystem => CueRegime::Interplanetary,
        FrameId::Planetocentric(_) | FrameId::LocalEnu(_) => CueRegime::Atmosphere,
    }
}

/// Stateful frame selection with explicit replacement semantics.
///
/// The state contains only the active regime. Replacing a frame clears the
/// previous regime so a transition cannot retain a vacuum or atmosphere term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueState {
    frame: FrameId,
    regime: CueRegime,
}

impl CueState {
    /// Creates state for `frame`.
    pub fn new(frame: FrameId) -> Self {
        Self {
            frame,
            regime: regime_for_frame(frame),
        }
    }

    /// Returns the active frame.
    pub fn frame(self) -> FrameId {
        self.frame
    }

    /// Returns the active cue regime.
    pub fn regime(self) -> CueRegime {
        self.regime
    }

    /// Replaces the active frame and returns whether the regime changed.
    pub fn set_frame(&mut self, frame: FrameId) -> bool {
        let next = regime_for_frame(frame);
        let changed = next != self.regime;
        self.frame = frame;
        self.regime = next;
        changed
    }
}

/// Seeded dust-column sample used by the extinction and reddening models.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DustColumn {
    /// Normalized column density in [0, 1].
    pub density: f64,
}

/// Samples a deterministic dust column for a region and local position.
///
/// The only random stream is the ADR-019 `dust_column` layer. Position adds
/// a smooth, bounded modulation without creating a second unscoped seed.
pub fn sample_dust_column(master_seed: u64, region: RegionId, position: [f64; 3]) -> DustColumn {
    let mut rng = region_seed(master_seed, region).layer("dust_column");
    let base = rng.unit_f64();
    let phase = position
        .iter()
        .enumerate()
        .map(|(i, value)| clean_signed(*value) * (i as f64 + 1.0) * 0.37)
        .sum::<f64>();
    let modulation = 0.85 + 0.15 * phase.sin();
    DustColumn {
        density: (base * modulation).clamp(0.0, 1.0),
    }
}

/// Tunable dust extinction parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtinctionParams {
    /// Color excess per normalized dust-density unit.
    pub ebv_per_density: f64,
    /// Ratio `A_v / E(B-V)`.
    pub r_v: f64,
    /// Reference wavelength for the visual band, in nanometers.
    pub visual_wavelength_nm: f64,
}

impl ExtinctionParams {
    /// Physically conventional initial calibration.
    pub const fn spec_defaults() -> Self {
        Self {
            ebv_per_density: 1.0,
            r_v: 3.1,
            visual_wavelength_nm: 550.0,
        }
    }

    /// Validated constructor.
    pub fn new(ebv_per_density: f64, r_v: f64, visual_wavelength_nm: f64) -> Option<Self> {
        if ebv_per_density.is_finite()
            && ebv_per_density >= 0.0
            && r_v.is_finite()
            && r_v > 0.0
            && visual_wavelength_nm.is_finite()
            && visual_wavelength_nm > 0.0
        {
            Some(Self {
                ebv_per_density,
                r_v,
                visual_wavelength_nm,
            })
        } else {
            None
        }
    }
}

/// Converts normalized dust density to `E(B-V)`.
pub fn color_excess(density: f64, params: ExtinctionParams) -> f64 {
    clean_nonnegative(density) * clean_nonnegative(params.ebv_per_density)
}

/// Converts `E(B-V)` to visual extinction `A_v`.
pub fn visual_extinction(ebv: f64, params: ExtinctionParams) -> f64 {
    clean_nonnegative(ebv) * clean_positive(params.r_v)
}

/// Returns an extinction multiplier for a wavelength in nanometers.
///
/// The simple power law is a calibrated source-term approximation, not a
/// replacement for a future dust map. It preserves the required ordering:
/// shorter wavelengths attenuate more strongly than longer wavelengths.
pub fn extinction_transmission(wavelength_nm: f64, av: f64, params: ExtinctionParams) -> f64 {
    if !wavelength_nm.is_finite() || wavelength_nm <= 0.0 {
        return 0.0;
    }
    let visual = clean_positive(params.visual_wavelength_nm);
    if visual == 0.0 {
        return 0.0;
    }
    let relative = (wavelength_nm / visual).powf(-1.2);
    let transmission = 10.0_f64.powf(-0.4 * clean_nonnegative(av) * relative);
    if transmission.is_finite() {
        transmission.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Applies dust attenuation to an RGB triplet ordered as R, G, B.
pub fn redden_rgb(rgb: [f64; 3], av: f64, params: ExtinctionParams) -> [f64; 3] {
    [
        rgb[0] * extinction_transmission(650.0, av, params),
        rgb[1] * extinction_transmission(550.0, av, params),
        rgb[2] * extinction_transmission(450.0, av, params),
    ]
}

/// Doppler color shift from radial peculiar velocity in meters per second.
///
/// Positive velocity is recession and shifts the tint redward; negative
/// velocity shifts it blueward. The result is a bounded RGB multiplier.
pub fn peculiar_velocity_tint(radial_velocity_mps: f64) -> [f64; 3] {
    let beta = (clean_signed(radial_velocity_mps) / 299_792_458.0).clamp(-0.25, 0.25);
    [1.0 + 0.35 * beta, 1.0, 1.0 - 0.35 * beta]
}

/// Hubble-flow redshift, enabled only after the caller establishes cosmic-web
/// dominance. Distance is in meters and Hubble constant is in SI units.
pub fn hubble_redshift(distance_m: f64, hubble_constant_s: f64) -> f64 {
    if distance_m.is_finite() && distance_m >= 0.0 && hubble_constant_s.is_finite() {
        (hubble_constant_s.max(0.0) * distance_m / 299_792_458.0).max(0.0)
    } else {
        0.0
    }
}

/// Rayleigh optical-depth approximation for a normalized atmosphere.
pub fn rayleigh_optical_depth(air_mass: f64, wavelength_nm: f64) -> f64 {
    if !air_mass.is_finite() || air_mass < 0.0 || !wavelength_nm.is_finite() || wavelength_nm <= 0.0
    {
        return 0.0;
    }
    air_mass * (550.0 / wavelength_nm).powi(4)
}

/// Henyey-Greenstein Mie phase approximation.
pub fn mie_phase(cos_angle: f64, anisotropy: f64) -> f64 {
    let g = anisotropy.clamp(-0.99, 0.99);
    let mu = cos_angle.clamp(-1.0, 1.0);
    let denominator = (1.0 + g * g - 2.0 * g * mu).powf(1.5);
    (1.0 - g * g) / (4.0 * std::f64::consts::PI * denominator.max(f64::MIN_POSITIVE))
}

/// Mobile-safe atmosphere source term using reduced angular complexity.
pub fn atmosphere_radiance(air_mass: f64, cos_angle: f64, wavelength_nm: f64, mobile: bool) -> f64 {
    let rayleigh = (-rayleigh_optical_depth(air_mass, wavelength_nm)).exp();
    let mie = mie_phase(cos_angle, if mobile { 0.55 } else { 0.76 });
    (rayleigh * mie).max(0.0)
}

/// Deterministic cosmic-web sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WebDensity {
    /// Filament density in [0, 1].
    pub filament: f64,
    /// Dust-node density in [0, 1].
    pub dust_node: f64,
}

/// Samples independent seeded cosmic-web density layers.
pub fn sample_web_density(master_seed: u64, region: RegionId, position: [f64; 3]) -> WebDensity {
    let seed = region_seed(master_seed, region);
    let mut filaments = seed.layer("cosmic_filaments");
    let mut nodes = seed.layer("cosmic_dust_nodes");
    let phase = position[0] * 0.17 + position[1] * 0.31 + position[2] * 0.47;
    let filament = (0.65 * filaments.unit_f64() + 0.35 * (phase.sin() * 0.5 + 0.5)).clamp(0.0, 1.0);
    let dust_node = (0.65 * nodes.unit_f64() + 0.35 * (phase.cos() * 0.5 + 0.5)).clamp(0.0, 1.0);
    WebDensity {
        filament,
        dust_node,
    }
}

/// Integrates a seeded web sample using the selected tier budget.
///
/// This is the CPU reference contract for the eventual renderer pass. It
/// keeps the source term deterministic and bounded while the GPU binary owns
/// texture, attachment, and dispatch details.
pub fn raymarch_web(density: WebDensity, budget: RaymarchBudget) -> f64 {
    if budget.steps == 0 {
        return 0.0;
    }
    let filament_density = clean_nonnegative(density.filament).min(1.0);
    let node_density = clean_nonnegative(density.dust_node).min(1.0);
    let mut accumulated = 0.0;
    let mut transmittance = 1.0;
    for index in 0..budget.steps {
        let t = (index as f64 + 0.5) / budget.steps as f64;
        let filament = filament_density * (1.0 - 0.45 * t);
        let node = node_density * (0.35 + 0.65 * t);
        let source = (filament + node).max(0.0);
        let opacity = (0.04 * source).clamp(0.0, 0.25);
        accumulated += transmittance * source * opacity;
        transmittance *= 1.0 - opacity;
    }
    accumulated.clamp(0.0, 1.0)
}

/// Tiered raymarch budget for the cosmic-web source term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaymarchBudget {
    /// Internal square sample resolution.
    pub resolution: u16,
    /// Maximum samples per ray.
    pub steps: u16,
}

impl RaymarchBudget {
    /// Conservative budget that preserves the Low-tier frame requirement.
    pub const fn for_tier(tier: QualityTier) -> Self {
        match tier {
            QualityTier::Low => Self {
                resolution: 64,
                steps: 16,
            },
            QualityTier::Medium => Self {
                resolution: 128,
                steps: 32,
            },
            QualityTier::High => Self {
                resolution: 256,
                steps: 64,
            },
        }
    }
}

fn clean_nonnegative(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn clean_signed(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn clean_positive(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::BodyId;

    fn region(frame: FrameId) -> RegionId {
        RegionId {
            frame,
            cell: [2, -1, 3],
        }
    }

    #[test]
    fn frame_chain_maps_to_physical_regimes() {
        assert_eq!(
            regime_for_frame(FrameId::Cosmological),
            CueRegime::CosmicWeb
        );
        assert_eq!(
            regime_for_frame(FrameId::Galactocentric),
            CueRegime::Interstellar
        );
        assert_eq!(
            regime_for_frame(FrameId::LocalGroup),
            CueRegime::LocalIntergalactic
        );
        assert_eq!(
            regime_for_frame(FrameId::StellarNeighborhood),
            CueRegime::Interstellar
        );
        assert_eq!(
            regime_for_frame(FrameId::SolarSystem),
            CueRegime::Interplanetary
        );
        assert_eq!(
            regime_for_frame(FrameId::Planetocentric(BodyId::EARTH)),
            CueRegime::Atmosphere
        );
        assert_eq!(
            regime_for_frame(FrameId::LocalEnu(BodyId::EARTH)),
            CueRegime::Atmosphere
        );
    }

    #[test]
    fn frame_switch_replaces_regime_without_leftover_state() {
        let mut state = CueState::new(FrameId::SolarSystem);
        assert_eq!(state.regime(), CueRegime::Interplanetary);
        assert!(state.set_frame(FrameId::LocalGroup));
        assert_eq!(state.regime(), CueRegime::LocalIntergalactic);
        assert!(state.set_frame(FrameId::Galactocentric));
        assert_eq!(state.frame(), FrameId::Galactocentric);
        assert_eq!(state.regime(), CueRegime::Interstellar);
        assert!(!state.set_frame(FrameId::Galactocentric));
    }

    #[test]
    fn dust_and_web_samples_are_deterministic_and_domain_separated() {
        let a = sample_dust_column(42, region(FrameId::StellarNeighborhood), [1.0, 2.0, 3.0]);
        let b = sample_dust_column(42, region(FrameId::StellarNeighborhood), [1.0, 2.0, 3.0]);
        assert_eq!(a, b);

        let web_a = sample_web_density(42, region(FrameId::Cosmological), [1.0, 2.0, 3.0]);
        let web_b = sample_web_density(42, region(FrameId::Cosmological), [1.0, 2.0, 3.0]);
        assert_eq!(web_a, web_b);
        assert_ne!(
            sample_web_density(42, region(FrameId::Cosmological), [1.0, 2.0, 3.0]).filament,
            sample_dust_column(42, region(FrameId::Cosmological), [1.0, 2.0, 3.0]).density
        );
    }

    #[test]
    fn extinction_is_monotonic_and_reddens_blue_light_more() {
        let params = ExtinctionParams::spec_defaults();
        let low = visual_extinction(color_excess(0.1, params), params);
        let high = visual_extinction(color_excess(0.8, params), params);
        assert!(high > low);
        assert!(
            extinction_transmission(450.0, high, params)
                < extinction_transmission(650.0, high, params)
        );
        let rgb = redden_rgb([1.0, 1.0, 1.0], high, params);
        assert!(rgb[0] > rgb[2]);
    }

    #[test]
    fn peculiar_velocity_handles_local_blueshift_without_hubble_tint() {
        let tint = peculiar_velocity_tint(-30_000.0);
        assert!(tint[2] > tint[0]);
        assert_eq!(hubble_redshift(-1.0, 1.0), 0.0);
        assert!(hubble_redshift(1.0e24, 2.2e-18) > 0.0);
    }

    #[test]
    fn atmosphere_is_finite_and_mobile_fallback_is_bounded() {
        let full = atmosphere_radiance(2.0, 0.8, 550.0, false);
        let mobile = atmosphere_radiance(2.0, 0.8, 550.0, true);
        assert!(full.is_finite() && full >= 0.0);
        assert!(mobile.is_finite() && mobile >= 0.0);
        assert!(rayleigh_optical_depth(2.0, 450.0) > rayleigh_optical_depth(2.0, 650.0));
    }

    #[test]
    fn tier_budgets_are_monotonic() {
        let low = RaymarchBudget::for_tier(QualityTier::Low);
        let medium = RaymarchBudget::for_tier(QualityTier::Medium);
        let high = RaymarchBudget::for_tier(QualityTier::High);
        assert!(low.resolution < medium.resolution && medium.resolution < high.resolution);
        assert!(low.steps < medium.steps && medium.steps < high.steps);
    }

    #[test]
    fn raymarch_reference_is_bounded_and_budget_sensitive() {
        let density = WebDensity {
            filament: 0.8,
            dust_node: 0.4,
        };
        let low = raymarch_web(density, RaymarchBudget::for_tier(QualityTier::Low));
        let high = raymarch_web(density, RaymarchBudget::for_tier(QualityTier::High));
        assert!(low.is_finite() && (0.0..=1.0).contains(&low));
        assert!(high.is_finite() && (0.0..=1.0).contains(&high));
        assert!(high >= low);
    }
}
