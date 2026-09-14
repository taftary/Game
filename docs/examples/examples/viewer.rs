//! Vulkan debug viewer. Target: windowed scene inspection via `winit` and
//! `vulkano`. Current: stub that validates the `gpu`-gated build path.
//!
//! This example requires the `gpu` feature and is excluded from headless CI:
//! `cargo run --example viewer --features gpu` on a Vulkan-capable display.

fn main() {
    println!("viewer: Vulkan integration is Planned; gpu gate compiles.");
}
