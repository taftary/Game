//! Timed interplanetary transit (UMAP-018/019): selection → offer →
//! countdown → commit, cancellable any tick before the commit.
//!
//! Pure + headless: the viewer advances [`Transit::tick`] on the fixed
//! 20 Hz sim step and fires `journey::EnterOrbit` on [`Transit::ready`];
//! cancelling drops the struct (explicit [`Transit::cancel`]). Gameplay
//! rule (`docs/game/gameplay.md`): interplanetary transit is a timed
//! action with fuel/energy cost, cancellable before commit. The cost is
//! a **deferred hook** ([`TransitCost`]): computed from the target's
//! orbit, displayed in the offer UI, never deducted — the resource model
//! (and any deduction) belongs to the colonies milestone (M4).
//!
//! OQ-6 answered: one hop class (interplanetary) at a fixed
//! [`TRANSIT_TICKS`]. Interstellar timing lands with the M4 fuel model.
//!
//! ```
//! use game::transit::{Transit, plan_cost};
//!
//! let cost = plan_cost(2.5);
//! assert!(cost.fuel > 0.0 && cost.energy > 0.0);
//! let mut transit = Transit::begin(0, 1);
//! assert!(!transit.ready());
//! for _ in 0..60 {
//!     transit.tick();
//! }
//! assert!(transit.ready());
//! assert_eq!(transit.progress(), 1.0);
//! ```

/// Fixed sim step in seconds (20 Hz, per `docs/techstack/simulation.md`).
pub const SIM_DT_SECS: f32 = 0.05;

/// Interplanetary transit duration in sim ticks: 60 ticks = 3.0 s.
/// Fixed (OQ-6) — a placeholder feel pending M4/M7 gameplay tuning, but
/// a constant the tuning changes, never per-hop improvisation.
pub const TRANSIT_TICKS: u64 = 60;

/// Deferred transit cost (UMAP-019): fuel + energy for the hop,
/// computed from the target orbit, displayed, never deducted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitCost {
    pub fuel: f32,
    pub energy: f32,
}

/// Cost hook for a target at `orbit_au`: farther targets cost more.
/// Linear placeholder pending the M4 fuel model — the shape (pure
/// function of the descriptor) is what M4 consumes.
pub fn plan_cost(orbit_au: f64) -> TransitCost {
    let au = orbit_au.max(0.0) as f32;
    TransitCost {
        fuel: 4.0 + 2.0 * au,
        energy: 2.0 + 1.0 * au,
    }
}

/// One underway transit: target + elapsed ticks. Commit (the caller's
/// `journey::EnterOrbit`) is legal only at [`Transit::ready`]; before
/// that [`Transit::cancel`] drops it with no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transit {
    star_index: u32,
    planet_index: u32,
    elapsed: u64,
}

impl Transit {
    /// Begin the countdown to (`star_index`, `planet_index`).
    pub fn begin(star_index: u32, planet_index: u32) -> Self {
        Self {
            star_index,
            planet_index,
            elapsed: 0,
        }
    }

    /// Target star (the system the arrival lands in).
    pub fn star_index(self) -> u32 {
        self.star_index
    }

    /// Target planet (the orbit arrival binds to).
    pub fn planet_index(self) -> u32 {
        self.planet_index
    }

    /// Advance one sim tick; saturates at [`TRANSIT_TICKS`] (no
    /// overrun — a stalled commit still reads exactly ready).
    pub fn tick(&mut self) {
        self.elapsed = (self.elapsed + 1).min(TRANSIT_TICKS);
    }

    /// Ticks left (0 once ready).
    pub fn remaining_ticks(self) -> u64 {
        TRANSIT_TICKS - self.elapsed
    }

    /// Countdown progress 0.0 → 1.0 (drives the viewer progress bar).
    pub fn progress(self) -> f32 {
        self.elapsed as f32 / TRANSIT_TICKS as f32
    }

    /// Commit-legal: the full duration elapsed.
    pub fn ready(self) -> bool {
        self.elapsed >= TRANSIT_TICKS
    }

    /// Cancel before commit: consumes the transit, no effect.
    /// (`Drop` would do the same; the explicit call documents intent at
    /// the cancel site.)
    pub fn cancel(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_reaches_ready_exactly_at_duration() {
        let mut transit = Transit::begin(2, 1);
        assert!(!transit.ready());
        assert_eq!(transit.remaining_ticks(), TRANSIT_TICKS);
        for _ in 0..TRANSIT_TICKS {
            assert!(!transit.ready());
            transit.tick();
        }
        assert!(transit.ready());
        assert_eq!(transit.remaining_ticks(), 0);
        assert_eq!(transit.progress(), 1.0);
    }

    #[test]
    fn ticks_saturate_without_overrun() {
        let mut transit = Transit::begin(0, 0);
        for _ in 0..TRANSIT_TICKS + 10 {
            transit.tick();
        }
        assert!(transit.ready());
        assert_eq!(transit.progress(), 1.0);
    }

    #[test]
    fn cost_grows_with_orbit() {
        let near = plan_cost(0.5);
        let far = plan_cost(8.0);
        assert!(far.fuel > near.fuel && far.energy > near.energy);
        assert_eq!(
            plan_cost(1.0),
            TransitCost {
                fuel: 6.0,
                energy: 3.0
            }
        );
    }

    #[test]
    fn targets_round_trip() {
        let transit = Transit::begin(4, 2);
        assert_eq!((transit.star_index(), transit.planet_index()), (4, 2));
    }
}
