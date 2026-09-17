//! `engine::handoff` — soft patched-conic SOI handoff (spec §3, ADR-014).
//!
//! Hard SOI switches kick velocity and flicker HUDs. Instead: blend
//! acceleration contributions across a 10% radial band around the
//! Laplace SOI radius with smoothstep weighting (no linear ramp), gate
//! candidacy on the 10⁻³ acceleration rule, transform state vectors
//! before the boundary-crossing physics step, and emit hysteresis
//! events for HUD (`navigation-hud`) and autosave (ADR-004).
//!
//! Position is C⁰ continuous and velocity ~C¹ by construction: the
//! transform is linear (exact up to fp); the "~" concedes patched
//! conics approximate N-body, not our math.
//!
//! ```
//! use game_engine::handoff::{handoff_eligible, hill_radius, laplace_soi_radius};
//!
//! // Earth–Sun: Laplace ≈ 924,000 km ≈ 0.62 × Hill (spec §3 note).
//! let soi = laplace_soi_radius(1.495_978_707e11, 5.972_168e24, 1.988_409_9e30);
//! let hill = hill_radius(1.495_978_707e11, 5.972_168e24, 1.988_409_9e30);
//! assert!((soi - 9.24e8).abs() / 9.24e8 < 0.01);
//! assert!((soi / hill - 0.62).abs() < 0.05);
//! assert!(handoff_eligible(5.9e-6, 1.0e-7));
//! ```

use crate::frames::BodyId;
use glam::DVec3;

/// Shared handoff/compression constant (PO decision, spec §10
/// addendum): a secondary frame becomes a handoff candidate once its
/// acceleration reaches this fraction of the primary's.
pub const HANDOFF_THRESHOLD: f64 = 1e-3;
/// Blend-band half-width as a fraction of `r_soi` (10% band: 1.05 → 0.95).
pub const BLEND_HALF_WIDTH: f64 = 0.05;
/// HUD "Approaching / Entering" threshold (spec §10): rising past this
/// blend weight fires `Approaching`.
pub const APPROACH_WEIGHT: f64 = 0.2;
/// Hysteresis floor: `Exited` fires only below this (gap kills repeats).
pub const EXIT_WEIGHT: f64 = 0.15;

/// Laplace SOI radius `a·(m/M)^(2/5)` — the handoff radius (spec §3).
/// Same length units in/out.
pub fn laplace_soi_radius(semi_major_axis: f64, m_secondary: f64, m_primary: f64) -> f64 {
    debug_assert!(semi_major_axis > 0.0 && m_secondary > 0.0 && m_primary > 0.0);
    semi_major_axis * (m_secondary / m_primary).powf(0.4)
}

/// Hill sphere radius `a·(m/3M)^(1/3)` — for **stability analysis**
/// (moon orbit limits, ring boundaries), NOT handoffs (spec §3 note).
/// Same length units in/out.
pub fn hill_radius(semi_major_axis: f64, m_secondary: f64, m_primary: f64) -> f64 {
    debug_assert!(semi_major_axis > 0.0 && m_secondary > 0.0 && m_primary > 0.0);
    semi_major_axis * (m_secondary / (3.0 * m_primary)).powf(1.0 / 3.0)
}

/// Handoff candidacy: secondary acceleration ≥ 10⁻³ of primary's.
pub fn handoff_eligible(a_primary: f64, a_secondary: f64) -> bool {
    a_secondary >= HANDOFF_THRESHOLD * a_primary
}

/// Smoothstep `t²(3−2t)` on clamped `t` — zero derivative at both edges
/// (no acceleration-derivative kink, ADR-014).
pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Radial blend band around one body's SOI radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendBand {
    /// Nominal Laplace SOI radius (caller length units).
    pub r_soi: f64,
}

impl BlendBand {
    /// Secondary-dominant weight at `distance` from the secondary center:
    /// 0 at/above 1.05 × r_soi, 1 at/below 0.95 × r_soi, smoothstep
    /// between. Outside the band the primary dominates; inside, the
    /// secondary does (spec §3).
    pub fn weight(&self, distance: f64) -> f64 {
        debug_assert!(self.r_soi > 0.0);
        let outer = (1.0 + BLEND_HALF_WIDTH) * self.r_soi;
        let inner = (1.0 - BLEND_HALF_WIDTH) * self.r_soi;
        smoothstep((outer - distance) / (outer - inner))
    }
}

/// Blend primary/secondary acceleration contributions by `weight`
/// (0 = primary only, 1 = secondary only). Edge-exact: the band edges
/// return the single-body accelerations bit-identically (no 1-ulp lerp
/// residue), matching the spec's inside/outside semantics.
pub fn blend_accelerations(a_primary: DVec3, a_secondary: DVec3, weight: f64) -> DVec3 {
    let w = weight.clamp(0.0, 1.0);
    if w <= 0.0 {
        a_primary
    } else if w >= 1.0 {
        a_secondary
    } else {
        a_primary + (a_secondary - a_primary) * w
    }
}

/// Transform a state vector from the primary frame to the secondary
/// frame, given the secondary center's state in the primary frame.
/// `unit_ratio` = secondary-meters-per-unit / primary-meters-per-unit.
/// Linear in both vectors: C⁰/~C¹-exact up to fp rounding. Inverse in
/// [`transform_to_primary`].
pub fn transform_to_secondary(
    pos_pri: DVec3,
    vel_pri: DVec3,
    body_pos_pri: DVec3,
    body_vel_pri: DVec3,
    unit_ratio: f64,
) -> (DVec3, DVec3) {
    (
        (pos_pri - body_pos_pri) / unit_ratio,
        (vel_pri - body_vel_pri) / unit_ratio,
    )
}

/// Inverse of [`transform_to_secondary`].
pub fn transform_to_primary(
    pos_sec: DVec3,
    vel_sec: DVec3,
    body_pos_pri: DVec3,
    body_vel_pri: DVec3,
    unit_ratio: f64,
) -> (DVec3, DVec3) {
    (
        pos_sec * unit_ratio + body_pos_pri,
        vel_sec * unit_ratio + body_vel_pri,
    )
}

/// Handoff lifecycle event: hysteresis-ordered, HUD-complete payload
/// (body identity + weight feed spec §10 indicators).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandoffEvent {
    /// Blend weight rose past [`APPROACH_WEIGHT`] — show
    /// "Approaching [body] SOI".
    Approaching {
        /// Secondary body being entered.
        body: BodyId,
        /// Weight at firing.
        weight: f64,
    },
    /// Weight reached 1 — completed handoff. The caller commits the
    /// `FrameChain` (`reason: BoundaryCrossing`) and fires the ADR-004
    /// autosave trigger.
    Entered {
        /// Secondary body entered.
        body: BodyId,
    },
    /// Weight fell below [`EXIT_WEIGHT`] after entry — clean exit, no
    /// repeat storm.
    Exited {
        /// Secondary body departed.
        body: BodyId,
    },
}

/// Hysteresis state machine over blend weights. Feed one weight per
/// physics step; drain [`HandoffMonitor::events`]. At most one event
/// per threshold crossing, in order, by construction.
#[derive(Clone, Debug)]
pub struct HandoffMonitor {
    body: BodyId,
    entered: bool,
    approached: bool,
    pending: Vec<HandoffEvent>,
}

impl HandoffMonitor {
    /// Watch entries into `body`'s SOI.
    pub fn new(body: BodyId) -> Self {
        Self {
            body,
            entered: false,
            approached: false,
            pending: Vec::new(),
        }
    }

    /// Feed the current blend weight (0 = primary-dominant, 1 = inside).
    pub fn update(&mut self, weight: f64) {
        let w = weight.clamp(0.0, 1.0);
        if !self.entered {
            if w >= 1.0 {
                // Jumped straight in (spawned inside): approach implied.
                if !self.approached {
                    self.pending.push(HandoffEvent::Approaching {
                        body: self.body,
                        weight: w,
                    });
                    self.approached = true;
                }
                self.pending.push(HandoffEvent::Entered { body: self.body });
                self.entered = true;
            } else if w >= APPROACH_WEIGHT && !self.approached {
                self.pending.push(HandoffEvent::Approaching {
                    body: self.body,
                    weight: w,
                });
                self.approached = true;
            }
        } else if w < EXIT_WEIGHT {
            self.pending.push(HandoffEvent::Exited { body: self.body });
            self.entered = false;
            self.approached = false;
        }
    }

    /// Drain fired events, oldest first.
    pub fn events(&mut self) -> Vec<HandoffEvent> {
        std::mem::take(&mut self.pending)
    }

    /// True between `Entered` and `Exited`.
    pub fn inside(&self) -> bool {
        self.entered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{BodyId, FrameChain, FrameId, FrameLink, TransitionReason};
    use crate::physics::{IntegratorState, M_EARTH_KG, M_SUN_KG, verlet_step};
    use glam::DQuat;

    const EARTH: BodyId = BodyId::EARTH;
    const AU_KM: f64 = 1.495_978_707e8;
    const MU_SUN_KM: f64 = 1.327_124_400_18e11; // μ☉ in km³/s²
    const MU_EARTH_KM: f64 = 3.986_004_418e5; // μ⊕ in km³/s²

    fn earth_soi_km() -> f64 {
        laplace_soi_radius(AU_KM, M_EARTH_KG, M_SUN_KG)
    }

    #[test]
    fn earth_sun_radii_match_reference_values() {
        // PO tolerance table: Laplace vs Hill regression. References:
        // Laplace ≈ 924,000 km; Laplace ≈ 0.62 × Hill for Earth–Sun
        // (spec §3 note, corrected 2026-09-17 — the spec text had the
        // ratio inverted; Hill exceeds Laplace at every planetary mass
        // ratio since Hill/Laplace = 0.693·(m/M)^(−1/15) > 1 for m < M).
        let soi = earth_soi_km();
        assert!((soi - 9.24e5).abs() / 9.24e5 < 0.01, "SOI = {soi} km");
        let hill = hill_radius(AU_KM, M_EARTH_KG, M_SUN_KG);
        assert!((hill - 1.496e6).abs() / 1.496e6 < 0.01, "Hill = {hill} km");
        let ratio = soi / hill;
        assert!((0.5..=0.7).contains(&ratio), "ratio = {ratio}");
    }

    #[test]
    fn eligibility_follows_the_shared_threshold() {
        assert!(handoff_eligible(5.9e-6, 6.0e-9)); // above 1e-3
        assert!(handoff_eligible(5.9e-6, 1.0e-7));
        assert!(!handoff_eligible(5.9e-6, 5.8e-9)); // below 1e-3
        assert!(!handoff_eligible(5.9e-6, 0.0));
    }

    #[test]
    fn smoothstep_has_zero_edge_derivatives() {
        assert_eq!(smoothstep(-0.5), 0.0);
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(1.5), 1.0);
        // Numerical derivatives at the edges vanish (no kink).
        let h = 1e-7;
        assert!((smoothstep(h) - smoothstep(0.0)).abs() / h < 1e-6);
        assert!((smoothstep(1.0) - smoothstep(1.0 - h)).abs() / h < 1e-6);
        // …while the middle actually blends.
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn band_weight_is_edge_exact() {
        let band = BlendBand { r_soi: 9.24e5 };
        assert_eq!(band.weight(9.24e5 * 1.05), 0.0);
        assert_eq!(band.weight(9.24e5 * 1.06), 0.0);
        assert_eq!(band.weight(9.24e5 * 0.95), 1.0);
        assert_eq!(band.weight(9.24e5 * 0.5), 1.0);
        let mid = band.weight(9.24e5);
        assert!(mid > 0.0 && mid < 1.0, "mid-band weight = {mid}");
    }

    #[test]
    fn blended_accel_is_continuous_at_band_edges() {
        // No kink BY CONSTRUCTION: w = 0/1 exactly at the edges, so the
        // blend equals the single-body accelerations there.
        let band = BlendBand { r_soi: 9.24e5 };
        let a_pri = DVec3::new(-5.9e-6, 1e-9, 0.0);
        let a_sec = DVec3::new(3.1e-7, -2e-8, 0.0);
        assert_eq!(
            blend_accelerations(a_pri, a_sec, band.weight(9.24e5 * 1.05)),
            a_pri
        );
        assert_eq!(
            blend_accelerations(a_pri, a_sec, band.weight(9.24e5 * 0.95)),
            a_sec
        );
        // Interior is a genuine mixture.
        let mid = blend_accelerations(a_pri, a_sec, band.weight(9.24e5));
        assert!(mid != a_pri && mid != a_sec);
    }

    #[test]
    fn state_transform_round_trips() {
        let pos = DVec3::new(1.5e8, 1e6, 0.0);
        let vel = DVec3::new(-2.0, 29.0, 0.5);
        let body_pos = DVec3::new(1.495978707e8, 0.0, 0.0);
        let body_vel = DVec3::new(0.0, 29.78, 0.0);
        let ratio = 1.0 / 1.495_978_707e8; // km → AU-ish secondary units
        let (ps, vs) = transform_to_secondary(pos, vel, body_pos, body_vel, ratio);
        let (pb, vb) = transform_to_primary(ps, vs, body_pos, body_vel, ratio);
        assert!((pb - pos).length() / pos.length() < 1e-12);
        assert!((vb - vel).length() / vel.length() < 1e-12);
    }

    #[test]
    fn monitor_orders_events_without_repeats() {
        let mut mon = HandoffMonitor::new(EARTH);
        // Approach with noise straddling 0.2: exactly one Approaching.
        for w in [0.0, 0.1, 0.19, 0.21, 0.19, 0.22, 0.18, 0.25, 0.21, 0.3] {
            mon.update(w);
        }
        assert_eq!(
            mon.events(),
            [HandoffEvent::Approaching {
                body: EARTH,
                weight: 0.21
            }]
        );
        // Full entry then noisy exit: Entered once, Exited once.
        for w in [
            0.5, 0.9, 1.0, 1.0, 0.9, 0.5, 0.2, 0.16, 0.14, 0.16, 0.1, 0.0,
        ] {
            mon.update(w);
        }
        assert_eq!(
            mon.events(),
            [
                HandoffEvent::Entered { body: EARTH },
                HandoffEvent::Exited { body: EARTH }
            ]
        );
        assert!(!mon.inside());
        // Re-entry works after exit.
        mon.update(1.0);
        let events = mon.events();
        assert_eq!(
            events,
            [
                HandoffEvent::Approaching {
                    body: EARTH,
                    weight: 1.0
                },
                HandoffEvent::Entered { body: EARTH }
            ]
        );
        assert!(mon.inside());
    }

    #[test]
    fn ship_crossing_earth_soi_is_continuous_and_commits() {
        // DoD 1: full pipeline on a radial Earth approach. Primary =
        // Sun point mass at origin (solar frame, km); secondary = Earth
        // at 1 AU (kinematic backdrop). Ship falls from 2.0e6 km to
        // inside the band on Verlet + blended acceleration.
        let earth_pos = DVec3::new(AU_KM, 0.0, 0.0);
        let earth_vel = DVec3::new(0.0, 29.78, 0.0);
        let au_per_km = 1.0 / 1.495_978_707e8;
        let sun_accel = |p: DVec3| -p * (MU_SUN_KM / p.length().powi(3));
        let earth_accel = |p: DVec3| {
            let d = p - earth_pos;
            -d * (MU_EARTH_KM / d.length().powi(3))
        };
        let band = BlendBand {
            r_soi: earth_soi_km(),
        };
        let blended = |p: DVec3| {
            let a_pri = sun_accel(p);
            let a_sec = earth_accel(p);
            let w = if handoff_eligible(a_pri.length(), a_sec.length()) {
                band.weight((p - earth_pos).length())
            } else {
                0.0
            };
            blend_accelerations(a_pri, a_sec, w)
        };
        let mut ship = IntegratorState {
            pos: earth_pos + DVec3::new(2.0e6, 0.0, 0.0),
            // Pure radial approach: Earth is a static backdrop here, so
            // no comoving offset — anything else flies off in +y.
            vel: DVec3::new(-2.0, 0.0, 0.0),
        };
        let mut mon = HandoffMonitor::new(EARTH);
        let dt = 100.0;
        let mut entered_at: Option<(DVec3, DVec3)> = None;
        for _ in 0..20_000 {
            // Monitor and dynamics consume the SAME gated weight.
            let a_pri = sun_accel(ship.pos);
            let a_sec = earth_accel(ship.pos);
            let d = (ship.pos - earth_pos).length();
            let w = if handoff_eligible(a_pri.length(), a_sec.length()) {
                band.weight(d)
            } else {
                0.0
            };
            mon.update(w);
            for e in mon.events() {
                if matches!(e, HandoffEvent::Entered { .. }) {
                    entered_at = Some((ship.pos, ship.vel));
                }
            }
            if entered_at.is_some() {
                break;
            }
            verlet_step(&mut ship, dt, &blended);
        }
        let (pos_pri, vel_pri) = entered_at.expect("ship never entered the SOI");
        // C⁰: the real chain commit converts the crossing state into the
        // planet frame (km, Earth-relative)…
        let planet_link = FrameLink {
            rotation: DQuat::IDENTITY,
            origin: earth_pos * au_per_km, // Earth center in AU
        };
        let mut chain = FrameChain::new(
            FrameId::SolarSystem,
            pos_pri * au_per_km,
            DQuat::IDENTITY,
            vec![
                FrameLink::identity(), // SolarSystem → StellarNeighborhood
                FrameLink::identity(),
                FrameLink::identity(),
                FrameLink::identity(),
            ],
        );
        let event = chain
            .commit_to_child(
                FrameId::Planetocentric(EARTH),
                planet_link,
                TransitionReason::BoundaryCrossing,
                0.0,
            )
            .expect("adjacent commit");
        assert_eq!(event.to, FrameId::Planetocentric(EARTH));
        // …matching the direct transform math within 1 m (documented
        // tolerance: fp rounding across AU↔km unit ratios)…
        let (ps, vs) = transform_to_secondary(pos_pri, vel_pri, earth_pos, earth_vel, 1.0);
        assert!((chain.position() - ps).length() < 1e-3, "C⁰ break");
        // …and velocity round-trips through the transform (~C¹).
        let (pb, vb) = transform_to_primary(ps, vs, earth_pos, earth_vel, 1.0);
        assert!((pb - pos_pri).length() < 1.0, "C⁰ break");
        assert!((vb - vel_pri).length() < 1e-9, "~C¹ break");
        // Post-handoff integration stays sane (no NaN/jump): 100 steps
        // on Earth-only gravity in planetocentric km.
        let mut s = IntegratorState { pos: ps, vel: vs };
        for _ in 0..100 {
            verlet_step(&mut s, dt, &|p| -p * (MU_EARTH_KM / p.length().powi(3)));
            assert!(s.pos.is_finite() && s.vel.is_finite());
        }
    }

    #[test]
    fn eligibility_holds_through_the_test_scenario() {
        // Guards the crossing test's premise: at 2e6 km from Earth the
        // secondary already exceeds 1e-3 of primary (so the test really
        // exercises the blend, not the w = 0 fallback).
        let earth_pos = DVec3::new(AU_KM, 0.0, 0.0);
        let ship = earth_pos + DVec3::new(2.0e6, 0.0, 0.0);
        let a_pri = MU_SUN_KM / ship.length_squared();
        let a_sec = MU_EARTH_KM / 2.0e6f64.powi(2);
        assert!(handoff_eligible(a_pri, a_sec));
        assert!(a_sec / a_pri > 1e-3);
    }
}
