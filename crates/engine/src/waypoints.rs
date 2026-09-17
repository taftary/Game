//! Waypoint-scale coordination for the spec §9.3 journey.
//!
//! Waypoints are a presentation layer over the seven stable reference frames.
//! This module never changes frame coordinates; it only derives deterministic
//! visual parameters and lifecycle events from caller-owned progress.

use crate::frames::{FrameChain, FrameId};
use std::collections::VecDeque;

/// The ten player-facing dimensions, ordered from facility interior to the
/// cosmic web.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WaypointId {
    Interior = 10,
    Exterior = 9,
    Aerial = 8,
    Earth = 7,
    SolarSystem = 6,
    Neighborhood = 5,
    MilkyWay = 4,
    LocalGroup = 3,
    Supercluster = 2,
    CosmicWeb = 1,
}

impl WaypointId {
    pub const ALL: [Self; 10] = [
        Self::Interior,
        Self::Exterior,
        Self::Aerial,
        Self::Earth,
        Self::SolarSystem,
        Self::Neighborhood,
        Self::MilkyWay,
        Self::LocalGroup,
        Self::Supercluster,
        Self::CosmicWeb,
    ];

    pub const fn number(self) -> u8 {
        self as u8
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Interior => "interior",
            Self::Exterior => "exterior",
            Self::Aerial => "aerial",
            Self::Earth => "earth",
            Self::SolarSystem => "solar-system",
            Self::Neighborhood => "neighborhood",
            Self::MilkyWay => "milky-way",
            Self::LocalGroup => "local-group",
            Self::Supercluster => "supercluster",
            Self::CosmicWeb => "cosmic-web",
        }
    }

    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::CosmicWeb),
            2 => Some(Self::Supercluster),
            3 => Some(Self::LocalGroup),
            4 => Some(Self::MilkyWay),
            5 => Some(Self::Neighborhood),
            6 => Some(Self::SolarSystem),
            7 => Some(Self::Earth),
            8 => Some(Self::Aerial),
            9 => Some(Self::Exterior),
            10 => Some(Self::Interior),
            _ => None,
        }
    }
}

/// One directed boundary between adjacent waypoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WaypointLeg {
    pub from: WaypointId,
    pub to: WaypointId,
}

impl WaypointLeg {
    pub const fn new(from: WaypointId, to: WaypointId) -> Option<Self> {
        if from.number() == to.number() + 1 {
            Some(Self { from, to })
        } else {
            None
        }
    }

    pub const fn number(self) -> u8 {
        self.from.number()
    }
}

/// Derive a waypoint from the stable frame and an optional local altitude in
/// meters. Non-planet frames map directly; local ENU uses altitude bands.
pub fn waypoint_for(chain: &FrameChain, altitude_m: f64) -> WaypointId {
    match chain.active() {
        FrameId::LocalEnu(_) => {
            if altitude_m < 10.0 {
                WaypointId::Interior
            } else if altitude_m < 1_000.0 {
                WaypointId::Exterior
            } else if altitude_m < 100_000.0 {
                WaypointId::Aerial
            } else {
                WaypointId::Earth
            }
        }
        FrameId::Planetocentric(_) => {
            if altitude_m < 100_000.0 {
                WaypointId::Aerial
            } else {
                WaypointId::Earth
            }
        }
        FrameId::SolarSystem => WaypointId::SolarSystem,
        FrameId::StellarNeighborhood => WaypointId::Neighborhood,
        FrameId::LocalGroup => WaypointId::LocalGroup,
        FrameId::Galactocentric => WaypointId::MilkyWay,
        FrameId::Cosmological => WaypointId::CosmicWeb,
    }
}

/// Interpolated visual controls for one waypoint leg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionDescriptor {
    pub leg: WaypointLeg,
    pub progress: f64,
    pub exposure_keys: [f64; 3],
    pub cue_weight: f64,
    pub zodiacal_intensity: f64,
    pub haze: f64,
    pub sky_depth: f64,
    pub limb_glow: f64,
    pub sun_disk_scale: f64,
}

impl TransitionDescriptor {
    /// Build a descriptor. Progress is clamped so callers cannot create a
    /// partially invalid render state during a cancelled fly-to.
    pub fn for_leg(leg: WaypointLeg, progress: f64) -> Self {
        let t = progress.clamp(0.0, 1.0);
        let n = leg.number();
        let start = params_for(leg.from);
        let end = params_for(leg.to);
        Self {
            leg,
            progress: t,
            exposure_keys: lerp3(start.0, end.0, t),
            cue_weight: lerp(start.1, end.1, t),
            zodiacal_intensity: lerp(start.2, end.2, t),
            haze: lerp(start.3, end.3, t),
            sky_depth: lerp(start.4, end.4, t),
            limb_glow: lerp(start.5, end.5, t),
            sun_disk_scale: if n == 7 || n == 6 {
                1.0 - t
            } else {
                lerp(start.6, end.6, t)
            },
        }
    }
}

fn params_for(id: WaypointId) -> ([f64; 3], f64, f64, f64, f64, f64, f64) {
    match id {
        WaypointId::Interior => ([0.02, 0.01, 0.0001], 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
        WaypointId::Exterior => ([0.25, 0.12, 0.01], 0.2, 0.0, 0.25, 0.1, 0.0, 1.0),
        WaypointId::Aerial => ([0.35, 0.2, 0.02], 0.35, 0.0, 0.6, 0.55, 0.2, 1.0),
        WaypointId::Earth => ([0.45, 0.25, 0.03], 0.45, 0.0, 0.0, 1.0, 1.0, 1.0),
        WaypointId::SolarSystem => ([0.25, 0.12, 0.08], 0.55, 1.0, 0.0, 1.0, 0.0, 0.04),
        WaypointId::Neighborhood => ([0.08, 0.04, 0.12], 0.65, 0.6, 0.0, 1.0, 0.0, 0.01),
        WaypointId::MilkyWay => ([0.04, 0.02, 0.18], 0.8, 0.45, 0.0, 1.0, 0.0, 0.005),
        WaypointId::LocalGroup => ([0.02, 0.01, 0.22], 0.85, 0.2, 0.0, 1.0, 0.0, 0.002),
        WaypointId::Supercluster => ([0.01, 0.005, 0.3], 0.9, 0.05, 0.0, 1.0, 0.0, 0.001),
        WaypointId::CosmicWeb => ([0.005, 0.002, 0.4], 1.0, 0.0, 0.0, 1.0, 0.0, 0.0005),
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

/// Lifecycle of a transition event, consumable by debug and future autosave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionPhase {
    Started,
    Progress,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionEvent {
    pub leg: WaypointLeg,
    pub phase: TransitionPhase,
    pub progress: f64,
    pub sim_time_s: f64,
}

/// Bounded event queue. Dropping the oldest progress event preserves boundary
/// events and prevents a stalled debug consumer from growing memory forever.
#[derive(Debug)]
pub struct TransitionEvents {
    queue: VecDeque<TransitionEvent>,
    capacity: usize,
}

impl TransitionEvents {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }
    pub fn emit(&mut self, event: TransitionEvent) {
        if self.queue.len() == self.capacity {
            self.queue.pop_front();
        }
        self.queue.push_back(event);
    }
    pub fn drain(&mut self) -> impl Iterator<Item = TransitionEvent> + '_ {
        self.queue.drain(..)
    }
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{BodyId, FrameLink};
    use glam::{DQuat, DVec3};

    fn chain(frame: FrameId) -> FrameChain {
        FrameChain::new(
            frame,
            DVec3::ZERO,
            DQuat::IDENTITY,
            vec![FrameLink::identity(); frame.depth()],
        )
    }

    #[test]
    fn maps_all_scale_frames_and_local_altitude_bands() {
        assert_eq!(
            waypoint_for(&chain(FrameId::Cosmological), 0.0),
            WaypointId::CosmicWeb
        );
        assert_eq!(
            waypoint_for(&chain(FrameId::SolarSystem), 0.0),
            WaypointId::SolarSystem
        );
        assert_eq!(
            waypoint_for(&chain(FrameId::LocalEnu(BodyId::EARTH)), 50.0),
            WaypointId::Exterior
        );
        assert_eq!(
            waypoint_for(&chain(FrameId::LocalEnu(BodyId::EARTH)), 100_000.0),
            WaypointId::Earth
        );
    }

    #[test]
    fn descriptor_is_clamped_and_handoff_is_continuous() {
        let leg = WaypointLeg::new(WaypointId::SolarSystem, WaypointId::Neighborhood).unwrap();
        let a = TransitionDescriptor::for_leg(leg, -1.0);
        let b = TransitionDescriptor::for_leg(leg, 2.0);
        assert_eq!(a.progress, 0.0);
        assert_eq!(b.progress, 1.0);
        let mid = TransitionDescriptor::for_leg(leg, 0.5);
        assert!(mid.exposure_keys.iter().all(|v| v.is_finite()));
        assert!(mid.zodiacal_intensity > 0.0);
    }

    #[test]
    fn event_queue_is_bounded_and_drainable() {
        let leg = WaypointLeg::new(WaypointId::Earth, WaypointId::SolarSystem).unwrap();
        let mut events = TransitionEvents::new(2);
        for i in 0..3 {
            events.emit(TransitionEvent {
                leg,
                phase: TransitionPhase::Progress,
                progress: i as f64,
                sim_time_s: i as f64,
            });
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events.drain().count(), 2);
        assert!(events.is_empty());
    }
}
