//! Post chain: HDR scene-target selection + tonemap resolve pass
//! (`plans/v0.2.0/exposure-tone-mapping`, phases 2–3).
//!
//! The scene renders into an HDR color attachment; a fullscreen
//! resolve pass tone maps into the swapchain image. This module owns
//! the headless-testable half: format selection over a support
//! predicate, the resolve GLSL sources, and the push-constant block.
//! Binaries own the GPU half (images, framebuffers, descriptor sets,
//! pipelines) following the `create_depth_view` precedent.
//!
//! Orientation contract (read before touching the resolve UVs): Vulkan
//! framebuffer row 0 is the top row and holds NDC `+1` content (no
//! Y-flip anywhere — AGENTS.md rendering invariants). Normalized
//! image sampling reads row 0 at `v = 0`. The fullscreen triangle's
//! raw `pos` varying lands `(0, 1)` on the top-left pixel, which would
//! sample the *bottom* row — hence `v_uv = vec2(pos.x, 1.0 - pos.y)`,
//! which samples row 0 at the top. This is the correct mapping, not a
//! flip to "fix": NDC `+1` stays the top row end to end.

use vulkano::buffer::BufferContents;
use vulkano::format::Format;

/// HDR scene-target candidates, most to least preferred. 16F RGBA is
/// the working format (linear headroom for the ~30-stop span);
/// B10G11R11 unsigned-float packed is the mobile-friendly fallback
/// (ADR-007 tiers). Both need `COLOR_ATTACHMENT | SAMPLED_IMAGE`
/// optimal-tiling support — the binary's support predicate checks
/// exactly those flags via `PhysicalDevice::format_properties`.
pub const HDR_FORMAT_PREFERENCE: [Format; 2] =
    [Format::R16G16B16A16_SFLOAT, Format::B10G11R11_UFLOAT_PACK32];

/// Outcome of [`select_hdr_format`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdrSelection {
    /// Render the scene into this HDR format; resolve pass active.
    Hdr(Format),
    /// No candidate is renderable on this device: keep the direct
    /// LDR-to-swapchain path. Content is unchanged, only dynamic-range
    /// headroom is lost (Low-tier contract).
    LdrBypass,
}

/// Picks the HDR scene-target format: first renderable candidate in
/// [`HDR_FORMAT_PREFERENCE`] order wins, else [`HdrSelection::LdrBypass`].
/// The caller supplies the device query as a predicate so selection
/// stays headless-testable; every candidate is probed in order.
///
/// ```
/// use game_engine::render::post::{HdrSelection, select_hdr_format};
/// use vulkano::format::Format;
///
/// // Full support: the working format wins.
/// let sel = select_hdr_format(|_| true);
/// assert_eq!(sel, HdrSelection::Hdr(Format::R16G16B16A16_SFLOAT));
/// // Nothing renderable: content-preserving LDR bypass.
/// assert_eq!(select_hdr_format(|_| false), HdrSelection::LdrBypass);
/// // Only the packed fallback: mobile path.
/// let sel = select_hdr_format(|f| f == Format::B10G11R11_UFLOAT_PACK32);
/// assert_eq!(sel, HdrSelection::Hdr(Format::B10G11R11_UFLOAT_PACK32));
/// ```
pub fn select_hdr_format(mut is_renderable: impl FnMut(Format) -> bool) -> HdrSelection {
    HDR_FORMAT_PREFERENCE
        .iter()
        .find(|f| is_renderable(**f))
        .map(|f| HdrSelection::Hdr(*f))
        .unwrap_or(HdrSelection::LdrBypass)
}

/// Resolve vertex shader: fullscreen triangle from `gl_VertexIndex`
/// alone (no vertex buffers bound — the pipeline uses an empty vertex
/// input). `v_uv` follows the orientation contract above.
pub const RESOLVE_VERT: &str = r"#version 450
layout(location = 0) out vec2 v_uv;
void main() {
    vec2 pos = vec2(float((gl_VertexIndex << 1) & 2), float(gl_VertexIndex & 2));
    v_uv = vec2(pos.x, 1.0 - pos.y);
    gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);
}";

/// ACES fit as a GLSL function: the exact Narkowicz expression the
/// CPU [`aces_approx`](super::exposure::aces_approx) mirrors
/// (`x·(2.51·x+0.03) / (x·(2.43·x+0.59)+0.14)`, clamped to [0, 1]).
/// Single-authored here so the CPU mirror and the shader cannot drift
/// apart unnoticed — the cross-check test pins the constants on both
/// sides.
pub const ACES_FIT_GLSL: &str = r"vec3 aces_fit(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}";

/// Resolve fragment shader, Phase-3 filmic variant: exposure-scaled
/// HDR through [`ACES_FIT_GLSL`]. Composed from [`RESOLVE_FRAG_FIXED`]
/// by string composition (same pattern as
/// `planet_vert_logdepth(PLANET_VERT)`) — the fixed source is never
/// hand-duplicated, so sampling, bindings, and output stay identical
/// between variants.
pub fn resolve_frag_aces() -> String {
    let with_fit = RESOLVE_FRAG_FIXED.replacen(
        "void main() {",
        &format!("{ACES_FIT_GLSL}\nvoid main() {{"),
        1,
    );
    with_fit.replacen("hdr * pc.exposure", "aces_fit(hdr * pc.exposure)", 1)
}

/// Resolve fragment shader, Phase-2 fixed-exposure variant: linear
/// scale only (`exposure = 1.0` verifies the plumbing independently of
/// tone mapping). Separate `texture2D` + `sampler` (naga has no
/// combined `sampler2D` globals — same pattern as the debug UI shader).
/// The ACES variant is composed from this source (see
/// [`resolve_frag_aces`]), never by editing it.
pub const RESOLVE_FRAG_FIXED: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D hdr_tex;
layout(set = 0, binding = 1) uniform sampler hdr_sampler;
layout(push_constant) uniform PushConstants {
    float exposure;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
void main() {
    vec3 hdr = texture(sampler2D(hdr_tex, hdr_sampler), v_uv).rgb;
    f_color = vec4(hdr * pc.exposure, 1.0);
}";

/// Resolve push-constant block: exposure multiplier applied before
/// tone mapping (4 B, far under the 128 B Vulkan 1.1 floor).
///
/// ```
/// use game_engine::render::post::ResolvePush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<ResolvePush>(), 4);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct ResolvePush {
    /// Linear exposure multiplier (1.0 = pass-through).
    pub exposure: f32,
}

// ---------------------------------------------------------------------------
// Bloom chain (update-2026-09-18-2328): threshold extract + separable
// Gaussian blur + bloom-composite resolve. Same split as the rest of
// this module — GLSL sources and params here, GPU resources in the
// binaries (the `game_tools` `ResolvePass` precedent).
// ---------------------------------------------------------------------------

/// Tunable bloom-chain parameters.
///
/// ```
/// use game_engine::render::post::BloomParams;
///
/// let p = BloomParams::spec_defaults();
/// assert_eq!((p.threshold, p.intensity), (1.0, 0.85));
/// assert!(BloomParams::new(1.0, 0.85, 1.6).is_some());
/// assert!(BloomParams::new(-1.0, 0.85, 1.6).is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BloomParams {
    /// HDR threshold: linear luminance above this feeds the bloom
    /// chain (emissive node cores sit at 1.5–4.0 by construction).
    pub threshold: f32,
    /// Bloom weight added back at resolve (0 = chain runs dry).
    pub intensity: f32,
    /// Blur sigma in target texels (the 9-tap kernel below is the
    /// σ = 1.6 specialization; other values rescale the step push).
    pub sigma_texels: f32,
}

impl BloomParams {
    /// Shipped calibration: threshold at the emissive floor, gentle
    /// bloom weight, σ = 1.6 kernel.
    pub const fn spec_defaults() -> Self {
        Self {
            threshold: 1.0,
            intensity: 0.85,
            sigma_texels: 1.6,
        }
    }

    /// Validated constructor: threshold in [0, 8], intensity in
    /// [0, 2], sigma in (0, 8] — all finite.
    pub fn new(threshold: f32, intensity: f32, sigma_texels: f32) -> Option<Self> {
        if threshold.is_finite()
            && (0.0..=8.0).contains(&threshold)
            && intensity.is_finite()
            && (0.0..=2.0).contains(&intensity)
            && sigma_texels.is_finite()
            && sigma_texels > 0.0
            && sigma_texels <= 8.0
        {
            Some(Self {
                threshold,
                intensity,
                sigma_texels,
            })
        } else {
            None
        }
    }
}

/// Separable 9-tap Gaussian weights for σ (offsets 0–4, normalized —
/// the single author of the kernel both sides pin against).
/// Central weight first, then the symmetric pairs.
pub fn gaussian9_weights(sigma: f64) -> [f64; 5] {
    let g = |x: f64| (-x * x / (2.0 * sigma * sigma)).exp();
    let (g0, g1, g2, g3, g4) = (g(0.0), g(1.0), g(2.0), g(3.0), g(4.0));
    let total = g0 + 2.0 * (g1 + g2 + g3 + g4);
    [g0 / total, g1 / total, g2 / total, g3 / total, g4 / total]
}

/// Bright-pass fragment shader: hard threshold extract of the HDR
/// scene into the half-res bloom target (linear throughout — tone
/// mapping happens only at resolve). The 64.0 ceiling is a runaway
/// firewall: no legitimate emissive exceeds it, while an Inf/NaN
/// leak upstream would otherwise smear across both blur scales into
/// the resolve. Same sampler discipline as the resolve shaders
/// (separate `texture2D` + `sampler` for naga).
pub const BLOOM_BRIGHT_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D hdr_tex;
layout(set = 0, binding = 1) uniform sampler hdr_sampler;
layout(push_constant) uniform PushConstants {
    float threshold;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
void main() {
    vec3 hdr = texture(sampler2D(hdr_tex, hdr_sampler), v_uv).rgb;
    vec3 bloom = min(max(hdr - vec3(pc.threshold), vec3(0.0)), vec3(64.0));
    f_color = vec4(bloom, 1.0);
}";

/// Blur fragment shader: one axis of the separable 9-tap Gaussian
/// (σ = 1.6 texels — weights pinned by
/// `bloom_weights_match_cpu_mirror`). The push `step` selects the
/// axis (`(1/w, 0)` then `(0, 1/h)`) and rescales the kernel for the
/// wide pass. Explicit texel fetches, so a nearest sampler suffices.
pub const BLOOM_BLUR_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D bloom_tex;
layout(set = 0, binding = 1) uniform sampler bloom_sampler;
layout(push_constant) uniform PushConstants {
    vec2 step;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
void main() {
    vec3 acc = texture(sampler2D(bloom_tex, bloom_sampler), v_uv).rgb * 0.2504;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + pc.step * 1.0).rgb * 0.2060;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv - pc.step * 1.0).rgb * 0.2060;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + pc.step * 2.0).rgb * 0.1146;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv - pc.step * 2.0).rgb * 0.1146;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + pc.step * 3.0).rgb * 0.0432;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv - pc.step * 3.0).rgb * 0.0432;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + pc.step * 4.0).rgb * 0.0110;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv - pc.step * 4.0).rgb * 0.0110;
    f_color = vec4(acc, 1.0);
}";

/// Resolve fragment shader, bloom-composite variant: exposure-scaled
/// HDR plus the intensity-weighted bloom texture through
/// [`ACES_FIT_GLSL`]. Composed from [`RESOLVE_FRAG_FIXED`] (same
/// pattern as [`resolve_frag_aces`]): the bloom sampler rides
/// bindings 2–3, the fixed source is never hand-duplicated.
pub fn resolve_frag_bloom() -> String {
    let with_fit = RESOLVE_FRAG_FIXED.replacen(
        "void main() {",
        &format!("{ACES_FIT_GLSL}\nvoid main() {{"),
        1,
    );
    let with_bloom_tex = with_fit.replacen(
        "layout(set = 0, binding = 1) uniform sampler hdr_sampler;",
        "layout(set = 0, binding = 1) uniform sampler hdr_sampler;\nlayout(set = 0, binding = 2) uniform texture2D bloom_tex;\nlayout(set = 0, binding = 3) uniform sampler bloom_sampler;",
        1,
    );
    let with_bloom_push = with_bloom_tex.replacen(
        "float exposure;",
        "float exposure;\n    float intensity;",
        1,
    );
    // Scene-sample clamp (below f16 max): dense additive packing can
    // push texels to +Inf per channel, and `aces_fit(Inf)` is NaN —
    // which the swapchain readback renders as random primaries. The
    // clamp keeps overflow out of the fit, whatever the accumulation.
    let with_clamp = with_bloom_push.replacen(
        "vec3 hdr = texture(sampler2D(hdr_tex, hdr_sampler), v_uv).rgb;",
        "vec3 hdr = min(texture(sampler2D(hdr_tex, hdr_sampler), v_uv).rgb, vec3(65000.0));",
        1,
    );
    with_clamp.replacen(
        "f_color = vec4(hdr * pc.exposure, 1.0);",
        "vec3 bloom = texture(sampler2D(bloom_tex, bloom_sampler), v_uv).rgb;\n    f_color = vec4(aces_fit(hdr * pc.exposure + bloom * pc.intensity), 1.0);",
        1,
    )
}

/// Bright-pass push-constant block: HDR threshold (4 B).
///
/// ```
/// use game_engine::render::post::BloomBrightPush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomBrightPush>(), 4);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomBrightPush {
    /// Linear threshold subtracted before clamping at zero.
    pub threshold: f32,
}

/// Blur push-constant block: per-tap texel step selecting the blur
/// axis (8 B).
///
/// ```
/// use game_engine::render::post::BloomBlurPush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomBlurPush>(), 8);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomBlurPush {
    /// Axis step in UV (`(1/w, 0)` horizontal, `(0, 1/h)` vertical,
    /// scaled for the wide pass).
    pub step: [f32; 2],
}

/// Bloom-resolve push-constant block: exposure + bloom weight (8 B).
///
/// ```
/// use game_engine::render::post::BloomResolvePush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomResolvePush>(), 8);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomResolvePush {
    /// Linear exposure multiplier applied to the HDR scene sample.
    pub exposure: f32,
    /// Bloom weight added before tone mapping.
    pub intensity: f32,
}

#[cfg(test)]
mod tests {
    use super::super::shaders::{ShaderKind, compile_glsl_to_spirv};
    use super::*;

    #[test]
    fn selection_prefers_the_working_format_then_falls_back() {
        assert_eq!(
            select_hdr_format(|_| true),
            HdrSelection::Hdr(Format::R16G16B16A16_SFLOAT)
        );
        assert_eq!(
            select_hdr_format(|f| f == Format::B10G11R11_UFLOAT_PACK32),
            HdrSelection::Hdr(Format::B10G11R11_UFLOAT_PACK32)
        );
        assert_eq!(select_hdr_format(|_| false), HdrSelection::LdrBypass);
    }

    #[test]
    fn selection_probes_every_candidate_in_preference_order() {
        let mut probed = Vec::new();
        let sel = select_hdr_format(|f| {
            probed.push(f);
            false
        });
        assert_eq!(sel, HdrSelection::LdrBypass);
        assert_eq!(probed, HDR_FORMAT_PREFERENCE);
    }

    #[test]
    fn resolve_shaders_compile_under_naga() {
        compile_glsl_to_spirv(ShaderKind::Vertex, RESOLVE_VERT)
            .expect("resolve vertex shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, RESOLVE_FRAG_FIXED)
            .expect("resolve fragment shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, &resolve_frag_aces())
            .expect("ACES resolve fragment shader must compile");
    }

    #[test]
    fn aces_composition_shares_the_fixed_source() {
        let aces = resolve_frag_aces();
        // Sampling, bindings, and output survive composition.
        for anchor in [
            "uniform texture2D hdr_tex;",
            "uniform sampler hdr_sampler;",
            "float exposure;",
            "f_color = vec4(",
        ] {
            assert!(aces.contains(anchor), "composition dropped {anchor:?}");
        }
        // The fit wraps the exposure-scaled sample, exactly once.
        assert_eq!(
            aces.matches("aces_fit(hdr * pc.exposure)").count(),
            1,
            "fit must wrap the sample once"
        );
        assert!(
            !aces.contains("hdr * pc.exposure, 1.0"),
            "no unmapped linear output may survive"
        );
    }

    #[test]
    fn bloom_params_validate_and_default() {
        let d = BloomParams::spec_defaults();
        assert_eq!((d.threshold, d.intensity, d.sigma_texels), (1.0, 0.85, 1.6));
        assert!(BloomParams::new(1.0, 0.85, 1.6).is_some());
        assert!(BloomParams::new(8.0, 2.0, 8.0).is_some());
        assert!(BloomParams::new(-0.1, 0.85, 1.6).is_none());
        assert!(BloomParams::new(1.0, 2.1, 1.6).is_none());
        assert!(BloomParams::new(1.0, 0.85, 0.0).is_none());
        assert!(BloomParams::new(f32::NAN, 0.85, 1.6).is_none());
        assert!(BloomParams::new(1.0, 0.85, f32::INFINITY).is_none());
    }

    #[test]
    fn bloom_shaders_compile_under_naga() {
        compile_glsl_to_spirv(ShaderKind::Fragment, BLOOM_BRIGHT_FRAG)
            .expect("bloom bright fragment shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, BLOOM_BLUR_FRAG)
            .expect("bloom blur fragment shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, &resolve_frag_bloom())
            .expect("bloom resolve fragment shader must compile");
    }

    #[test]
    fn bloom_weights_match_cpu_mirror() {
        // The blur kernel embeds the σ = 1.6 specialization as
        // literals; this pins them against the single-authored
        // `gaussian9_weights` (the ACES constant-pin precedent).
        let want = gaussian9_weights(1.6);
        for (tap, w) in want.iter().enumerate() {
            let literal = format!("{w:.4}");
            assert!(
                BLOOM_BLUR_FRAG.contains(&literal),
                "blur kernel lost tap {tap} weight {literal}"
            );
        }
        let total: f64 = want[0] + 2.0 * (want[1] + want[2] + want[3] + want[4]);
        assert!((total - 1.0).abs() < 1e-12, "kernel must sum to 1: {total}");
    }

    #[test]
    fn bloom_bright_clamps_runaway_hdr() {
        // Runaway firewall pin (visual-issue fix): the bright extract
        // must ceiling its output so an Inf/NaN leak upstream cannot
        // smear across both blur scales into the resolve.
        assert!(
            BLOOM_BRIGHT_FRAG.contains("vec3(64.0)"),
            "bright-pass ceiling missing"
        );
    }

    #[test]
    fn bloom_resolve_clamps_scene_sample() {
        // Overflow guard pin (visual-issue fix round 2): the composed
        // resolve must ceiling the scene sample below f16 max before
        // the fit, so +Inf texels can never produce NaN primaries.
        assert!(
            resolve_frag_bloom().contains("vec3(65000.0)"),
            "scene-sample clamp missing from the bloom resolve"
        );
    }

    #[test]
    fn bloom_resolve_composition_shares_the_fixed_source() {
        let bloom = resolve_frag_bloom();
        // Sampling, bindings, and output survive composition.
        for anchor in [
            "uniform texture2D hdr_tex;",
            "uniform sampler hdr_sampler;",
            "uniform texture2D bloom_tex;",
            "uniform sampler bloom_sampler;",
            "float exposure;",
            "float intensity;",
            "f_color = vec4(",
        ] {
            assert!(bloom.contains(anchor), "composition dropped {anchor:?}");
        }
        // The fit wraps scene + weighted bloom exactly once, and no
        // unmapped linear output survives.
        assert_eq!(
            bloom
                .matches("aces_fit(hdr * pc.exposure + bloom * pc.intensity)")
                .count(),
            1,
            "fit must wrap the composite once"
        );
        assert!(
            !bloom.contains("hdr * pc.exposure, 1.0"),
            "no unmapped linear output may survive"
        );
    }

    #[test]
    fn aces_fit_constants_match_the_cpu_mirror() {
        // `aces_approx` and `ACES_FIT_GLSL` implement the same
        // Narkowicz expression; this pins both sides to the same five
        // constants so they cannot drift apart unnoticed.
        for constant in ["2.51", "2.43", "0.59", "0.03", "0.14"] {
            assert!(
                ACES_FIT_GLSL.contains(constant),
                "GLSL fit lost constant {constant}"
            );
        }
        // Spot-check the mirror agrees at the middle-grey anchor.
        let cpu = super::super::exposure::aces_approx(0.18);
        assert!((cpu - 0.267).abs() < 0.005, "cpu mirror drifted: {cpu}");
    }
}
