//! Log-depth buffer kernel (`plans/v0.2.0/log-depth-rendering`, ADR-016).
//!
//! 26 decades of scale cannot fit one linear depth range. Within a single
//! draw pass this module provides the Outerra-style `log(z)` encoding:
//! the vertex shader rewrites `gl_Position.z` so the post-divide depth is
//! logarithmic in view-space distance `w`, and the CPU side mirrors that
//! math exactly for tests and pass planning.
//!
//! Vulkan form (Z in [0, 1] — **not** the OpenGL `2.0 / …` variant):
//!
//! ```text
//! d(w) = log2(1 + w) / log2(1 + far),   w in [0, far]  →  d in [0, 1]
//! ```
//!
//! Scope contract (rendering invariants, `docs/techstack/rendering.md`):
//! the encoding changes **depth writes only**. NDC conventions (NDC +1 =
//! top row), the RH `directx::perspective` projection, and
//! `FrontFace::CounterClockwise` + `CullMode::Back` are untouched —
//! `projection_uses_vulkan_ndc` passes unmodified.
//!
//! Everything here is pure math + GLSL source composition (same shape as
//! [`crate::render::checker`]): headless-testable, no GPU, no I/O. The
//! `*_f32` functions mirror the hardware path bit-for-bit so the
//! separability oracle proves z-fight behavior without a GPU.

/// Guard inside `log2(max(eps, 1 + w))`: keeps the logarithm finite for
/// fragments at or behind the camera plane. Real fragments with `w <= 0`
/// are removed by clipping; the guard only stops NaN poisoning.
///
/// Emitted into GLSL via [`glsl_log_depth_epilogue`] (formatted from this
/// constant, never hand-duplicated).
pub const LOG_GUARD_EPSILON: f32 = 1e-6;

/// Log-depth parameters for one draw pass: the far plane the encoding is
/// normalized against.
///
/// ```
/// use game_engine::render::depth::LogDepthParams;
///
/// let log = LogDepthParams::new(1.0e12).expect("far plane must be positive");
/// assert_eq!(log.encode(0.0), 0.0);
/// assert!((log.encode(1.0e12) - 1.0).abs() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogDepthParams {
    far_plane: f64,
}

impl LogDepthParams {
    /// Builds params for `far_plane` (view-space units, same units as the
    /// `w` passed to [`encode`](Self::encode)). Returns `None` for
    /// non-positive or non-finite input — a pass with no range has no
    /// encoding.
    ///
    /// ```
    /// use game_engine::render::depth::LogDepthParams;
    ///
    /// assert!(LogDepthParams::new(100.0).is_some());
    /// assert!(LogDepthParams::new(0.0).is_none());
    /// assert!(LogDepthParams::new(-5.0).is_none());
    /// assert!(LogDepthParams::new(f64::INFINITY).is_none());
    /// assert!(LogDepthParams::new(f64::NAN).is_none());
    /// ```
    pub fn new(far_plane: f64) -> Option<Self> {
        if far_plane.is_finite() && far_plane > 0.0 {
            Some(Self { far_plane })
        } else {
            None
        }
    }

    /// The far plane this encoding is normalized against.
    pub fn far_plane(self) -> f64 {
        self.far_plane
    }

    /// Outerra `Fcoef` in Vulkan [0, 1] form: `1 / log2(far + 1)`.
    ///
    /// ```
    /// use game_engine::render::depth::LogDepthParams;
    ///
    /// let log = LogDepthParams::new(1.0e12).expect("valid");
    /// let expected = 1.0 / (1.0e12f64 + 1.0).log2();
    /// assert!((log.fcoef() - expected).abs() < 1e-15);
    /// ```
    pub fn fcoef(self) -> f64 {
        1.0 / (self.far_plane + 1.0).log2()
    }

    /// Encodes view-space distance `w` to depth in [0, 1].
    ///
    /// `w <= 0` (at/behind the camera — removed by clipping on GPU) maps
    /// to `0.0`; `w >= far` saturates to `1.0`. Non-finite `w` maps to the
    /// matching endpoint (`+inf → 1.0`, anything else → `0.0`).
    ///
    /// ```
    /// use game_engine::render::depth::LogDepthParams;
    ///
    /// let log = LogDepthParams::new(1.0e6).expect("valid");
    /// assert_eq!(log.encode(-3.0), 0.0);
    /// assert_eq!(log.encode(f64::INFINITY), 1.0);
    /// assert_eq!(log.encode(2.0e6), 1.0);
    /// ```
    pub fn encode(self, w: f64) -> f64 {
        if !w.is_finite() {
            return if w.is_sign_positive() { 1.0 } else { 0.0 };
        }
        if w <= 0.0 {
            return 0.0;
        }
        ((1.0 + w).log2() * self.fcoef()).clamp(0.0, 1.0)
    }

    /// Inverse of [`encode`](Self::encode) on [0, 1]: view-space distance
    /// for a depth value. Used by tests and the band planner, never on
    /// GPU.
    ///
    /// ```
    /// use game_engine::render::depth::LogDepthParams;
    ///
    /// let log = LogDepthParams::new(1.0e12).expect("valid");
    /// for w in [1.0e-3, 1.0, 1.0e6, 1.0e9, 1.0e12] {
    ///     let round_tripped = log.decode(log.encode(w));
    ///     assert!((round_tripped - w).abs() / w < 1e-12, "w = {w}");
    /// }
    /// ```
    pub fn decode(self, d: f64) -> f64 {
        let clamped = if d.is_finite() {
            d.clamp(0.0, 1.0)
        } else {
            0.0
        };
        2.0f64.powf(clamped / self.fcoef()) - 1.0
    }

    /// Hardware-faithful `f32` encoding: exactly what the
    /// [`glsl_log_depth_epilogue`] shader computes after the perspective
    /// divide (`z_clip / w_clip`), evaluated in `f32` like the GPU
    /// interpolators. The separability oracle compares this against
    /// [`linear_depth_f32`].
    ///
    /// ```
    /// use game_engine::render::depth::LogDepthParams;
    ///
    /// let log = LogDepthParams::new(1.0e12).expect("valid");
    /// let d = log.encode_f32(1.0e9);
    /// assert!((0.0..=1.0).contains(&d));
    /// // Matches the f64 path to float precision.
    /// assert!((d as f64 - log.encode(1.0e9)).abs() < 1e-6);
    /// ```
    pub fn encode_f32(self, w: f32) -> f32 {
        let far = self.far_plane as f32;
        (1.0f32 + w).max(LOG_GUARD_EPSILON).log2() / (1.0f32 + far).log2()
    }
}

/// Classic linear perspective depth in [0, 1] (Vulkan form) evaluated in
/// `f32` like the hardware rasterizer:
///
/// ```text
/// d(w) = (far / (far - near)) * (1 - near / w)
/// ```
///
/// Non-positive `w` (clipped on GPU) maps to `0.0`. This is the
/// before-half of the separability oracle: the depth the current D16
/// viewer passes and the depth-less smoke would resolve against.
///
/// ```
/// use game_engine::render::depth::linear_depth_f32;
///
/// assert_eq!(linear_depth_f32(0.0, 0.1, 100.0), 0.0);
/// let d = linear_depth_f32(50.0, 0.1, 100.0);
/// assert!((0.0..=1.0).contains(&d));
/// ```
pub fn linear_depth_f32(w: f32, near: f32, far: f32) -> f32 {
    if w <= 0.0 {
        return 0.0;
    }
    far / (far - near) * (1.0 - near / w)
}

/// Whether two depths land in the same UNORM quantum (`bits` = 16 for
/// `D16_UNORM`): the fixed-point proxy for z-fighting. Inputs outside
/// [0, 1] are clamped first (out-of-range fragments are clipped by
/// hardware, not depth-tested).
///
/// ```
/// use game_engine::render::depth::same_unorm_quantum;
///
/// assert!(same_unorm_quantum(0.5, 0.5 + 1e-6, 16));
/// assert!(!same_unorm_quantum(0.25, 0.75, 16));
/// ```
pub fn same_unorm_quantum(a: f32, b: f32, bits: u32) -> bool {
    let bits = bits.clamp(1, 24);
    let levels = (1u32 << bits) as f32 - 1.0;
    (a.clamp(0.0, 1.0) * levels).round() == (b.clamp(0.0, 1.0) * levels).round()
}

/// Whether two depths are the identical `f32` value (`D32_SFLOAT` stores
/// the float exactly): the float-buffer proxy for z-fighting.
///
/// ```
/// use game_engine::render::depth::same_float_depth;
///
/// assert!(same_float_depth(0.75, 0.75));
/// assert!(!same_float_depth(0.75, 0.75 + 1e-7));
/// ```
pub fn same_float_depth(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits()
}

/// Push-constant field the log-depth vertex shaders read the far plane
/// from. Spliced into the consumer's `PushConstants` block (after the
/// `mat4 mvp` — total stays under the 128 B Vulkan 1.1 floor).
pub fn glsl_log_depth_push_field() -> &'static str {
    "    float log_far;\n"
}

/// Vertex-shader epilogue implementing the encoding: spliced as the last
/// statements of `main`, operating on `gl_Position`, reading
/// `pc.log_far` (see [`glsl_log_depth_push_field`]).
///
/// The epsilon formats from [`LOG_GUARD_EPSILON`] so the Rust and GLSL
/// sides can never drift apart; the structure is pinned by
/// `epilogue_keeps_expected_structure` below.
pub fn glsl_log_depth_epilogue() -> String {
    format!(
        "    gl_Position.z = log2(max({:.1e}, 1.0 + gl_Position.w)) / log2(1.0 + pc.log_far) * gl_Position.w;\n",
        LOG_GUARD_EPSILON
    )
}

/// Composed planet vertex shader with log-depth: the single-authored
/// `PLANET_VERT` body plus [`glsl_log_depth_epilogue`], compiled by
/// consumers through `compile_glsl_to_spirv` like every other shader
/// (stack rule: GLSL in, naga, SPIR-V out).
pub fn planet_vert_logdepth(planet_vert: &str) -> String {
    let mut composed = String::with_capacity(planet_vert.len() + 512);
    // Insert the push field into the push block.
    let push_marker = "    mat4 mvp;\n";
    match planet_vert.find(push_marker) {
        Some(idx) => {
            let (head, tail) = planet_vert.split_at(idx + push_marker.len());
            composed.push_str(head);
            composed.push_str(glsl_log_depth_push_field());
            composed.push_str(tail);
        }
        None => {
            composed.push_str(planet_vert);
        }
    }
    // Splice the epilogue before the closing brace of main: the base
    // shader ends `    v_tint = tint;\n}\n`.
    match composed.rfind("}\n") {
        Some(idx) => {
            composed.insert_str(idx, &glsl_log_depth_epilogue());
        }
        None => {
            composed.push_str(&glsl_log_depth_epilogue());
        }
    }
    composed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shaders::{PLANET_VERT, ShaderKind, compile_glsl_to_spirv};

    /// Scene for the separability oracle (LDP-002): near-coplanar pair
    /// 10 km apart at 1e9 m view distance, near = 0.1 m, far = 1e12 m —
    /// three decades between near and geometry, three more to far.
    const ORACLE_NEAR: f32 = 0.1;
    const ORACLE_FAR: f32 = 1.0e12;
    const ORACLE_W_A: f32 = 1.0e9;
    const ORACLE_W_B: f32 = 1.0e9 + 1.0e4;

    #[test]
    fn fcoef_endpoints_hold() {
        let log = LogDepthParams::new(1.0e12).expect("valid far");
        assert_eq!(log.encode(0.0), 0.0);
        assert!((log.encode(1.0e12) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn encode_is_monotonic_across_26_decades() {
        let log = LogDepthParams::new(1.0e20).expect("valid far");
        let mut prev = log.encode(1.0e-6);
        let mut w: f64 = 1.0e-6;
        while w < 1.0e20 {
            w *= 10.0;
            let d = log.encode(w.min(1.0e20));
            assert!(d > prev, "monotonic at w = {w:e}");
            prev = d;
        }
    }

    #[test]
    fn decode_inverts_encode() {
        let log = LogDepthParams::new(1.0e12).expect("valid far");
        for exponent in -3..=12 {
            let w = 10.0f64.powi(exponent);
            let round_tripped = log.decode(log.encode(w));
            let rel = (round_tripped - w).abs() / w;
            assert!(rel < 1e-12, "w = {w:e}, rel = {rel:e}");
        }
    }

    #[test]
    fn oracle_linear_depth_z_fights_at_decade_range() {
        let a = linear_depth_f32(ORACLE_W_A, ORACLE_NEAR, ORACLE_FAR);
        let b = linear_depth_f32(ORACLE_W_B, ORACLE_NEAR, ORACLE_FAR);
        assert_eq!(a, 1.0, "linear saturates at decade range");
        assert!(same_float_depth(a, b), "D32F linear: pair collides");
        assert!(same_unorm_quantum(a, b, 16), "D16 linear: pair collides");
    }

    #[test]
    fn oracle_log_depth_separates_on_float_buffer() {
        let log = LogDepthParams::new(ORACLE_FAR as f64).expect("valid far");
        let a = log.encode_f32(ORACLE_W_A);
        let b = log.encode_f32(ORACLE_W_B);
        assert!(b > a, "log encoding stays monotonic in f32");
        assert!(
            !same_float_depth(a, b),
            "D32F log: 10 km pair at 1e9 m separates (a={a:e}, b={b:e})"
        );
    }

    #[test]
    fn oracle_log_depth_needs_float_buffer_at_decade_range() {
        // Fixed-point D16 quanta (1/65535) are coarser than the log slope
        // this far out — this is why log passes use D32F, not D16.
        let log = LogDepthParams::new(ORACLE_FAR as f64).expect("valid far");
        let a = log.encode_f32(ORACLE_W_A);
        let b = log.encode_f32(ORACLE_W_B);
        assert!(
            same_unorm_quantum(a, b, 16),
            "D16 log: same pair collides — documents the D32F requirement"
        );
    }

    #[test]
    fn epilogue_keeps_expected_structure() {
        let epilogue = glsl_log_depth_epilogue();
        let epsilon = format!("{:.1e}", LOG_GUARD_EPSILON);
        for token in ["gl_Position", "log2", "pc.log_far"] {
            assert!(
                epilogue.contains(token),
                "epilogue lost {token:?}: {epilogue}"
            );
        }
        assert!(
            epilogue.contains(&epsilon),
            "epilogue guard drifted from LOG_GUARD_EPSILON ({epsilon}): {epilogue}"
        );
    }

    #[test]
    fn composed_planet_shader_compiles_to_floor_safe_spirv() {
        let source = planet_vert_logdepth(PLANET_VERT);
        assert!(source.contains("float log_far;"), "push field spliced");
        assert!(source.contains("gl_Position.z"), "epilogue spliced");
        let words = compile_glsl_to_spirv(ShaderKind::Vertex, &source)
            .expect("composed log-depth vertex shader must compile");
        assert!(!words.is_empty());
        assert_eq!(words[0], 0x0723_0203, "SPIR-V magic");
        assert!(
            words[1] <= 0x0001_0300,
            "SPIR-V version {:#x} exceeds the Vulkan 1.1 ceiling",
            words[1]
        );
    }
}
