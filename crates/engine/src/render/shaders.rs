//! Planet shaders: GLSL sources compiled to SPIR-V by `naga`.
//!
//! Stack rule (`docs/techstack/stack.md`): shaders are authored GLSL and
//! compiled by `naga` (`glsl-in` → `spv-out`) — no external `shaderc`, no
//! WGSL in the runtime path. [`compile_glsl_to_spirv`] is pure and
//! headless-tested; only the `vulkano::shader::ShaderModule` built from
//! its output needs a GPU `Device` (done in `game_tools`).

use std::fmt;

use naga::front::glsl::{Frontend, Options as GlslOptions};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{ShaderStage, back::spv};

/// Which pipeline stage a GLSL source targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderKind {
    Vertex,
    Fragment,
}

impl ShaderKind {
    fn stage(self) -> ShaderStage {
        match self {
            Self::Vertex => ShaderStage::Vertex,
            Self::Fragment => ShaderStage::Fragment,
        }
    }
}

/// Planet vertex shader: MVP push constants, passes world normal + tint.
pub const PLANET_VERT: &str = r"#version 450

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in float tint;

layout(push_constant) uniform PushConstants {
    mat4 mvp;
} pc;

layout(location = 0) out vec3 v_normal;
layout(location = 1) out float v_tint;

void main() {
    gl_Position = pc.mvp * vec4(position, 1.0);
    v_normal = normal;
    v_tint = tint;
}
";

/// Planet fragment shader: flat-shaded base (hex slate-blue, pentagon
/// gold) lit by a fixed sun direction + ambient term.
pub const PLANET_FRAG: &str = r"#version 450

layout(location = 0) in vec3 v_normal;
layout(location = 1) in float v_tint;

layout(location = 0) out vec4 f_color;

void main() {
    vec3 n = normalize(v_normal);
    vec3 sun = normalize(vec3(0.5, 0.8, 0.6));
    float diffuse = max(dot(n, sun), 0.0);
    vec3 hex_color = vec3(0.25, 0.45, 0.75);
    vec3 pent_color = vec3(1.0, 0.8, 0.2);
    vec3 base = mix(hex_color, pent_color, v_tint);
    f_color = vec4(base * (0.25 + 0.75 * diffuse), 1.0);
}
";

/// GLSL→SPIR-V compilation failure (parse, validation, or backend).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderCompileError(String);

impl fmt::Display for ShaderCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shader compile failed: {}", self.0)
    }
}

impl std::error::Error for ShaderCompileError {}

/// Compiles GLSL `source` for `kind` to SPIR-V words via `naga`.
///
/// Returns `Err` (never panics) on invalid input. Output targets
/// SPIR-V 1.0 — under the Vulkan 1.1 floor ceiling (≤ 1.3).
pub fn compile_glsl_to_spirv(
    kind: ShaderKind,
    source: &str,
) -> Result<Vec<u32>, ShaderCompileError> {
    let stage = kind.stage();
    let module = Frontend::default()
        .parse(&GlslOptions::from(stage), source)
        .map_err(|errors| ShaderCompileError(format!("{kind:?} parse failed: {errors}")))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| ShaderCompileError(format!("{kind:?} invalid: {error}")))?;
    spv::write_vec(
        &module,
        &info,
        &spv::Options::default(),
        Some(&spv::PipelineOptions {
            shader_stage: stage,
            entry_point: "main".to_owned(),
        }),
    )
    .map_err(|error| ShaderCompileError(format!("{kind:?} backend failed: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SPIR-V magic number (spec §2.3).
    const SPIRV_MAGIC: u32 = 0x0723_0203;
    /// Vulkan 1.1 accepts SPIR-V up to 1.3 — our floor ceiling.
    const SPIRV_1_3_VERSION: u32 = 0x0001_0300;

    fn assert_valid_spirv(words: &[u32]) {
        assert!(!words.is_empty());
        assert_eq!(words[0], SPIRV_MAGIC);
        assert!(
            words[1] <= SPIRV_1_3_VERSION,
            "SPIR-V version {:#x} exceeds the Vulkan 1.1 ceiling",
            words[1]
        );
    }

    #[test]
    fn planet_shaders_compile_to_floor_safe_spirv() {
        let vert = compile_glsl_to_spirv(ShaderKind::Vertex, PLANET_VERT)
            .expect("planet vertex shader must compile");
        let frag = compile_glsl_to_spirv(ShaderKind::Fragment, PLANET_FRAG)
            .expect("planet fragment shader must compile");
        assert_valid_spirv(&vert);
        assert_valid_spirv(&frag);
    }

    #[test]
    fn invalid_glsl_returns_error_without_panicking() {
        let result = compile_glsl_to_spirv(ShaderKind::Vertex, "this is not glsl %%");
        assert!(result.is_err(), "expected Err, got {result:?}");
        let empty = compile_glsl_to_spirv(ShaderKind::Fragment, "");
        assert!(empty.is_err());
    }
}
