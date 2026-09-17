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
