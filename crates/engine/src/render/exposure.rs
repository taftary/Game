//! Auto-exposure + filmic tone mapping + dark adaptation
//! (`plans/v0.2.0/exposure-tone-mapping`, ADR-021, spec §9.2).
//!
//! Sun vs background starlight spans ~10 orders of magnitude; surface
//! daylight vs deep space ~30 stops. No fixed exposure covers both, so
//! exposure is driven by the dominant light source in view (Sun, planet
//! albedo, ambient starlight), scene radiance is mapped filmically
//! (ACES fit, never clipping), and star visibility fades in gradually
//! as sky luminance falls across the twilight stages.
//!
//! Everything here is pure math over caller-supplied scalars: the
//! module takes key luminances in, returns exposure multipliers and
//! visibility factors out. No Sun position, albedo field, or waypoint
//! runtime exists in code yet — the per-frame loop consumes
//! caller-supplied key inputs (`waypoint-transitions` owns the real
//! frame→light wiring later; the headless sweep harness is the caller
//! for DoD-1).
//!
//! Photometric contract: [`zodiacal`](super::zodiacal) emits *relative*
//! linear radiance; [`PHOTOMETRIC_ZERO_POINT`] is the absolute anchor
//! this feature owns, scaling relative source radiance into normalized
//! scene luminance where `1.0` is the daylight reference. The constant
//! ships as the identity anchor pending photographic validation
//! (same deferred-validation discipline as the zodiacal feature's
//! DoD-2) — the interface carries the contract, not the constant.

/// Absolute photometric anchor: relative source radiance × this =
/// normalized scene luminance (`1.0` = daylight reference). Ships as
/// the identity anchor; photographic validation (COBE/DIRBE or HST
/// background references, per spec §9.4) sets the final number before
/// release. Callers must route every environment source term through
/// [`calibrate`] so the validation lands in one place.
pub const PHOTOMETRIC_ZERO_POINT: f64 = 1.0;

/// Middle-grey anchor for [`exposure_for_key`]: the key luminance that
/// maps to a display value of [`MIDDLE_GREY`] before tone mapping.
pub const MIDDLE_GREY: f64 = 0.18;

/// Minimum exposure multiplier (2^-15): bounds the 30-stop daylight →
/// deep-space span on the bright end so a sunlit key cannot drive the
/// multiplier to zero. Bug-catching rail, not physics.
pub const EXPOSURE_MIN: f64 = 1.0 / 32768.0;

/// Maximum exposure multiplier (2^15): matching rail on the dark end
/// so a near-black key cannot drive the multiplier to infinity.
pub const EXPOSURE_MAX: f64 = 32768.0;

/// The hero light source currently keying auto-exposure (spec §9.2:
/// Sun, planet albedo, or ambient starlight when nothing brighter
/// dominates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DominantSource {
    /// Resolved Sun (disk or point): the key light of waypoints 6–10.
    Sun,
    /// Reflected planet light (surface daylight, albedo-driven keys).
    PlanetAlbedo,
    /// Ambient starlight: the floor when no brighter source dominates.
    Starlight,
}

/// Tunable exposure behavior. Fields are public so validators can tune
/// pacing against captures; [`spec_defaults`](Self::spec_defaults)
/// pins the shipped initials.
///
/// ```
/// use game_engine::render::exposure::ExposureParams;
///
/// let params = ExposureParams::spec_defaults();
/// assert!(ExposureParams::new(0.1, 3.0, 0.5, -3.5, -5.5, -7.0).is_some());
/// assert!(ExposureParams::new(0.0, 3.0, 0.5, -3.5, -5.5, -7.0).is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureParams {
    /// Relative margin a challenger must clear to dethrone the
    /// incumbent source, in (0, 1): challenger wins only if
    /// `challenger > incumbent * (1 + band)`.
    pub hysteresis_band: f64,
    /// Light-adaptation rate (1/s, > 0): exposure chase speed when the
    /// scene brightens. Faster than [`Self::adapt_down_rate`].
    pub adapt_up_rate: f64,
    /// Dark-adaptation rate (1/s, > 0): chase speed when the scene
    /// darkens. Deliberately slower (mirrors scotopic behavior — UX
    /// note in the parent notion).
    pub adapt_down_rate: f64,
    /// log10 sky luminance where stars start fading in (civil
    /// twilight, < 0, brightest of the three).
    pub twilight_civil: f64,
    /// log10 sky luminance of the mid fade (nautical twilight).
    pub twilight_nautical: f64,
    /// log10 sky luminance where stars are fully in (astronomical
    /// twilight, darkest of the three).
    pub twilight_astronomical: f64,
}

impl ExposureParams {
    /// Shipped initials: 10% switch hysteresis, ~1 s light adaptation,
    /// multi-second dark adaptation, twilight stages at −3.5 / −5.5 /
    /// −7.0 log10 below the daylight reference.
    pub fn spec_defaults() -> Self {
        Self {
            hysteresis_band: 0.1,
            adapt_up_rate: 3.0,
            adapt_down_rate: 0.5,
            twilight_civil: -3.5,
            twilight_nautical: -5.5,
            twilight_astronomical: -7.0,
        }
    }

    /// Validated constructor: `None` on non-finite input, a
    /// non-positive hysteresis band, non-positive rates, non-negative
    /// twilight thresholds, or thresholds that are not strictly
    /// descending (civil brightest, astronomical darkest).
    pub fn new(
        hysteresis_band: f64,
        adapt_up_rate: f64,
        adapt_down_rate: f64,
        twilight_civil: f64,
        twilight_nautical: f64,
        twilight_astronomical: f64,
    ) -> Option<Self> {
        let finite = [
            hysteresis_band,
            adapt_up_rate,
            adapt_down_rate,
            twilight_civil,
            twilight_nautical,
            twilight_astronomical,
        ]
        .iter()
        .all(|v| v.is_finite());
        let descending = twilight_civil < 0.0
            && twilight_nautical < twilight_civil
            && twilight_astronomical < twilight_nautical;
        if finite
            && hysteresis_band > 0.0
            && hysteresis_band < 1.0
            && adapt_up_rate > 0.0
            && adapt_down_rate > 0.0
            && descending
        {
            Some(Self {
                hysteresis_band,
                adapt_up_rate,
                adapt_down_rate,
                twilight_civil,
                twilight_nautical,
                twilight_astronomical,
            })
        } else {
            None
        }
    }
}

/// Scales a relative source radiance (e.g.
/// [`radiance_relative`](super::zodiacal::radiance_relative)) into
/// normalized scene luminance via [`PHOTOMETRIC_ZERO_POINT`].
///
/// ```
/// use game_engine::render::exposure::calibrate;
///
/// assert_eq!(calibrate(2.0), 2.0);
/// assert_eq!(calibrate(f64::NAN), 0.0);
/// ```
pub fn calibrate(relative_radiance: f64) -> f64 {
    if relative_radiance.is_finite() && relative_radiance > 0.0 {
        relative_radiance * PHOTOMETRIC_ZERO_POINT
    } else {
        0.0
    }
}

/// Picks the dominant light source from per-source key luminances.
/// The incumbent keeps the crown on ties and near-ties: a challenger
/// wins only by clearing `band` (see [`ExposureParams`]), so the
/// waypoint 6 → 5 Sun-disk→point handoff switches exactly once instead
/// of oscillating. Non-finite keys sanitize to zero.
///
/// ```
/// use game_engine::render::exposure::{DominantSource, select_source};
///
/// // Sun dominates deep-space keys by orders of magnitude.
/// assert_eq!(
///     select_source(1.0, 0.1, 1e-7, DominantSource::Starlight, 0.1),
///     DominantSource::Sun
/// );
/// // Near-tie: the incumbent (Sun) holds against a 5% challenger.
/// assert_eq!(
///     select_source(1.0, 1.05, 1e-7, DominantSource::Sun, 0.1),
///     DominantSource::Sun
/// );
/// // A 20% challenger clears the 10% band and takes over.
/// assert_eq!(
///     select_source(1.0, 1.2, 1e-7, DominantSource::Sun, 0.1),
///     DominantSource::PlanetAlbedo
/// );
/// ```
pub fn select_source(
    sun_key: f64,
    albedo_key: f64,
    starlight_key: f64,
    current: DominantSource,
    band: f64,
) -> DominantSource {
    fn clean(v: f64) -> f64 {
        if v.is_finite() && v > 0.0 { v } else { 0.0 }
    }
    let keys = [
        (DominantSource::Sun, clean(sun_key)),
        (DominantSource::PlanetAlbedo, clean(albedo_key)),
        (DominantSource::Starlight, clean(starlight_key)),
    ];
    let incumbent = keys
        .iter()
        .find(|(s, _)| *s == current)
        .map(|(_, v)| *v)
        .unwrap_or(0.0);
    let (best_source, best_value) = keys
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .copied()
        .unwrap_or((current, incumbent));
    let margin = if band.is_finite() && band > 0.0 {
        1.0 + band
    } else {
        1.0
    };
    if best_source == current || best_value > incumbent * margin {
        best_source
    } else {
        current
    }
}

/// Exposure multiplier for a key luminance: `MIDDLE_GREY / key`,
/// clamped to [`EXPOSURE_MIN`]..=[`EXPOSURE_MAX`]. A `+INFINITY` key
/// (blinding) pins to `EXPOSURE_MIN`; NaN, negative, and
/// non-positive keys pin to `EXPOSURE_MAX` (darkest assumption — the
/// tone mapper, not a runaway multiplier, handles the bright end).
///
/// ```
/// use game_engine::render::exposure::{EXPOSURE_MAX, EXPOSURE_MIN, MIDDLE_GREY, exposure_for_key};
///
/// assert_eq!(exposure_for_key(MIDDLE_GREY), 1.0);
/// assert_eq!(exposure_for_key(0.0), EXPOSURE_MAX);
/// assert_eq!(exposure_for_key(f64::NAN), EXPOSURE_MAX);
/// assert_eq!(exposure_for_key(f64::INFINITY), EXPOSURE_MIN);
/// ```
pub fn exposure_for_key(key_luminance: f64) -> f64 {
    if key_luminance == f64::INFINITY {
        EXPOSURE_MIN
    } else if key_luminance.is_finite() && key_luminance > 0.0 {
        (MIDDLE_GREY / key_luminance).clamp(EXPOSURE_MIN, EXPOSURE_MAX)
    } else {
        EXPOSURE_MAX
    }
}

/// Steps the adapted exposure toward `target` over `dt` seconds with
/// separate up/down rates (see [`ExposureParams`]): exponential
/// approach, so large scene swings ease in instead of popping.
/// Non-positive `dt` or a non-finite target holds the current value.
///
/// ```
/// use game_engine::render::exposure::adapt_exposure;
///
/// // Frozen time: no motion.
/// assert_eq!(adapt_exposure(1.0, 2.0, 0.0, 3.0, 0.5), 1.0);
/// // Brightening chases fast, darkening creeps (same dt, same gap).
/// let up = adapt_exposure(1.0, 2.0, 0.5, 3.0, 0.5);
/// let down = adapt_exposure(2.0, 1.0, 0.5, 3.0, 0.5);
/// assert!((up - 1.0) > (2.0 - down));
/// // Long dwells converge.
/// assert!((adapt_exposure(1.0, 2.0, 60.0, 3.0, 0.5) - 2.0).abs() < 1e-9);
/// ```
pub fn adapt_exposure(
    current: f64,
    target: f64,
    dt_seconds: f64,
    up_rate: f64,
    down_rate: f64,
) -> f64 {
    if !(dt_seconds.is_finite() && dt_seconds > 0.0)
        || !target.is_finite()
        || !(up_rate > 0.0 && up_rate.is_finite())
        || !(down_rate > 0.0 && down_rate.is_finite())
    {
        return current;
    }
    let rate = if target > current { up_rate } else { down_rate };
    current + (target - current) * (1.0 - (-rate * dt_seconds).exp())
}

/// ACES-fitted filmic curve (Narkowicz fit), the CPU mirror of the
/// resolve-shader tone mapper: `x·(2.51·x+0.03) / (x·(2.43·x+0.59)+0.14)`,
/// clamped to [0, 1]. Never clips by construction — highlight detail
/// rolls off instead of hard-clipping (DoD-3). Non-finite or negative
/// input yields 0.0.
///
/// ```
/// use game_engine::render::exposure::aces_approx;
///
/// assert_eq!(aces_approx(0.0), 0.0);
/// assert!((aces_approx(0.18) - 0.267).abs() < 0.005);
/// assert_eq!(aces_approx(1.0e6), 1.0);
/// ```
pub fn aces_approx(x: f64) -> f64 {
    if !(x.is_finite() && x > 0.0) {
        return 0.0;
    }
    let num = x * (2.51 * x + 0.03);
    let den = x * (2.43 * x + 0.59) + 0.14;
    (num / den).clamp(0.0, 1.0)
}

/// Star visibility factor in [0, 1] for a sky luminance: 0 above the
/// civil threshold, smooth fade through nautical, 1 below the
/// astronomical threshold — stars fade in across the twilight stages,
/// never popping at a fixed altitude (DoD-2). Non-finite or negative
/// input hides stars (0.0) rather than emitting garbage.
///
/// ```
/// use game_engine::render::exposure::{ExposureParams, star_visibility};
///
/// let p = ExposureParams::spec_defaults();
/// let daylight = 10f64.powf(p.twilight_civil) * 10.0;
/// let deep_night = 10f64.powf(p.twilight_astronomical) / 10.0;
/// assert_eq!(star_visibility(daylight, &p), 0.0);
/// assert_eq!(star_visibility(deep_night, &p), 1.0);
/// let mid = 10f64.powf((p.twilight_civil + p.twilight_astronomical) / 2.0);
/// let v = star_visibility(mid, &p);
/// assert!(v > 0.0 && v < 1.0);
/// ```
pub fn star_visibility(sky_luminance: f64, params: &ExposureParams) -> f64 {
    if !(sky_luminance.is_finite() && sky_luminance > 0.0) {
        return 0.0;
    }
    let log_sky = sky_luminance.log10();
    // 1 below astronomical, 0 above civil, smoothstep between.
    let t = ((log_sky - params.twilight_astronomical)
        / (params.twilight_civil - params.twilight_astronomical))
        .clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    1.0 - smooth
}

/// Per-frame auto-exposure state: the dominant source plus the adapted
/// multiplier. [`step`](Self::step) folds one frame of key inputs
/// through hysteresis selection, key→exposure mapping, and rate-split
/// adaptation — the whole loop stays pure so the headless sweep
/// harness (DoD-1) and binaries share the exact behavior.
///
/// ```
/// use game_engine::render::exposure::{DominantSource, ExposureLoop, ExposureParams};
///
/// let params = ExposureParams::spec_defaults();
/// let mut loop_ = ExposureLoop::new(DominantSource::Sun, 1.0);
/// let (source, exposure) = loop_.step(1.0, 0.1, 1e-7, 1.0 / 60.0, &params);
/// assert_eq!(source, DominantSource::Sun);
/// assert!(exposure > 0.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureLoop {
    /// Currently keying source (hysteresis-filtered).
    pub source: DominantSource,
    /// Adapted exposure multiplier.
    pub exposure: f64,
}

impl ExposureLoop {
    /// Starts at the given source and multiplier.
    pub fn new(source: DominantSource, exposure: f64) -> Self {
        Self { source, exposure }
    }

    /// Advances one frame: re-selects the dominant source (hysteresis
    /// holds the incumbent on near-ties), maps its key to a target
    /// exposure, and eases toward it at the up/down rate. Returns the
    /// new `(source, exposure)`. A non-positive `dt` still re-selects
    /// the source (keys are instantaneous) but holds the multiplier.
    pub fn step(
        &mut self,
        sun_key: f64,
        albedo_key: f64,
        starlight_key: f64,
        dt_seconds: f64,
        params: &ExposureParams,
    ) -> (DominantSource, f64) {
        self.source = select_source(
            sun_key,
            albedo_key,
            starlight_key,
            self.source,
            params.hysteresis_band,
        );
        let key = match self.source {
            DominantSource::Sun => sun_key,
            DominantSource::PlanetAlbedo => albedo_key,
            DominantSource::Starlight => starlight_key,
        };
        self.exposure = adapt_exposure(
            self.exposure,
            exposure_for_key(key),
            dt_seconds,
            params.adapt_up_rate,
            params.adapt_down_rate,
        );
        (self.source, self.exposure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_reject_degenerate_pacing() {
        assert!(ExposureParams::new(0.1, 3.0, 0.5, -3.5, -5.5, -7.0).is_some());
        assert!(ExposureParams::spec_defaults().hysteresis_band == 0.1);
        // Band must be a (0, 1) margin.
        assert!(ExposureParams::new(0.0, 3.0, 0.5, -3.5, -5.5, -7.0).is_none());
        assert!(ExposureParams::new(1.0, 3.0, 0.5, -3.5, -5.5, -7.0).is_none());
        // Rates must be positive.
        assert!(ExposureParams::new(0.1, 0.0, 0.5, -3.5, -5.5, -7.0).is_none());
        assert!(ExposureParams::new(0.1, 3.0, -0.5, -3.5, -5.5, -7.0).is_none());
        // Twilight stages must strictly descend below zero.
        assert!(ExposureParams::new(0.1, 3.0, 0.5, -3.5, -3.5, -7.0).is_none());
        assert!(ExposureParams::new(0.1, 3.0, 0.5, 1.0, -5.5, -7.0).is_none());
        assert!(ExposureParams::new(0.1, 3.0, 0.5, -3.5, -5.5, f64::NAN).is_none());
    }

    #[test]
    fn selection_holds_incumbent_on_ties_and_near_ties() {
        // Exact tie: incumbent keeps the crown.
        assert_eq!(
            select_source(1.0, 1.0, 1.0, DominantSource::PlanetAlbedo, 0.1),
            DominantSource::PlanetAlbedo
        );
        // Zero keys: incumbent holds (starlight floor wins ties).
        assert_eq!(
            select_source(0.0, 0.0, 0.0, DominantSource::Sun, 0.1),
            DominantSource::Sun
        );
        // NaN sanitizes to zero — finite incumbent survives.
        assert_eq!(
            select_source(
                f64::NAN,
                0.5,
                f64::NEG_INFINITY,
                DominantSource::PlanetAlbedo,
                0.1
            ),
            DominantSource::PlanetAlbedo
        );
    }

    #[test]
    fn selection_switches_exactly_once_across_handoff_sweep() {
        // Waypoint 6 → 5 rehearsal: leaving the solar system dims every
        // bright key — the Sun collapses fastest (disk → point), albedo
        // recedes with its planet, starlight holds its floor. The crown
        // must pass Sun → PlanetAlbedo → Starlight with no flicker back
        // (each switch is monotonic in the sweep).
        let params = ExposureParams::spec_defaults();
        let mut current = DominantSource::Sun;
        let mut switches = 0;
        let mut last_index = 0;
        let order = [
            DominantSource::Sun,
            DominantSource::PlanetAlbedo,
            DominantSource::Starlight,
        ];
        let (mut sun, mut albedo) = (1.0, 0.1);
        for _ in 0..2000 {
            let next = select_source(sun, albedo, 1e-7, current, params.hysteresis_band);
            if next != current {
                switches += 1;
                let idx = order.iter().position(|s| *s == next).unwrap();
                assert!(
                    idx > last_index,
                    "crown moved backward: {current:?} -> {next:?}"
                );
                last_index = idx;
                current = next;
            }
            sun *= 0.97;
            albedo *= 0.985;
        }
        assert_eq!(current, DominantSource::Starlight);
        assert_eq!(switches, 2, "expected exactly two handoffs, got {switches}");
    }

    #[test]
    fn exposure_rails_bound_the_thirty_stop_span() {
        assert!((exposure_for_key(1.0) - MIDDLE_GREY).abs() < 1e-12);
        // Blinding key pins to the floor rail; near-black key to the cap.
        assert_eq!(exposure_for_key(1.0e9), EXPOSURE_MIN);
        assert_eq!(exposure_for_key(1.0e-12), EXPOSURE_MAX);
        assert_eq!(exposure_for_key(f64::INFINITY), EXPOSURE_MIN);
    }

    #[test]
    fn adaptation_is_monotone_and_rate_split() {
        // Same gap, same dt: brightening covers more ground.
        let up = adapt_exposure(1.0, 2.0, 0.5, 3.0, 0.5);
        let down = adapt_exposure(2.0, 1.0, 0.5, 3.0, 0.5);
        assert!(up > 1.0 && up < 2.0);
        assert!(down > 1.0 && down < 2.0);
        assert!((up - 1.0) > (2.0 - down));
        // Shorter dt covers less ground (no pops from big steps).
        let small = adapt_exposure(1.0, 2.0, 0.01, 3.0, 0.5);
        assert!((small - 1.0) < (up - 1.0));
        // Degenerate inputs hold position.
        assert_eq!(adapt_exposure(1.0, f64::NAN, 0.5, 3.0, 0.5), 1.0);
        assert_eq!(adapt_exposure(1.0, 2.0, -1.0, 3.0, 0.5), 1.0);
    }

    #[test]
    fn aces_never_clips_and_rises_monotonically() {
        let params = ExposureParams::spec_defaults();
        let _ = params;
        let mut prev = 0.0;
        let mut x = 0.0;
        while x <= 1.0e6 {
            let y = aces_approx(x);
            assert!(y.is_finite() && y <= 1.0, "clip or NaN at x={x}: {y}");
            assert!(y >= prev, "non-monotonic at x={x}: {y} < {prev}");
            prev = y;
            x = if x < 1.0 { x + 0.01 } else { x * 1.5 };
        }
        assert_eq!(prev, 1.0);
    }

    #[test]
    fn twilight_fade_is_gradual_with_no_step() {
        let params = ExposureParams::spec_defaults();
        // Sweep daylight → deep night: visibility rises monotonically
        // from exactly 0 to exactly 1.
        let mut prev = 0.0;
        let mut seen_mid = false;
        for i in 0..=200 {
            let log_sky = 1.0 - (i as f64) * (9.0 / 200.0);
            let v = star_visibility(10f64.powf(log_sky), &params);
            assert!(v >= prev, "pop at log10={log_sky}: {v} < {prev}");
            if v > 0.0 && v < 1.0 {
                seen_mid = true;
            }
            prev = v;
        }
        assert_eq!(prev, 1.0);
        assert!(seen_mid, "fade must pass through partial visibility");
        assert_eq!(star_visibility(f64::NAN, &params), 0.0);
    }

    #[test]
    fn calibrate_routes_every_source_through_the_zero_point() {
        assert_eq!(calibrate(2.0), 2.0 * PHOTOMETRIC_ZERO_POINT);
        assert_eq!(calibrate(0.0), 0.0);
        assert_eq!(calibrate(-1.0), 0.0);
        // Zodiacal contract end to end: pole radiance calibrates finite.
        let pole = super::super::zodiacal::radiance_relative(22.69);
        let scene = calibrate(pole);
        assert!(scene.is_finite() && scene > 0.0);
    }

    #[test]
    fn loop_steps_source_then_eases_the_multiplier() {
        let params = ExposureParams::spec_defaults();
        let mut loop_ = ExposureLoop::new(DominantSource::Sun, 1.0);
        // Sun keys dominate: crown holds, multiplier eases toward the
        // sun-key target but does not teleport (dt = one frame).
        let (source, exposure) = loop_.step(1.0, 0.1, 1e-7, 1.0 / 60.0, &params);
        assert_eq!(source, DominantSource::Sun);
        // Target is exposure_for_key(1.0) = 0.18; one frame must move
        // toward it without arriving.
        assert!(
            (0.18..1.0).contains(&exposure),
            "one frame must move toward, not reach, the target (got {exposure})"
        );
        // Frozen time: source still re-selects, multiplier holds.
        let (source, held) = loop_.step(1e-9, 1e-8, 1e-7, 0.0, &params);
        assert_eq!(source, DominantSource::Starlight);
        assert_eq!(held, exposure);
    }

    #[test]
    fn loop_handoff_sweep_is_continuous_with_no_pops() {
        // End-to-end DoD-1 rehearsal through the real loop: 6 → 5 key
        // sweep at 60 Hz. The crown must change hands exactly twice and
        // no single frame may jump the multiplier (adaptation eases
        // every swing — the no-visible-pop requirement).
        let params = ExposureParams::spec_defaults();
        let mut loop_ = ExposureLoop::new(DominantSource::Sun, 1.0);
        let dt = 1.0 / 60.0;
        let (mut sun, mut albedo) = (1.0, 0.1);
        let mut switches = 0;
        let mut prev_source = DominantSource::Sun;
        let mut prev = 1.0;
        let mut max_step_ratio = 0.0f64;
        for _ in 0..4000 {
            let (source, exposure) = loop_.step(sun, albedo, 1e-7, dt, &params);
            if source != prev_source {
                switches += 1;
                prev_source = source;
            }
            max_step_ratio = max_step_ratio.max((exposure / prev - 1.0).abs());
            prev = exposure;
            sun *= 0.99;
            albedo *= 0.995;
        }
        assert_eq!(loop_.source, DominantSource::Starlight);
        assert_eq!(switches, 2, "expected exactly two handoffs, got {switches}");
        assert!(
            max_step_ratio < 0.05,
            "frame-to-frame exposure jump {max_step_ratio} would pop"
        );
    }
}
