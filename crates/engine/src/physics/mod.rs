//! `engine::physics` — cheapest-correct physics model per scale (spec §5,
//! ADR-018) plus the symplectic integrator all integrated regimes share.
//!
//! Pure math over `f64` (`glam`), headless-testable, deterministic — the
//! same purity bar as [`crate::frames`]. No GPU, no I/O, no `unsafe`.
//! This module owns *models*; the tick that steps them lands with
//! `time-compression`, ship control with `free-flight-navigation`, and
//! catalog-backed providers with `star-catalog-streaming`.
//!
//! Module map:
//!
//! - [`units`] — per-frame length/time/mass systems + exact local `G`.
//! - [`potential`] — Miyamoto-Nagai disk + NFW halo (galactic bulk
//!   kinematics) with analytic acceleration.
//! - [`integrator`] — velocity Verlet (+ Euler baseline for comparison).
//! - [`kepler`] — closed-form two-body solver (ship dominant case).
//! - [`ephemeris`] — background-body positions: provider trait +
//!   built-in Keplerian-elements table (full VSOP87/DE440 later).
//! - [`proper_motion`] — linear Gaia extrapolation + validity window.
//! - [`populations`] — seeded procedural-body orbit distributions.
//! - [`static_field`] — cosmic-web row: static by design.
//!
//! ```
//! use game_engine::physics::{FrameUnits, GalacticPotential};
//! use game_engine::frames::FrameId;
//!
//! let solar = FrameUnits::of(FrameId::SolarSystem);
//! // G in AU³/(M☉·day²): Gaussian constant squared, not exactly 1.
//! assert!((solar.g_local() - 2.959_122_082_86e-4).abs() / 2.959_122_082_86e-4 < 1e-9);
//! let mw = GalacticPotential::milky_way();
//! assert!(mw.circular_velocity_kms(8.0) > 200.0);
//! ```

pub mod ephemeris;
pub mod integrator;
pub mod kepler;
pub mod populations;
pub mod potential;
pub mod proper_motion;
pub mod static_field;
pub mod units;

pub use ephemeris::{
    EphemerisBody, EphemerisMode, EphemerisTable, KeplerianTable, Validity as EphemerisValidity,
};
pub use integrator::{State as IntegratorState, euler_step, orbital_energy, verlet_step};
pub use kepler::{Elements, elements_from_state, period, solve_kepler, state_from_elements};
pub use populations::{OrbitPriors, SampledOrbit, sample_orbit};
pub use potential::GalacticPotential;
pub use proper_motion::{GAIA_EPOCH_YR, PROPER_MOTION_SPAN_YR, ProperMotion, extrapolate};
pub use static_field::StaticDensityField;
pub use units::{DAY_S, FrameUnits, G_SI, GYR_S, KYR_S, M_EARTH_KG, M_SUN_KG, MYR_S, YEAR_S};
