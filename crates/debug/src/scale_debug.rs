//! Read-only scale debug screens for the ten journey dimensions.

use game::journey::{Journey, Layer};
use game_engine::waypoints::{TransitionEvent, TransitionPhase, WaypointId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleRow {
    pub waypoint: WaypointId,
    pub active: bool,
    pub position: &'static str,
}

/// Selection state for the scale screen. It owns no simulation state and only
/// formats snapshots supplied by the debug shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleDebugState {
    pub selected: Option<WaypointId>,
}

impl ScaleDebugState {
    pub const fn new() -> Self {
        Self { selected: None }
    }

    pub fn select(&mut self, waypoint: Option<WaypointId>) {
        self.selected = waypoint;
    }

    pub fn rows(self, journey: &Journey) -> [ScaleRow; 10] {
        let active = match journey.active_layer() {
            Layer::Galaxy => WaypointId::MilkyWay,
            Layer::System => WaypointId::SolarSystem,
            Layer::Orbit => WaypointId::Earth,
        };
        WaypointId::ALL.map(|waypoint| ScaleRow {
            waypoint,
            active: waypoint == active,
            position: if waypoint == active {
                "active frame"
            } else {
                "inactive · last state"
            },
        })
    }
}

impl Default for ScaleDebugState {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn registers_all_ten_dimensions_and_active_frame() {
        let state = ScaleDebugState::new();
        let rows = state.rows(&Journey::new(7));
        assert_eq!(rows.len(), 10);
        assert_eq!(rows.iter().filter(|row| row.active).count(), 1);
        assert_eq!(
            rows.iter().find(|row| row.active).map(|row| row.waypoint),
            Some(WaypointId::MilkyWay)
        );
    }

    #[test]
    fn selected_dimension_is_ui_state_only() {
        let mut state = ScaleDebugState::new();
        state.select(Some(WaypointId::Interior));
        assert_eq!(state.selected, Some(WaypointId::Interior));
        assert_eq!(Journey::new(3).active_layer(), Layer::Galaxy);
    }
}
