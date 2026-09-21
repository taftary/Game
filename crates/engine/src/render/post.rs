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

/// Mip-prefilter push-constant block: soft-knee threshold + knee +
/// source texel size (16 B).
///
/// ```
/// use game_engine::render::post::BloomPrefilterPush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomPrefilterPush>(), 16);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomPrefilterPush {
    /// Knee center (linear luminance feeding the chain).
    pub threshold: f32,
    /// Soft-knee half-width around the threshold.
    pub knee: f32,
    /// One source texel in UV (scales the 13-tap offsets).
    pub texel: [f32; 2],
}

/// Mip-downsample push-constant block: source texel size (8 B).
///
/// ```
/// use game_engine::render::post::BloomDownPush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomDownPush>(), 8);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomDownPush {
    /// One source-level texel in UV.
    pub texel: [f32; 2],
}

/// Mip-upsample push-constant block: coarse-level weight (4 B).
///
/// ```
/// use game_engine::render::post::BloomUpPush;
/// use std::mem::size_of;
///
/// assert_eq!(size_of::<BloomUpPush>(), 4);
/// ```
#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct BloomUpPush {
    /// Weight of the tented coarse level (`level_weights[k]`).
    pub weight: f32,
}

// ---------------------------------------------------------------------------
// Mip bloom chain (plans/v0.3.3/bloom-mip-chain): soft-knee prefilter +
// Jimenez 13-tap downsample pyramid + 3x3 tent upsample. Same split as the
// rest of this module — GLSL sources and params here, GPU resources in the
// binaries. Every level is its own image, written once (Intel UHD 620
// write-once rule).
// ---------------------------------------------------------------------------

/// 13-tap downsample offsets in texels with weights (CPU mirror of the
/// [`BLOOM_PREFILTER_FRAG`] / [`BLOOM_DOWN_FRAG`] kernel): center, four
/// inner diagonals (±1,±1), four axes (±2,0)/(0,±2), four outer
/// diagonals (±2,±2). Weights sum to 1.0.
pub fn jimenez13_offsets() -> [([f32; 2], f32); 13] {
    [
        ([0.0, 0.0], 0.125),
        ([1.0, 1.0], 0.125),
        ([-1.0, 1.0], 0.125),
        ([1.0, -1.0], 0.125),
        ([-1.0, -1.0], 0.125),
        ([2.0, 0.0], 0.0625),
        ([-2.0, 0.0], 0.0625),
        ([0.0, 2.0], 0.0625),
        ([0.0, -2.0], 0.0625),
        ([2.0, 2.0], 0.03125),
        ([-2.0, 2.0], 0.03125),
        ([2.0, -2.0], 0.03125),
        ([-2.0, -2.0], 0.03125),
    ]
}

/// 3x3 tent weights, row-major (CPU mirror of [`BLOOM_UP_FRAG`]):
/// corners 1/16, edges 2/16, center 4/16. Sums to 1.0.
pub fn tent9_weights() -> [f64; 9] {
    [
        0.0625, 0.125, 0.0625, //
        0.125, 0.25, 0.125, //
        0.0625, 0.125, 0.0625,
    ]
}

/// Soft-knee threshold (CPU mirror of the `knee_soft` GLSL in
/// [`BLOOM_PREFILTER_FRAG`]): zero below `t - k`, quadratic on
/// `[t - k, t + k]`, linear `x - t` above. Rational only, no `exp`.
pub fn soft_knee(x: f64, t: f64, k: f64) -> f64 {
    let u = x - t + k;
    if u <= 0.0 {
        0.0
    } else if u < 2.0 * k {
        u * u / (4.0 * k + 1e-6)
    } else {
        x - t
    }
}

/// Tunable mip-bloom-chain parameters (per `bloom-mip-chain` FR2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MipBloomParams {
    /// HDR threshold feeding the chain (knee softens its onset).
    pub threshold: f32,
    /// Soft-knee half-width around the threshold.
    pub knee: f32,
    /// Bloom weight added back at resolve.
    pub intensity: f32,
    /// Pyramid levels in [2, 5] (down images ½ … ½^levels).
    pub levels: u8,
    /// Per-level up-pass weights (fine core … coarse halo).
    pub level_weights: [f32; 5],
}

impl MipBloomParams {
    /// Shipped calibration: threshold at the emissive floor, half-unit
    /// knee, gentle weight, 4 levels, halo-weighted falloff (grade
    /// round 1, 2026-09-21: coarse levels carry the Illustris halo
    /// tail — the core levels alone read as the old tight lobe).
    pub const fn spec_defaults() -> Self {
        Self {
            threshold: 1.0,
            knee: 0.5,
            intensity: 0.85,
            levels: 4,
            level_weights: [1.0, 0.9, 0.8, 0.7, 0.6],
        }
    }

    /// Validated constructor: threshold in [0, 8], knee in [0, 2],
    /// intensity in [0, 2], levels in [2, 5], weights finite and
    /// non-negative — all finite.
    pub fn new(
        threshold: f32,
        knee: f32,
        intensity: f32,
        levels: u8,
        level_weights: [f32; 5],
    ) -> Option<Self> {
        if threshold.is_finite()
            && (0.0..=8.0).contains(&threshold)
            && knee.is_finite()
            && (0.0..=2.0).contains(&knee)
            && intensity.is_finite()
            && (0.0..=2.0).contains(&intensity)
            && (2..=5).contains(&levels)
            && level_weights.iter().all(|w| w.is_finite() && *w >= 0.0)
        {
            Some(Self {
                threshold,
                knee,
                intensity,
                levels,
                level_weights,
            })
        } else {
            None
        }
    }

    /// Tier presets: Low 3 levels, Medium 4, High 5; every other field
    /// is the spec default.
    pub fn for_tier(tier: super::tier::QualityTier) -> Self {
        let mut params = Self::spec_defaults();
        params.levels = match tier {
            super::tier::QualityTier::Low => 3,
            super::tier::QualityTier::Medium => 4,
            super::tier::QualityTier::High => 5,
        };
        params
    }
}

/// Prefilter fragment shader: soft-knee threshold + 13-tap downsample
/// from the full-res scene into the half-res `down[0]` target.
///
/// Kernel: Jimenez 2014 ("Next Generation Post Processing in Call of
/// Duty") — center 0.125, inner diagonals 0.125, axes 0.0625, outer
/// diagonals 0.03125. Rational knee only, no `exp`. Separate
/// `texture2D` + `sampler` (the file's convention, naga-compatible).
pub const BLOOM_PREFILTER_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D hdr_tex;
layout(set = 0, binding = 1) uniform sampler hdr_sampler;
layout(push_constant) uniform PushConstants {
    float threshold;
    float knee;
    vec2 texel;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
float knee_soft(float x, float t, float k) { float u = x - t + k; return (u <= 0.0) ? 0.0 : (u < 2.0*k ? u*u/(4.0*k+1e-6) : x - t); }
void main() {
    vec2 t = pc.texel;
    vec3 acc = texture(sampler2D(hdr_tex, hdr_sampler), v_uv).rgb * 0.125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(1.0, 1.0)).rgb * 0.125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(-1.0, 1.0)).rgb * 0.125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(1.0, -1.0)).rgb * 0.125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(-1.0, -1.0)).rgb * 0.125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(2.0, 0.0)).rgb * 0.0625;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(-2.0, 0.0)).rgb * 0.0625;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(0.0, 2.0)).rgb * 0.0625;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(0.0, -2.0)).rgb * 0.0625;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(2.0, 2.0)).rgb * 0.03125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(-2.0, 2.0)).rgb * 0.03125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(2.0, -2.0)).rgb * 0.03125;
    acc += texture(sampler2D(hdr_tex, hdr_sampler), v_uv + t * vec2(-2.0, -2.0)).rgb * 0.03125;
    vec3 bloom = vec3(
        knee_soft(acc.r, pc.threshold, pc.knee),
        knee_soft(acc.g, pc.threshold, pc.knee),
        knee_soft(acc.b, pc.threshold, pc.knee));
    f_color = vec4(bloom, 1.0);
}";

/// Downsample fragment shader: the same Jimenez 13-tap kernel as the
/// prefilter, sampling its own pyramid level into the next (push
/// `texel` is the source-level texel size).
pub const BLOOM_DOWN_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D bloom_tex;
layout(set = 0, binding = 1) uniform sampler bloom_sampler;
layout(push_constant) uniform PushConstants {
    vec2 texel;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
void main() {
    vec2 t = pc.texel;
    vec3 acc = texture(sampler2D(bloom_tex, bloom_sampler), v_uv).rgb * 0.125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(1.0, 1.0)).rgb * 0.125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(-1.0, 1.0)).rgb * 0.125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(1.0, -1.0)).rgb * 0.125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(-1.0, -1.0)).rgb * 0.125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(2.0, 0.0)).rgb * 0.0625;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(-2.0, 0.0)).rgb * 0.0625;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(0.0, 2.0)).rgb * 0.0625;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(0.0, -2.0)).rgb * 0.0625;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(2.0, 2.0)).rgb * 0.03125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(-2.0, 2.0)).rgb * 0.03125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(2.0, -2.0)).rgb * 0.03125;
    acc += texture(sampler2D(bloom_tex, bloom_sampler), v_uv + t * vec2(-2.0, -2.0)).rgb * 0.03125;
    f_color = vec4(acc, 1.0);
}";

/// Upsample fragment shader: 3x3 tent of the coarser up level (corners
/// 1/16, edges 2/16, center 4/16) times push `weight`, plus the
/// same-level down image into a fresh target.
pub const BLOOM_UP_FRAG: &str = r"#version 450
layout(set = 0, binding = 0) uniform texture2D coarse_tex;
layout(set = 0, binding = 1) uniform sampler coarse_sampler;
layout(set = 0, binding = 2) uniform texture2D fine_tex;
layout(set = 0, binding = 3) uniform sampler fine_sampler;
layout(push_constant) uniform PushConstants {
    float weight;
} pc;
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 f_color;
void main() {
    vec2 t = 1.0 / vec2(textureSize(sampler2D(coarse_tex, coarse_sampler), 0));
    vec3 acc = texture(sampler2D(coarse_tex, coarse_sampler), v_uv).rgb * 0.25;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(-1.0, 0.0)).rgb * 0.125;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(1.0, 0.0)).rgb * 0.125;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(0.0, -1.0)).rgb * 0.125;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(0.0, 1.0)).rgb * 0.125;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(-1.0, -1.0)).rgb * 0.0625;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(1.0, -1.0)).rgb * 0.0625;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(-1.0, 1.0)).rgb * 0.0625;
    acc += texture(sampler2D(coarse_tex, coarse_sampler), v_uv + t * vec2(1.0, 1.0)).rgb * 0.0625;
    vec3 fine = texture(sampler2D(fine_tex, fine_sampler), v_uv).rgb;
    f_color = vec4(acc * pc.weight + fine, 1.0);
}";

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
        compile_glsl_to_spirv(ShaderKind::Fragment, &resolve_frag_bloom())
            .expect("bloom resolve fragment shader must compile");
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

    #[test]
    fn mip_bloom_shaders_compile_under_naga() {
        compile_glsl_to_spirv(ShaderKind::Fragment, BLOOM_PREFILTER_FRAG)
            .expect("bloom prefilter fragment shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, BLOOM_DOWN_FRAG)
            .expect("bloom downsample fragment shader must compile");
        compile_glsl_to_spirv(ShaderKind::Fragment, BLOOM_UP_FRAG)
            .expect("bloom upsample fragment shader must compile");
    }

    #[test]
    fn mip_bloom_kernel_literals_match_cpu_mirrors() {
        // Every Jimenez weight literal must appear in both 13-tap
        // sources (the ACES constant-pin precedent).
        for (_, w) in jimenez13_offsets() {
            let literal = format!("{w}");
            assert!(
                BLOOM_PREFILTER_FRAG.contains(&literal),
                "prefilter lost Jimenez weight {literal}"
            );
            assert!(
                BLOOM_DOWN_FRAG.contains(&literal),
                "downsample lost Jimenez weight {literal}"
            );
        }
        let total: f32 = jimenez13_offsets().iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-6, "kernel must sum to 1: {total}");
        // Every tent weight literal must appear in the upsample source.
        for w in tent9_weights() {
            let literal = format!("{w}");
            assert!(
                BLOOM_UP_FRAG.contains(&literal),
                "upsample lost tent weight {literal}"
            );
        }
        let tent_total: f64 = tent9_weights().iter().sum();
        assert!(
            (tent_total - 1.0).abs() < 1e-12,
            "tent must sum to 1: {tent_total}"
        );
    }

    #[test]
    fn soft_knee_mirror_spot_checks() {
        // Below the knee foot: fully suppressed.
        assert_eq!(soft_knee(0.5, 1.0, 0.5), 0.0);
        // At threshold the quadratic gives k/4 (formula pin, not a
        // hard-coded decimal; 1e-6 tolerance for the divide-by-zero
        // guard shared with the GLSL).
        for (t, k) in [(1.0, 0.5), (2.0, 0.25), (0.0, 1.0)] {
            assert!(
                (soft_knee(t, t, k) - k / 4.0).abs() < 1e-6,
                "knee(t, t, k) must equal k/4 for t={t} k={k}"
            );
        }
        // Above the knee head: linear x - t.
        for (x, t, k) in [(3.0, 1.0, 0.5), (5.0, 2.0, 0.25), (1.5, 0.0, 0.5)] {
            assert_eq!(soft_knee(x, t, k), x - t, "linear tail for x={x}");
        }
        // The GLSL source carries the same rational expression.
        assert!(
            BLOOM_PREFILTER_FRAG.contains("knee_soft"),
            "prefilter lost the knee function"
        );
    }

    #[test]
    fn mip_bloom_params_defaults_tiers_and_rejection() {
        let d = MipBloomParams::spec_defaults();
        assert_eq!((d.threshold, d.knee, d.intensity), (1.0, 0.5, 0.85));
        assert_eq!(d.levels, 4);
        assert_eq!(d.level_weights, [1.0, 0.9, 0.8, 0.7, 0.6]);
        // Tier presets only move the level count.
        use super::super::tier::QualityTier;
        for (tier, levels) in [
            (QualityTier::Low, 3),
            (QualityTier::Medium, 4),
            (QualityTier::High, 5),
        ] {
            let p = MipBloomParams::for_tier(tier);
            assert_eq!(p.levels, levels, "{tier:?}");
            assert_eq!(
                (p.threshold, p.knee, p.intensity, p.level_weights),
                (d.threshold, d.knee, d.intensity, d.level_weights),
                "{tier:?} keeps spec defaults otherwise"
            );
        }
        // Validated constructor: boundaries pass, everything else fails.
        let good_weights = [1.0, 0.9, 0.8, 0.7, 0.6];
        assert!(MipBloomParams::new(1.0, 0.5, 0.85, 4, good_weights).is_some());
        assert!(MipBloomParams::new(0.0, 0.0, 0.0, 2, [0.0; 5]).is_some());
        assert!(MipBloomParams::new(8.0, 2.0, 2.0, 5, good_weights).is_some());
        for bad in [
            MipBloomParams::new(-0.1, 0.5, 0.85, 4, good_weights),
            MipBloomParams::new(8.1, 0.5, 0.85, 4, good_weights),
            MipBloomParams::new(f32::NAN, 0.5, 0.85, 4, good_weights),
            MipBloomParams::new(1.0, -0.1, 0.85, 4, good_weights),
            MipBloomParams::new(1.0, 2.1, 0.85, 4, good_weights),
            MipBloomParams::new(1.0, f32::INFINITY, 0.85, 4, good_weights),
            MipBloomParams::new(1.0, 0.5, 2.1, 4, good_weights),
            MipBloomParams::new(1.0, 0.5, f32::NAN, 4, good_weights),
            MipBloomParams::new(1.0, 0.5, 0.85, 1, good_weights),
            MipBloomParams::new(1.0, 0.5, 0.85, 6, good_weights),
            MipBloomParams::new(1.0, 0.5, 0.85, 0, good_weights),
            MipBloomParams::new(1.0, 0.5, 0.85, 4, [1.0, -0.1, 0.6, 0.4, 0.3]),
            MipBloomParams::new(1.0, 0.5, 0.85, 4, [1.0, f32::NAN, 0.6, 0.4, 0.3]),
            MipBloomParams::new(1.0, 0.5, 0.85, 4, [1.0, 0.8, 0.6, 0.4, f32::INFINITY]),
        ] {
            assert!(bad.is_none(), "constructor must reject {bad:?}");
        }
    }
}
