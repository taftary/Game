//! In-app log console: bounded line history fed by transition
//! events (ADR-022: the old Transitions tab's event log lives here).

use game_engine::waypoints::{TransitionEvent, TransitionPhase};

/// Bounded console history (oldest lines age out).
pub const CONSOLE_CAPACITY: usize = 200;

/// Log console state: the transition-event feed plus free lines.
pub struct LogConsole {
    lines: Vec<String>,
    /// Transition events already drained into `lines`.
    fed_events: usize,
}

impl LogConsole {
    pub fn new() -> Self {
        LogConsole {
            lines: Vec::new(),
            fed_events: 0,
        }
    }

    /// Push one line, aging out the oldest past capacity.
    pub fn push(&mut self, line: String) {
        if self.lines.len() >= CONSOLE_CAPACITY {
            self.lines.remove(0);
        }
        self.lines.push(line);
    }

    /// Drain unseen events from `history` into lines. Returns the
    /// number of lines added. A shrink of `history` (log reset)
    /// re-arms the cursor instead of starving the feed.
    pub fn feed_transitions(&mut self, history: &[TransitionEvent]) -> usize {
        if history.len() < self.fed_events {
            self.fed_events = 0;
        }
        let mut added = 0;
        for event in history.iter().skip(self.fed_events) {
            self.push(format_event(*event));
            added += 1;
        }
        self.fed_events = history.len();
        added
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl Default for LogConsole {
    fn default() -> Self {
        LogConsole::new()
    }
}

pub fn format_event(event: TransitionEvent) -> String {
    let phase = match event.phase {
        TransitionPhase::Started => "started",
        TransitionPhase::Progress => "progress",
        TransitionPhase::Completed => "completed",
    };
    format!(
        "t={:.1}s {phase}: {} -> {} ({:.0}%)",
        event.sim_time_s,
        event.leg.from.name(),
        event.leg.to.name(),
        event.progress * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::waypoints::{WaypointId, WaypointLeg};

    fn sample_event(progress: f64) -> TransitionEvent {
        let leg = WaypointLeg::new(WaypointId::SolarSystem, WaypointId::Neighborhood).unwrap();
        TransitionEvent {
            leg,
            phase: TransitionPhase::Progress,
            progress,
            sim_time_s: 1.0,
        }
    }

    #[test]
    fn feed_drains_only_unseen_events() {
        let mut console = LogConsole::new();
        let history = vec![sample_event(0.0), sample_event(0.5)];
        assert_eq!(console.feed_transitions(&history), 2);
        assert_eq!(console.feed_transitions(&history), 0);
        assert_eq!(console.len(), 2);
        let longer = vec![sample_event(0.0), sample_event(0.5), sample_event(1.0)];
        assert_eq!(console.feed_transitions(&longer), 1);
        assert_eq!(console.len(), 3);
    }

    #[test]
    fn history_ages_out_past_capacity() {
        let mut console = LogConsole::new();
        for i in 0..CONSOLE_CAPACITY + 10 {
            console.push(format!("line {i}"));
        }
        assert_eq!(console.len(), CONSOLE_CAPACITY);
        assert!(console.lines()[0].starts_with("line 10"));
    }
}
