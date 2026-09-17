//! Developer-facing waypoint transition panel model.

use game_engine::waypoints::{
    TransitionDescriptor, TransitionEvent, TransitionPhase, WaypointId, WaypointLeg,
};

/// Bounded display state consumed by the tools-window panel.
pub struct TransitionPanel {
    pub current: Option<TransitionDescriptor>,
    pub history: Vec<TransitionEvent>,
}

impl TransitionPanel {
    pub fn new() -> Self {
        Self {
            current: None,
            history: Vec::new(),
        }
    }

    pub fn preview(&mut self) {
        let leg = WaypointLeg::new(WaypointId::SolarSystem, WaypointId::Neighborhood).unwrap();
        self.current = Some(TransitionDescriptor::for_leg(leg, 0.5));
        self.history = vec![
            TransitionEvent {
                leg,
                phase: TransitionPhase::Started,
                progress: 0.0,
                sim_time_s: 0.0,
            },
            TransitionEvent {
                leg,
                phase: TransitionPhase::Progress,
                progress: 0.5,
                sim_time_s: 1.0,
            },
            TransitionEvent {
                leg,
                phase: TransitionPhase::Completed,
                progress: 1.0,
                sim_time_s: 2.0,
            },
        ];
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

impl Default for TransitionPanel {
    fn default() -> Self {
        Self::new()
    }
}
