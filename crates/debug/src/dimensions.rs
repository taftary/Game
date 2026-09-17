//! Read-only 3D dimension tabs for the developer tools window.

use crate::system_map::SystemMapView;
use game::journey::{Journey, Layer};
use game_engine::waypoints::{WaypointId, WaypointLeg};
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionTab {
    Universe,
    Galactic,
    System,
    Planetary,
    Orbit,
    Connections,
}

impl DimensionTab {
    pub const ALL: [Self; 6] = [
        Self::Universe,
        Self::Galactic,
        Self::System,
        Self::Planetary,
        Self::Orbit,
        Self::Connections,
    ];
    pub const fn index(self) -> usize {
        match self {
            Self::Universe => 0,
            Self::Galactic => 1,
            Self::System => 2,
            Self::Planetary => 3,
            Self::Orbit => 4,
            Self::Connections => 5,
        }
    }
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Universe),
            1 => Some(Self::Galactic),
            2 => Some(Self::System),
            3 => Some(Self::Planetary),
            4 => Some(Self::Orbit),
            5 => Some(Self::Connections),
            _ => None,
        }
    }
    pub const fn title(self) -> &'static str {
        match self {
            Self::Universe => "L1 Universe",
            Self::Galactic => "L2 Galactic",
            Self::System => "L3 System",
            Self::Planetary => "L4 Planetary",
            Self::Orbit => "L5 Orbit",
            Self::Connections => "Connections",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DimensionsState {
    pub tab: DimensionTab,
}

impl DimensionsState {
    pub const fn new() -> Self {
        Self {
            tab: DimensionTab::Universe,
        }
    }
    pub fn select(&mut self, tab: DimensionTab) {
        self.tab = tab;
    }
}

impl Default for DimensionsState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn graph_positions() -> [Vec3; 10] {
    std::array::from_fn(|index| {
        Vec3::new(
            (index as f32 - 4.5) * 1.5,
            (index as f32 * 0.7).sin() * 0.7,
            0.0,
        )
    })
}

pub fn graph_legs() -> [WaypointLeg; 9] {
    std::array::from_fn(|index| {
        WaypointLeg::new(WaypointId::ALL[index], WaypointId::ALL[index + 1])
            .expect("waypoint list must be adjacent")
    })
}

pub fn active_waypoint(journey: &Journey) -> WaypointId {
    match journey.active_layer() {
        Layer::Galaxy => WaypointId::MilkyWay,
        Layer::System => WaypointId::SolarSystem,
        Layer::Orbit => WaypointId::Earth,
    }
}

pub fn planetary_camera(system: &SystemMapView) -> crate::map_camera::MapOrbitCamera {
    let mut camera = system.camera.clone();
    if let Some(index) = system.selected.or(system.focus)
        && let Some(planet) = system.system.planets.get(index as usize)
    {
        let (x, y, z) = crate::system_map::planet_slot(planet.orbit_radius_au, index);
        camera.set_target(Vec3::new(x as f32, y as f32, z as f32));
        let half = (planet.orbit_radius_au * 0.6).max(crate::system_map::MIN_VIEW_RADIUS_AU) as f32;
        camera.set_distance(crate::map_camera::MapOrbitCamera::distance_for_half_height(
            half,
        ));
    }
    camera
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tabs_and_graph_are_stable() {
        for (index, tab) in DimensionTab::ALL.iter().enumerate() {
            assert_eq!(tab.index(), index);
            assert_eq!(DimensionTab::from_index(index), Some(*tab));
        }
        assert_eq!(DimensionTab::from_index(6), None);
        assert_eq!(graph_positions().len(), 10);
        assert_eq!(graph_legs().len(), 9);
        assert_eq!(active_waypoint(&Journey::new(1)), WaypointId::MilkyWay);
    }
}
