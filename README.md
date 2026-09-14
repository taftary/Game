# PlanetCrafter

Procedural colony universe. Start here: [`docs/`](docs/README.md) (game
overview with links to stack, gates, milestones),
[`plans/README.md`](plans/README.md) (feature workflow),
[`AGENTS.md`](AGENTS.md) (assistant entry point).

## Requirements

- Rust toolchain (edition 2024, Rust 1.87+) via [rustup](https://rustup.rs).
  `rust-toolchain.toml` pins stable + `rustfmt`/`clippy`.
- A Vulkan-capable GPU + driver. For validation layers in dev builds,
  install the [Vulkan SDK](https://vulkan.lunarg.com/) (not needed to
  build — only to run with layers enabled).
- Mobile targets (`aarch64-linux-android`, `aarch64-apple-ios`) are
  CI-checked; install them locally only if you touch platform code
  (`rustup target add <target>`). Full device deploy arrives with M6.

## Quickstart

Build, test, and run commands live in
[`docs/techstack/quality.md`](docs/techstack/quality.md). Workspace layout:
[`docs/techstack/architecture.md`](docs/techstack/architecture.md).
