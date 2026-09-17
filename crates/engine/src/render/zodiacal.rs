//! Analytic zodiacal-light source term (`plans/v0.2.0/zodiacal-light`,
//! ADR-021, spec §9.4).
//!
//! Interplanetary space has no atmosphere, so the real depth cue at
//! solar-system scale is the faint glow of sunlight scattered off
//! interplanetary dust — evaluated per pixel from the camera ray in
//! ecliptic coordinates, converted to linear radiance *before* the
//! exposure/tone-mapping stage (`exposure-tone-mapping` consumes the
//! [`ZodiacalSource`] interface, never this struct directly, so a future
//! map-based HEALPix asset swaps in without interface churn).
//!
//! Spec §9.4 magnitude-offset form (β = ecliptic latitude, θ = solar
//! elongation):
//!
//! ```text
//! μ(β, θ) = μ₀ − Aβ·exp(−|β|/β₀) − Aθ·(1 / (1 + k·(1 − cosθ)))
//! ```
//!
//! Spec initial parameters: μ₀ = 23.0 mag/arcsec², Aβ = 1.5 mag,
//! β₀ = 20°, Aθ = 0.5 mag, k = 0.7. Reference falloff (pinned by tests):
//!
//! | direction | μ (mag/arcsec²) |
//! |---|---:|
//! | ecliptic pole (β=90°, θ=90°) | ≈ 22.69 |
//! | ecliptic quadrature (β=0°, θ=90°) | ≈ 21.21 |
//! | antisolar pole (β=90°, θ=180°) | ≈ 22.78 |
//! | near-Sun ecliptic (β=0°, θ→0°) | → 21.0 (saturates — see limit) |
//!
//! Known limit: the formula saturates toward the Sun instead of rising
//! into the F-corona; treat θ ≲ 15° as outside the valid range (handoff
//! constraint recorded for `exposure-tone-mapping`).
//!
//! Validation hook (DoD-2, full comparison before release per spec
//! §9.4): compare pole + elongation falloff against Leinert et al. 1998
//! diffuse night-sky reference values, converting S10 units via
//! `mag/arcsec² = 27.78 − 2.5·log10(S10)` (COBE/DIRBE and HST background
//! models are the alternates the spec names).
//!
//! Everything here is pure math over caller-supplied directions: the
//! contract starts at equatorial J2000 unit vectors, and engine
//! world ↔ equatorial mapping stays with `star-catalog-streaming`.

use glam::DVec3;

/// J2000 mean-ecliptic obliquity: the ecliptic pole tilt from the
/// equatorial pole, in degrees.
pub const ECLIPTIC_OBLIQUITY_DEG: f64 = 23.43928;

/// Brightest plausible V-band surface brightness (mag/arcsec²): safety
/// rail for [`surface_brightness_mag`], wider than the model's own
/// [21.0, 22.78] range so it only catches bugs — guard, not physics.
pub const MU_BRIGHTEST: f64 = 20.0;

/// Faintest plausible V-band surface brightness (mag/arcsec²): matching
/// safety rail on the faint end.
pub const MU_FAINTEST: f64 = 24.0;

/// Calibration parameters of the analytic model. Fields are public so
/// validators can tune against photographic references;
/// [`spec_defaults`](Self::spec_defaults) pins the spec §9.4 initials.
///
/// ```
/// use game_engine::render::zodiacal::ZodiacalParams;
///
/// let params = ZodiacalParams::spec_defaults();
/// assert_eq!(params.mu0, 23.0);
/// assert!(ZodiacalParams::new(23.0, 1.5, 20.0, 0.5, 0.7).is_some());
/// assert!(ZodiacalParams::new(23.0, 1.5, 0.0, 0.5, 0.7).is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZodiacalParams {
    /// Base V-band surface brightness at the ecliptic poles
    /// (mag/arcsec²).
    pub mu0: f64,
    /// Latitude falloff amplitude (mag).
    pub a_beta: f64,
    /// Latitude falloff scale (degrees, > 0).
    pub beta0_deg: f64,
    /// Forward-scattering amplitude (mag).
    pub a_theta: f64,
    /// Forward-scattering strength (≥ 0).
    pub k: f64,
}

impl ZodiacalParams {
    /// Spec §9.4 initial parameters.
    pub fn spec_defaults() -> Self {
        Self {
            mu0: 23.0,
            a_beta: 1.5,
            beta0_deg: 20.0,
            a_theta: 0.5,
            k: 0.7,
        }
    }

    /// Validated constructor: `None` on non-finite input, non-positive
    /// `beta0_deg`, or negative `k` (a negative scattering strength
    /// would unphysically darken toward the Sun).
    pub fn new(mu0: f64, a_beta: f64, beta0_deg: f64, a_theta: f64, k: f64) -> Option<Self> {
        if [mu0, a_beta, beta0_deg, a_theta, k]
            .iter()
            .all(|v| v.is_finite())
            && beta0_deg > 0.0
            && k >= 0.0
        {
            Some(Self {
                mu0,
                a_beta,
                beta0_deg,
                a_theta,
                k,
            })
        } else {
            None
        }
    }
}

/// V-band surface brightness μ(β, θ) in mag/arcsec² for ecliptic
/// latitude `beta_rad` and solar elongation `theta_rad`.
///
/// Both brightening terms are bounded by construction (latitude term in
/// [0, Aβ], forward term in [Aθ/(1+2k), Aθ]); the result is additionally
/// clamped to [`MU_BRIGHTEST`]…[`MU_FAINTEST`] as a bug rail.
/// Non-finite inputs map to [`MU_FAINTEST`] (no glow, never NaN
/// downstream).
///
/// ```
/// use game_engine::render::zodiacal::{ZodiacalParams, surface_brightness_mag};
/// use std::f64::consts::FRAC_PI_2;
///
/// let params = ZodiacalParams::spec_defaults();
/// let pole = surface_brightness_mag(FRAC_PI_2, FRAC_PI_2, &params);
/// assert!((pole - 22.69).abs() < 0.05, "pole = {pole}");
/// let quadrature = surface_brightness_mag(0.0, FRAC_PI_2, &params);
/// assert!((quadrature - 21.21).abs() < 0.05, "quadrature = {quadrature}");
/// assert!(quadrature < pole, "ecliptic is brighter than the pole");
/// ```
pub fn surface_brightness_mag(beta_rad: f64, theta_rad: f64, params: &ZodiacalParams) -> f64 {
    if !beta_rad.is_finite() || !theta_rad.is_finite() {
        return MU_FAINTEST;
    }
    let beta0_rad = params.beta0_deg.to_radians();
    let latitude = params.a_beta * (-beta_rad.abs() / beta0_rad).exp();
    let forward = params.a_theta / (1.0 + params.k * (1.0 - theta_rad.cos()));
    (params.mu0 - latitude - forward).clamp(MU_BRIGHTEST, MU_FAINTEST)
}

/// Linear proportional radiance for magnitude surface brightness μ:
/// `10^(-0.4·μ)`.
///
/// This is the spec §9.4 conversion (magnitude → linear **before**
/// exposure/tone mapping) up to the absolute zero point, which is
/// photometric calibration owned by `exposure-tone-mapping`: only ratios
/// are meaningful here, and that is all a tone mapper needs.
///
/// ```
/// use game_engine::render::zodiacal::radiance_relative;
///
/// // A 5-magnitude step is exactly a factor of 100 in linear radiance.
/// assert!((radiance_relative(18.0) / radiance_relative(23.0) - 100.0).abs() < 1e-9);
/// ```
pub fn radiance_relative(mu_mag_per_arcsec2: f64) -> f64 {
    10.0f64.powf(-0.4 * mu_mag_per_arcsec2)
}

/// Ecliptic coordinates `(beta, lambda)` in radians for an equatorial
/// J2000 unit direction: latitude from the mean ecliptic, longitude
/// eastward from the vernal equinox.
///
/// The input must be (near-)unit length; it is normalized defensively.
/// A zero vector yields `(0, 0)`.
///
/// ```
/// use game_engine::render::zodiacal::ecliptic_coords;
/// use glam::DVec3;
///
/// // J2000 ecliptic north pole: RA 270°, Dec +66.5607°.
/// let ra = 270.0f64.to_radians();
/// let dec = 66.5607f64.to_radians();
/// let pole = DVec3::new(ra.cos() * dec.cos(), ra.sin() * dec.cos(), dec.sin());
/// let (beta, _) = ecliptic_coords(pole);
/// assert!((beta.to_degrees() - 90.0).abs() < 0.01, "beta = {}", beta.to_degrees());
/// ```
pub fn ecliptic_coords(dir_equatorial: DVec3) -> (f64, f64) {
    let length = dir_equatorial.length();
    if !length.is_finite() || length <= 0.0 {
        return (0.0, 0.0);
    }
    let dir = dir_equatorial / length;
    // Rotate about the x-axis (equinox line) by the obliquity:
    // equatorial (x, y, z) → ecliptic (x, y·cosε + z·sinε, −y·sinε + z·cosε).
    let eps = ECLIPTIC_OBLIQUITY_DEG.to_radians();
    let (sin_eps, cos_eps) = eps.sin_cos();
    let x = dir.x;
    let y = dir.y * cos_eps + dir.z * sin_eps;
    let z = -dir.y * sin_eps + dir.z * cos_eps;
    (z.clamp(-1.0, 1.0).asin(), y.atan2(x))
}

/// Solar elongation θ in radians: the angle between a sky direction and
/// the Sun direction (both equatorial J2000 unit vectors). Inputs are
/// normalized defensively; degenerate input yields π (antisolar, faint).
///
/// ```
/// use game_engine::render::zodiacal::solar_elongation;
/// use glam::DVec3;
/// use std::f64::consts::FRAC_PI_2;
///
/// let sun = DVec3::X;
/// assert_eq!(solar_elongation(sun, sun), 0.0);
/// assert!((solar_elongation(DVec3::Y, sun) - FRAC_PI_2).abs() < 1e-12);
/// ```
pub fn solar_elongation(dir: DVec3, sun_dir: DVec3) -> f64 {
    let (a, b) = (dir.try_normalize(), sun_dir.try_normalize());
    match (a, b) {
        (Some(a), Some(b)) => a.dot(b).clamp(-1.0, 1.0).acos(),
        _ => std::f64::consts::PI,
    }
}

/// What the exposure stage reads: linear zodiacal radiance for a sky
/// direction. Implementations differ only in where μ comes from —
/// analytic formula today, HEALPix map asset tomorrow.
pub trait ZodiacalSource {
    /// Linear proportional radiance (same units as
    /// [`radiance_relative`]) for ecliptic latitude `beta_rad` and solar
    /// elongation `theta_rad`.
    fn radiance(&self, beta_rad: f64, theta_rad: f64) -> f64;
}

/// The spec §9.4 analytic model as a [`ZodiacalSource`].
///
/// ```
/// use game_engine::render::zodiacal::{AnalyticZodiacal, ZodiacalParams, ZodiacalSource};
/// use std::f64::consts::FRAC_PI_2;
///
/// let model = AnalyticZodiacal::new(ZodiacalParams::spec_defaults());
/// let pole = model.radiance(FRAC_PI_2, FRAC_PI_2);
/// let quadrature = model.radiance(0.0, FRAC_PI_2);
/// assert!(quadrature > pole, "ecliptic glows brighter than the pole");
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalyticZodiacal {
    params: ZodiacalParams,
}

impl AnalyticZodiacal {
    /// Wraps validated or default parameters.
    pub fn new(params: ZodiacalParams) -> Self {
        Self { params }
    }

    /// The parameters this instance evaluates.
    pub fn params(self) -> ZodiacalParams {
        self.params
    }
}

impl ZodiacalSource for AnalyticZodiacal {
    fn radiance(&self, beta_rad: f64, theta_rad: f64) -> f64 {
        radiance_relative(surface_brightness_mag(beta_rad, theta_rad, &self.params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    /// Constant-radiance stand-in for the future map-based asset: the
    /// swap test runs the same consumer code against both impls.
    struct StubMapZodiacal {
        value: f64,
    }

    impl ZodiacalSource for StubMapZodiacal {
        fn radiance(&self, _beta_rad: f64, _theta_rad: f64) -> f64 {
            self.value
        }
    }

    fn consumer_total<S: ZodiacalSource>(source: &S) -> f64 {
        // What exposure-tone-mapping will do: sample a few sky
        // directions through the trait only.
        let samples = [
            (FRAC_PI_2, FRAC_PI_2),
            (0.0, FRAC_PI_2),
            (FRAC_PI_2, PI),
            (0.0, 0.25),
        ];
        samples
            .iter()
            .map(|(beta, theta)| source.radiance(*beta, *theta))
            .sum()
    }

    #[test]
    fn params_reject_degenerate_calibration() {
        assert!(ZodiacalParams::new(23.0, 1.5, 20.0, 0.5, 0.7).is_some());
        assert!(ZodiacalParams::new(23.0, 1.5, 0.0, 0.5, 0.7).is_none());
        assert!(ZodiacalParams::new(23.0, 1.5, -5.0, 0.5, 0.7).is_none());
        assert!(ZodiacalParams::new(23.0, 1.5, 20.0, 0.5, -0.1).is_none());
        assert!(ZodiacalParams::new(f64::NAN, 1.5, 20.0, 0.5, 0.7).is_none());
        assert!(ZodiacalParams::new(23.0, 1.5, 20.0, 0.5, f64::INFINITY).is_none());
    }

    #[test]
    fn spec_falloff_pins_pole_and_quadrature() {
        let params = ZodiacalParams::spec_defaults();
        let pole = surface_brightness_mag(FRAC_PI_2, FRAC_PI_2, &params);
        assert!((pole - 22.69).abs() < 0.05, "pole = {pole}");
        let quadrature = surface_brightness_mag(0.0, FRAC_PI_2, &params);
        assert!(
            (quadrature - 21.21).abs() < 0.05,
            "quadrature = {quadrature}"
        );
        let antisolar = surface_brightness_mag(FRAC_PI_2, PI, &params);
        assert!((antisolar - 22.78).abs() < 0.05, "antisolar = {antisolar}");
    }

    #[test]
    fn falloff_is_monotonic_toward_plane_and_sun() {
        let params = ZodiacalParams::spec_defaults();
        // Toward the ecliptic plane at fixed elongation: brighter.
        let mut prev = surface_brightness_mag(FRAC_PI_2, FRAC_PI_2, &params);
        for deg in [60.0f64, 30.0, 0.0] {
            let mu = surface_brightness_mag(deg.to_radians(), FRAC_PI_2, &params);
            assert!(mu < prev, "brighter toward plane at {deg}°: {mu} vs {prev}");
            prev = mu;
        }
        // Toward the Sun along the plane: brighter (until saturation).
        let mut prev = surface_brightness_mag(0.0, PI, &params);
        for deg in [120.0f64, 90.0, 60.0, 30.0] {
            let mu = surface_brightness_mag(0.0, deg.to_radians(), &params);
            assert!(mu < prev, "brighter toward sun at {deg}°: {mu} vs {prev}");
            prev = mu;
        }
    }

    #[test]
    fn clamp_rail_never_triggers_inside_model_range() {
        let params = ZodiacalParams::spec_defaults();
        // Sweep the valid sky: nothing may hit the rails (rails exist
        // for bugs, not for physics).
        for beta_deg in (0..=90).step_by(5) {
            for theta_deg in (15..=180).step_by(5) {
                let mu = surface_brightness_mag(
                    (beta_deg as f64).to_radians(),
                    (theta_deg as f64).to_radians(),
                    &params,
                );
                assert!(
                    mu > MU_BRIGHTEST && mu < MU_FAINTEST,
                    "rail hit at β={beta_deg}° θ={theta_deg}°: {mu}"
                );
            }
        }
    }

    #[test]
    fn non_finite_input_yields_faint_never_nan() {
        let params = ZodiacalParams::spec_defaults();
        for (beta, theta) in [(f64::NAN, 1.0), (1.0, f64::NAN), (f64::INFINITY, 1.0)] {
            let mu = surface_brightness_mag(beta, theta, &params);
            assert_eq!(mu, MU_FAINTEST);
            assert!(radiance_relative(mu).is_finite());
        }
    }

    #[test]
    fn elongation_pins_cardinal_angles() {
        assert_eq!(solar_elongation(DVec3::X, DVec3::X), 0.0);
        assert!((solar_elongation(DVec3::Y, DVec3::X) - FRAC_PI_2).abs() < 1e-12);
        assert!((solar_elongation(-DVec3::X, DVec3::X) - PI).abs() < 1e-12);
    }

    #[test]
    fn interface_accepts_stub_map_asset_unchanged() {
        let analytic = AnalyticZodiacal::new(ZodiacalParams::spec_defaults());
        let stub = StubMapZodiacal { value: 1.0e-9 };
        // Same consumer, both impls — the swap changes no call site.
        let analytic_total = consumer_total(&analytic);
        let stub_total = consumer_total(&stub);
        assert!(analytic_total.is_finite() && analytic_total > 0.0);
        assert!((stub_total - 4.0e-9).abs() < 1e-18);
    }
}
