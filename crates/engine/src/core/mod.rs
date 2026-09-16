//! `engine::core` — shared kernel: seeded RNG, quantization, and later
//! math, units, time, and error types (see
//! `docs/techstack/architecture.md`).
//!
//! Determinism contract ([`risks/`](../../../../docs/risks/README.md)
//! #2, `docs/game/universe.md` rules): everything in `core` that feeds
//! generation is pure — same inputs → same outputs on every platform.
//! Concretely: seeded RNG only (no wall-clock, no thread ids),
//! integer-driven derivations, and quantization before any hash, store,
//! or cross-platform comparison.

pub mod quant;
pub mod rng;

pub use quant::{quantize_f32, quantize_f64};
pub use rng::SeededRng;
