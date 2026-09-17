//! Developer-facing waypoint transition panel model.

use game_engine::waypoints::{TransitionDescriptor, TransitionEvent, WaypointId, WaypointLeg};

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
