# Stack (locked for v1 unless an ADR overturns it)

Main dependencies (declared in workspace `Cargo.toml`, consumed by `crates/engine`):

| Crate | Version (v1 lock) | Role |
|---|---|---|
| [`vulkano`](https://crates.io/crates/vulkano) | `0.35` (default features: `macros`, `x11`) | Safe Vulkan bindings: Instance → Device → Swapchain → command buffers, graphics/compute pipelines |
| [`winit`](https://crates.io/crates/winit) | `0.30` stable (not `0.31-beta`) | Cross-platform windowing; provides `raw-window-handle 0.6` handles to `vulkano` Surfaces |
| [`naga`](https://crates.io/crates/naga) | `30` (`glsl-in`, `spv-out`, `wgsl-in`) | GLSL → SPIR-V shader compilation at runtime (pure Rust, no external `shaderc`) |
| [`fontdue`](https://crates.io/crates/fontdue) | `0.9` | Font rasterization for UI text (pure Rust, no system font stack) |
| [`glam`](https://crates.io/crates/glam) | `0.33` | SIMD-accelerated math (`Vec2/3/4`, `Mat4`, `Quat`) |

- **Language:** Rust, edition 2024, 1.85+.
- **Renderer:** custom engine on `vulkano` + Vulkan directly (no Bevy, no `wgpu`, no Unity/Unreal/Godot).
- **Graphics API:** Vulkan only in v1 code path.
  Vulkan on Windows / Linux / Android (phones, tablets, desktop);
  macOS / iOS via MoltenVK portability (`vulkan-portability` enumeration where required).
  No desktop-only GPU features in the critical path; every shader ships as SPIR-V
  compiled by `naga` from GLSL sources. Vulkan validation layers on in dev/debug builds.
- **Windowing:** `winit` `0.30` (ADR to change). Must stay on the `raw-window-handle 0.6`
  line compatible with `vulkano 0.35`; upgrading `winit` past that breaks Surface creation
  until `vulkano` catches up.
- **Shaders:** GLSL sources in-repo, compiled at runtime (dev) / cook time (release) by
  `naga` (`glsl-in` → `spv-out`). `wgsl-in` enabled for tooling/debugging only.
- **Text:** `fontdue` rasterizes `assets/fonts/` glyphs to atlases; no OS font dependency
  (required for consistent mobile rendering).
- **Math:** `glam` everywhere (ADR to replace). f64 game coordinates converted to
  `glam` f32 render coordinates at the floating-origin boundary (see [`../game/journey.md`](../game/journey.md)).
- **Procedural noise:** `noise` (default). Replace only if it fails cross-platform determinism tests (owner: M5, see [`../milestones/`](../milestones/)).
- **Serialization:** `serde` + `postcard` (default, `bincode` fallback) compact binary for saves (owner: ADR-004, see [`../decisions/`](../decisions/)).
- **UI:** custom immediate-or-retained UI on top of the `vulkano` swapchain (v1);
  text via `fontdue` atlases; no webview, no heavyweight retained framework on mobile path.
- **Audio:** `rodio` (default, `kira` fallback). Decided by ADR-006 (see [`../decisions/`](../decisions/)); audio is a stretch goal for the v1 vertical slice (M7).
- **No runtime GC language, no scripting VM in v1.** Data-driven via RON/TOML/JSON assets.

Why custom `vulkano` instead of Bevy / `wgpu`:

- Full control over Vulkan: explicit queues, swapchain, render passes, planet LOD, and mobile tiering.
- Smaller mobile binary and explicit GPU budget control; no abstraction-layer backend matrix.
- `naga` keeps the shader pipeline pure Rust (GLSL → SPIR-V, no C++ `shaderc` build dependency).
- Cost: we own swapchain recreation, synchronization, ECS/scheduling, asset pipeline,
  and editor tooling. `crates/tools` exists for exactly this. Apple support depends on
  MoltenVK quality on our reference devices (see [`../risks/`](../risks/)).
