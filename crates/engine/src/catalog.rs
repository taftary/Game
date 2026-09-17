//! `engine::catalog` — star-catalog streaming (spec §4, ADR-017).
//!
//! Gaia DR3's ~1.8 billion stars cannot be memory-resident, so catalog
//! access streams on demand through a HEALPix spatial index keyed by
//! camera position and active frame, with a deterministic procedural
//! fallback covering slow/absent tiles. This module is pure +
//! headless-testable: decode and index math never touch the GPU, the
//! clock, or the network — the threaded loader lands in Phase 3 behind
//! that same boundary.
//!
//! Submodules land phase by phase (`plan.md`): [`healpix`] (Phase 1)
//! implements the NESTED index; tile format, cache, scheduler,
//! fallback, and identity follow.

pub mod cache;
pub mod fallback;
pub mod format;
pub mod frustum;
pub mod healpix;
pub mod ids;
pub mod io;
pub mod metrics;
pub mod scheduler;
