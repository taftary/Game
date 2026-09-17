//! Analytic atmosphere source terms and the descriptor-driven shell shader.

use super::tier::QualityTier;

/// FAI reference and the alternative reanalysis threshold, both retained so
/// the presentation does not claim one contested boundary as universal.
pub const KARMAN_FAI_M: f64 = 100_000.0;
pub const KARMAN_REANALYSIS_M: f64 = 80_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereParams {
    pub altitude_m: f64,
    pub haze: f64,
    pub sky_depth: f64,
    pub limb_glow: f64,
    pub low_tier: bool,
}

impl AtmosphereParams {
    pub fn new(
        altitude_m: f64,
        haze: f64,
        sky_depth: f64,
        limb_glow: f64,
        tier: QualityTier,
    ) -> Self {
        Self {
            altitude_m: altitude_m.max(0.0),
            haze: haze.clamp(0.0, 1.0),
            sky_depth: sky_depth.clamp(0.0, 1.0),
            limb_glow: limb_glow.clamp(0.0, 1.0),
            low_tier: tier == QualityTier::Low,
        }
    }
}

/// Smooth blue -> indigo -> black sky ramp keyed by altitude.
pub fn sky_color(altitude_m: f64, depth: f64) -> [f64; 3] {
    let t = (altitude_m / KARMAN_FAI_M).clamp(0.0, 1.0);
    let blue = [0.06, 0.24, 0.9];
    let indigo = [0.03, 0.04, 0.3];
    let black = [0.002, 0.003, 0.01];
    let first = if t < 0.65 { t / 0.65 } else { 1.0 };
    let second = ((t - 0.65) / 0.35).clamp(0.0, 1.0);
    let base = mix3(blue, indigo, first);
    let base = mix3(base, black, second);
    [base[0] * depth, base[1] * depth, base[2] * depth]
}

/// Thin limb glow, strongest around the atmospheric boundary and grazing rays.
pub fn limb_glow(altitude_m: f64, cos_view: f64, strength: f64) -> f64 {
    let altitude = 1.0 - ((altitude_m - KARMAN_REANALYSIS_M).abs() / KARMAN_FAI_M).clamp(0.0, 1.0);
    altitude * (1.0 - cos_view.clamp(0.0, 1.0)).powf(2.0) * strength.clamp(0.0, 1.0)
}

/// Aerial haze transmission; mobile uses a cheaper linear approximation.
pub fn haze_transmission(distance_m: f64, altitude_m: f64, haze: f64, low_tier: bool) -> f64 {
    let density = haze.clamp(0.0, 1.0) * (1.0 + (altitude_m / 100_000.0).clamp(0.0, 4.0) * 0.25);
    let optical = density * distance_m.max(0.0) / 250_000.0;
    if low_tier {
        (1.0 - optical).clamp(0.0, 1.0)
    } else {
        (-optical).exp()
    }
}

/// GLSL shell fragment helper. The caller supplies the descriptor push block.
pub const ATMOSPHERE_SHELL_FRAG: &str = r#"
float atmosphere_shell(float altitude_m, float cos_view, float haze,
                       float sky_depth, float limb_strength) {
    float boundary = 1.0 - clamp(abs(altitude_m - 80000.0) / 100000.0, 0.0, 1.0);
    float limb = boundary * pow(1.0 - clamp(cos_view, 0.0, 1.0), 2.0) * limb_strength;
    float aerial = exp(-haze * max(altitude_m, 0.0) / 250000.0);
    return clamp((limb + aerial * sky_depth) * 0.5, 0.0, 1.0);
}
"#;

fn mix3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_deepens_and_limb_is_bounded() {
        let ground = sky_color(0.0, 1.0);
        let orbit = sky_color(KARMAN_FAI_M, 1.0);
        assert!(ground[2] > orbit[2]);
        assert!((0.0..=1.0).contains(&limb_glow(KARMAN_REANALYSIS_M, 0.0, 1.0)));
    }

    #[test]
    fn haze_is_finite_and_monotonic() {
        let near = haze_transmission(1_000.0, 10_000.0, 0.8, false);
        let far = haze_transmission(100_000.0, 10_000.0, 0.8, false);
        assert!(near.is_finite() && far.is_finite() && near > far);
    }
}
