//! Cosmic depth window: fog + slab visibility shared by cosmic draws.
//!
//! Single source of truth for the FR1/FR2 window term. The GLSL snippet
//! ([`COSMIC_WINDOW_GLSL`]) is concatenated into every cosmic vertex
//! shader; the Rust mirrors below pin its semantics in unit tests.

/// Shared GLSL window term: fog over view depth, soft slab cut, hub floor.
/// `clip_w` is view depth (>= 0); `fog_l` <= 0 disables fog;
/// `slab_half` <= 0 disables the slab; hub kinds (>= 1) never drop below 0.25.
pub const COSMIC_WINDOW_GLSL: &str = r#"
float cosmic_window_vis(float clip_w, float fog_l, float slab_center, float slab_half, int kind) {
    float fog = (fog_l <= 0.0) ? 1.0 : 1.0 / (1.0 + (clip_w / fog_l) * (clip_w / fog_l));
    float slab = (slab_half <= 0.0) ? 1.0 : 1.0 - smoothstep(slab_half - 5.0, slab_half + 5.0, abs(clip_w - slab_center));
    float vis = fog * slab;
    vis = (kind >= 1) ? max(vis, 0.25) : vis;
    return vis;
}
"#;

/// Demo fog length in Mpc (notion FR5).
pub const COSMIC_DEMO_FOG_MPC: f32 = 90.0;
/// Slab soft-edge half-width in Mpc (notion FR2).
pub const COSMIC_SLAB_EDGE_MPC: f32 = 5.0;

/// CPU mirror of the GLSL smoothstep.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fog visibility: 1 at the eye, fading as `1/(1+(d/L)^2)`.
pub fn fog_vis(dist_mpc: f32, fog_l_mpc: f32) -> f32 {
    if fog_l_mpc <= 0.0 {
        return 1.0;
    }
    let d = dist_mpc.max(0.0);
    let z = d / fog_l_mpc;
    1.0 / (1.0 + z * z)
}

/// Slab visibility: 1 inside, soft 5 Mpc edge, 0 outside.
pub fn slab_vis(depth_mpc: f32, center_mpc: f32, half_mpc: f32) -> f32 {
    if half_mpc <= 0.0 {
        return 1.0;
    }
    1.0 - smoothstep(
        half_mpc - COSMIC_SLAB_EDGE_MPC,
        half_mpc + COSMIC_SLAB_EDGE_MPC,
        (depth_mpc - center_mpc).abs(),
    )
}

/// Combined window: fog times slab, hub kinds floored at 0.25.
pub fn window_vis(
    dist_mpc: f32,
    fog_l_mpc: f32,
    depth_mpc: f32,
    center: f32,
    half: f32,
    hub_kind: bool,
) -> f32 {
    let vis = fog_vis(dist_mpc, fog_l_mpc) * slab_vis(depth_mpc, center, half);
    if hub_kind { vis.max(0.25) } else { vis }
}

/// Inspector slab state (notion FR3).
pub struct SlabState {
    pub on: bool,
    pub thickness_mpc: f32,
    pub center_mpc: f32,
}

impl SlabState {
    /// Slab disabled.
    pub fn off() -> Self {
        Self {
            on: false,
            thickness_mpc: 30.0,
            center_mpc: 0.0,
        }
    }

    /// Slab enabled with the default 30 Mpc thickness at depth 0.
    pub fn default_on() -> Self {
        Self {
            on: true,
            thickness_mpc: 30.0,
            center_mpc: 0.0,
        }
    }

    /// Clamp thickness into [10, 80] Mpc.
    pub fn set_thickness(&mut self, t: f32) {
        self.thickness_mpc = t.clamp(10.0, 80.0);
    }

    /// Scroll the slab center by a quarter thickness per notch,
    /// clamped to [-radius, radius] around the orbit target.
    pub fn scroll(&mut self, notches: i32, radius: f32) {
        let r = radius.abs();
        let step = self.thickness_mpc / 4.0;
        self.center_mpc = (self.center_mpc + step * notches as f32).clamp(-r, r);
    }
}

/// Move the eye so the framed width is unchanged under a new FOV.
pub fn fov_rescale_distance(distance: f32, old_fov_deg: f32, new_fov_deg: f32) -> f32 {
    distance * (old_fov_deg.to_radians() / 2.0).tan() / (new_fov_deg.to_radians() / 2.0).tan()
}

/// Visible width at a distance for a vertical FOV.
pub fn framed_width(distance: f32, fov_deg: f32) -> f32 {
    2.0 * distance * (fov_deg.to_radians() / 2.0).tan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glsl_declares_window_fn_with_smoothstep() {
        assert!(COSMIC_WINDOW_GLSL.contains("cosmic_window_vis"));
        assert!(COSMIC_WINDOW_GLSL.contains("smoothstep"));
    }

    #[test]
    fn glsl_is_arithmetic_only() {
        for banned in ["exp(", "pow(", "log(", "sin(", "cos("] {
            assert!(
                !COSMIC_WINDOW_GLSL.contains(banned),
                "GLSL must not contain {banned}"
            );
        }
    }

    #[test]
    fn fog_at_zero_is_one() {
        assert_eq!(fog_vis(0.0, COSMIC_DEMO_FOG_MPC), 1.0);
        assert_eq!(fog_vis(0.0, 30.0), 1.0);
    }

    #[test]
    fn fog_decreases_monotonically() {
        let ds = [0.0, 10.0, 50.0, 90.0, 150.0, 500.0, 1.0e6];
        let vs: Vec<f32> = ds
            .iter()
            .map(|d| fog_vis(*d, COSMIC_DEMO_FOG_MPC))
            .collect();
        for w in vs.windows(2) {
            assert!(w[0] > w[1], "fog not decreasing: {vs:?}");
        }
    }

    #[test]
    fn fog_fully_fogged_is_zero() {
        assert!(fog_vis(1.0e6, COSMIC_DEMO_FOG_MPC) < 1e-6);
    }

    #[test]
    fn fog_off_when_length_nonpositive() {
        assert_eq!(fog_vis(1.0e6, 0.0), 1.0);
        assert_eq!(fog_vis(1.0e6, -5.0), 1.0);
    }

    #[test]
    fn slab_center_is_one() {
        assert!((slab_vis(0.0, 0.0, 15.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn slab_far_outside_is_zero() {
        assert_eq!(slab_vis(1000.0, 0.0, 15.0), 0.0);
        assert_eq!(slab_vis(-1000.0, 0.0, 15.0), 0.0);
    }

    #[test]
    fn slab_off_when_half_nonpositive() {
        assert_eq!(slab_vis(999.0, 0.0, 0.0), 1.0);
        assert_eq!(slab_vis(999.0, 0.0, -3.0), 1.0);
    }

    #[test]
    fn hub_floor_holds_at_huge_depth() {
        assert_eq!(window_vis(1.0e6, 90.0, 1.0e6, 0.0, 15.0, true), 0.25);
        assert_eq!(window_vis(1.0e6, 90.0, 1.0e6, 0.0, 15.0, false), 0.0);
    }

    #[test]
    fn slab_thickness_clamps() {
        let mut s = SlabState::default_on();
        s.set_thickness(5.0);
        assert_eq!(s.thickness_mpc, 10.0);
        s.set_thickness(200.0);
        assert_eq!(s.thickness_mpc, 80.0);
        s.set_thickness(30.0);
        assert_eq!(s.thickness_mpc, 30.0);
    }

    #[test]
    fn slab_scroll_clamps_to_radius() {
        let mut s = SlabState::default_on();
        s.scroll(100, 50.0);
        assert_eq!(s.center_mpc, 50.0);
        s.scroll(-200, 50.0);
        assert_eq!(s.center_mpc, -50.0);
        // One notch moves a quarter thickness.
        let mut s = SlabState::default_on();
        s.scroll(1, 500.0);
        assert!((s.center_mpc - 7.5).abs() < 1e-6);
    }

    #[test]
    fn fov_rescale_preserves_framed_width() {
        let d0 = 430.0;
        let d1 = fov_rescale_distance(d0, 60.0, 20.0);
        let w0 = framed_width(d0, 60.0);
        let w1 = framed_width(d1, 20.0);
        assert!((w1 - w0).abs() / w0 < 0.01, "w0={w0} w1={w1}");
    }
}
