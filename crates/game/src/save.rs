//! Autosave trigger policy (ADR-004): when saves fire.
//!
//! Pure, headless, caller-fed — no wall-clock, no fs. The shell owns the
//! composition: a fired reason → capture `ShipSnapshot` + metadata →
//! `AutosaveRing::write` → surface the indicator.

use std::collections::VecDeque;

/// Default periodic interval (UX decision 2026-09-17).
pub const DEFAULT_INTERVAL_S: f64 = 120.0;
/// Interval clamp floor.
pub const MIN_INTERVAL_S: f64 = 30.0;
/// Interval clamp ceiling.
pub const MAX_INTERVAL_S: f64 = 600.0;
/// Bounded evidence log capacity.
const LOG_CAPACITY: usize = 64;

/// Why an autosave fired (ADR-004 trigger set + periodic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveReason {
    /// Confirmed active-frame transition.
    FrameTransition,
    /// Completed SOI handoff (`HandoffEvent::Entered`).
    SoiHandoff,
    /// Automated fly-to committed.
    FlyToStart,
    /// Automated fly-to reached its target.
    FlyToComplete,
    /// Clean suspend/quit.
    Quit,
    /// User-configurable periodic interval elapsed.
    Periodic,
}

impl SaveReason {
    /// Stable display name for the indicator/event log.
    pub fn name(self) -> &'static str {
        match self {
            SaveReason::FrameTransition => "frame-transition",
            SaveReason::SoiHandoff => "soi-handoff",
            SaveReason::FlyToStart => "fly-to-start",
            SaveReason::FlyToComplete => "fly-to-complete",
            SaveReason::Quit => "quit",
            SaveReason::Periodic => "periodic",
        }
    }
}

/// One fired trigger, for the evidence log.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaveEvent {
    /// Which trigger fired.
    pub reason: SaveReason,
    /// Simulation time when it fired.
    pub sim_time_s: f64,
}

/// Clamp a configured interval to the UX-approved range; non-finite
/// input falls back to the default (never panics on bad config).
pub fn clamp_interval(interval_s: f64) -> f64 {
    if !interval_s.is_finite() {
        return DEFAULT_INTERVAL_S;
    }
    interval_s.clamp(MIN_INTERVAL_S, MAX_INTERVAL_S)
}

/// Trigger manager: five event triggers + a periodic accumulator fed by
/// the caller's real-time dt. Event triggers always fire; `tick` fires
/// `Periodic` each time the accumulator crosses the interval.
pub struct Autosave {
    interval_s: f64,
    acc_s: f64,
    playtime_s: f64,
    log: VecDeque<SaveEvent>,
}

impl Autosave {
    /// Manager with the configured interval (clamped to 30–600 s).
    pub fn new(interval_s: f64) -> Self {
        Self {
            interval_s: clamp_interval(interval_s),
            acc_s: 0.0,
            playtime_s: 0.0,
            log: VecDeque::new(),
        }
    }

    /// Effective interval after clamping.
    pub fn interval_s(&self) -> f64 {
        self.interval_s
    }

    /// Accumulated session playtime (real seconds, caller-fed).
    pub fn playtime_s(&self) -> f64 {
        self.playtime_s
    }

    /// Bounded evidence log, oldest first.
    pub fn events(&self) -> impl Iterator<Item = SaveEvent> + '_ {
        self.log.iter().copied()
    }

    fn fire(&mut self, reason: SaveReason, sim_time_s: f64) -> SaveReason {
        if self.log.len() == LOG_CAPACITY {
            self.log.pop_front();
        }
        self.log.push_back(SaveEvent { reason, sim_time_s });
        reason
    }

    /// Confirmed active-frame transition → save.
    pub fn on_frame_transition(&mut self, sim_time_s: f64) -> SaveReason {
        self.fire(SaveReason::FrameTransition, sim_time_s)
    }

    /// Completed SOI handoff → save.
    pub fn on_soi_handoff(&mut self, sim_time_s: f64) -> SaveReason {
        self.fire(SaveReason::SoiHandoff, sim_time_s)
    }

    /// Fly-to committed → save.
    pub fn on_fly_to_start(&mut self, sim_time_s: f64) -> SaveReason {
        self.fire(SaveReason::FlyToStart, sim_time_s)
    }

    /// Fly-to reached its target → save.
    pub fn on_fly_to_complete(&mut self, sim_time_s: f64) -> SaveReason {
        self.fire(SaveReason::FlyToComplete, sim_time_s)
    }

    /// Clean suspend/quit → save.
    pub fn on_quit(&mut self, sim_time_s: f64) -> SaveReason {
        self.fire(SaveReason::Quit, sim_time_s)
    }

    /// Feed real-time dt (seconds). Returns `Periodic` when the
    /// accumulator crosses the configured interval (and resets it).
    pub fn tick(&mut self, dt_s: f64, sim_time_s: f64) -> Option<SaveReason> {
        if !(dt_s.is_finite() && dt_s > 0.0) {
            return None;
        }
        self.acc_s += dt_s;
        self.playtime_s += dt_s;
        if self.acc_s >= self.interval_s {
            self.acc_s = 0.0;
            Some(self.fire(SaveReason::Periodic, sim_time_s))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_clamps_to_ux_range() {
        assert_eq!(clamp_interval(120.0), 120.0);
        assert_eq!(clamp_interval(1.0), MIN_INTERVAL_S);
        assert_eq!(clamp_interval(10_000.0), MAX_INTERVAL_S);
        assert_eq!(clamp_interval(f64::NAN), DEFAULT_INTERVAL_S);
        assert_eq!(Autosave::new(1.0).interval_s(), MIN_INTERVAL_S);
    }

    #[test]
    fn every_adr004_trigger_fires_and_logs() {
        let mut autosave = Autosave::new(DEFAULT_INTERVAL_S);
        let fired = [
            autosave.on_frame_transition(1.0),
            autosave.on_soi_handoff(2.0),
            autosave.on_fly_to_start(3.0),
            autosave.on_fly_to_complete(4.0),
            autosave.on_quit(5.0),
            autosave.tick(DEFAULT_INTERVAL_S, 6.0).expect("periodic"),
        ];
        assert_eq!(
            fired,
            [
                SaveReason::FrameTransition,
                SaveReason::SoiHandoff,
                SaveReason::FlyToStart,
                SaveReason::FlyToComplete,
                SaveReason::Quit,
                SaveReason::Periodic,
            ]
        );
        let log: Vec<_> = autosave.events().collect();
        assert_eq!(log.len(), 6);
        assert_eq!(log[5].sim_time_s, 6.0);
    }

    #[test]
    fn periodic_respects_interval_and_bad_dt_is_ignored() {
        let mut autosave = Autosave::new(MIN_INTERVAL_S);
        assert_eq!(autosave.tick(MIN_INTERVAL_S - 1.0, 0.0), None);
        assert_eq!(autosave.tick(f64::NAN, 0.0), None);
        assert_eq!(autosave.tick(-5.0, 0.0), None);
        assert_eq!(autosave.tick(1.0, 1.0), Some(SaveReason::Periodic));
        assert!((autosave.playtime_s() - MIN_INTERVAL_S).abs() < 1e-9);
        // Accumulator reset: another full interval is needed.
        assert_eq!(autosave.tick(1.0, 2.0), None);
    }

    #[test]
    fn log_is_bounded() {
        let mut autosave = Autosave::new(MIN_INTERVAL_S);
        for i in 0..(LOG_CAPACITY + 10) {
            autosave.on_quit(i as f64);
        }
        let log: Vec<_> = autosave.events().collect();
        assert_eq!(log.len(), LOG_CAPACITY);
        assert_eq!(log[0].sim_time_s, 10.0, "oldest dropped first");
    }
}
