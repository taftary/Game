//! Player-facing navigation HUD view model.
//!
//! This module owns presentation state only. It reads simulation snapshots and
//! event payloads, returning owned readouts that a future windowed shell can
//! render without coupling the HUD to physics or a UI framework.

use game_engine::flight::{FlyToExec, ShipMode, ShipState, mode_of};
use game_engine::frames::{BodyId, FrameId};
use game_engine::handoff::HandoffEvent;
use game_engine::time::{CompressionClock, CompressionMode};

#[derive(Clone, Debug, PartialEq)]
pub struct SoiReadout {
    pub message: String,
    pub strength: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetReadout {
    pub label: String,
    pub distance_m: f64,
    pub eta_s: f64,
    pub line: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HudFrame {
    pub frame_line: String,
    pub time_line: String,
    pub soi: Option<SoiReadout>,
    pub target: Option<TargetReadout>,
}

pub struct HudInputs<'a> {
    pub ship: &'a ShipState,
    pub clock: &'a CompressionClock,
    pub executor: Option<&'a FlyToExec>,
    pub target_label: Option<&'a str>,
    pub soi_weight: f64,
    pub soi_events: &'a [HandoffEvent],
    pub body_label: Option<&'a str>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    soi_message: Option<String>,
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, inputs: &HudInputs<'_>) -> HudFrame {
        for event in inputs.soi_events {
            match event {
                HandoffEvent::Approaching { body, .. } => {
                    self.soi_message = Some(format!(
                        "Approaching {} SOI",
                        body_label(*body, inputs.body_label)
                    ));
                }
                HandoffEvent::Entered { body } => {
                    self.soi_message = Some(format!(
                        "Entering {} SOI",
                        body_label(*body, inputs.body_label)
                    ));
                }
                HandoffEvent::Exited { .. } => self.soi_message = None,
            }
        }

        let frame = inputs.ship.chain.active();
        let position = inputs.ship.chain.position();
        let body = frame.body().map(|id| body_label(id, inputs.body_label));
        let body_line = body
            .as_deref()
            .map(|name| format!(" · body {name}"))
            .unwrap_or_default();
        let frame_line = format!(
            "frame: {} · {}{} · pos ({:.3}, {:.3}, {:.3})",
            frame.name(),
            frame.unit_name(),
            body_line,
            position.x,
            position.y,
            position.z
        );

        let time_line = match inputs.clock.mode() {
            CompressionMode::RealTime => "time: real-time".to_owned(),
            CompressionMode::Compressed => {
                format!("time: x{:.3} compressed", inputs.clock.ratio())
            }
        };

        let soi = self.soi_message.clone().map(|message| SoiReadout {
            message,
            strength: inputs.soi_weight.clamp(0.0, 1.0),
        });

        let target = inputs.executor.and_then(|executor| {
            if mode_of(Some(executor)) != ShipMode::FlyTo {
                return None;
            }
            let plan = executor.plan();
            let now = inputs.clock.sim_time_s();
            let label = inputs
                .target_label
                .filter(|label| !label.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| canonical_target(plan.frame, plan.to));
            let distance_m = plan.remaining_m(now);
            let eta_s = plan.eta_s(now);
            Some(TargetReadout {
                line: format!("target: {label} · {:.3} m · ETA {:.1}s", distance_m, eta_s),
                label,
                distance_m,
                eta_s,
            })
        });

        HudFrame {
            frame_line,
            time_line,
            soi,
            target,
        }
    }
}

fn body_label(body: BodyId, supplied: Option<&str>) -> String {
    supplied
        .filter(|label| !label.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if body == BodyId::EARTH {
                "earth".to_owned()
            } else {
                format!("body-{}", body.0)
            }
        })
}

fn canonical_target(frame: FrameId, position: glam::DVec3) -> String {
    format!(
        "{}@({:.3},{:.3},{:.3})",
        frame.name(),
        position.x,
        position.y,
        position.z
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::flight::{FlyToExec, ShipState, Target, plan_fly_to};
    use game_engine::frames::{FrameChain, FrameLink};
    use game_engine::handoff::{APPROACH_WEIGHT, EXIT_WEIGHT};
    use glam::{DQuat, DVec3};

    fn ship() -> ShipState {
        ShipState {
            chain: FrameChain::new(
                FrameId::SolarSystem,
                DVec3::new(2.0, 0.0, 0.0),
                DQuat::IDENTITY,
                vec![
                    FrameLink::identity(),
                    FrameLink::identity(),
                    FrameLink::identity(),
                    FrameLink::identity(),
                ],
            ),
            vel: DVec3::ZERO,
            mass_kg: 5_000.0,
            fuel: 1.0,
        }
    }

    fn inputs<'a>(ship: &'a ShipState, clock: &'a CompressionClock) -> HudInputs<'a> {
        HudInputs {
            ship,
            clock,
            executor: None,
            target_label: None,
            soi_weight: 0.0,
            soi_events: &[],
            body_label: None,
        }
    }

    #[test]
    fn frame_and_time_readouts_use_public_display_accessors() {
        let ship = ship();
        let clock = CompressionClock::default();
        let mut hud = Hud::new();
        let frame = hud.update(&inputs(&ship, &clock));
        assert!(frame.frame_line.contains("solar-system"));
        assert!(frame.frame_line.contains("AU"));
        assert_eq!(frame.time_line, "time: real-time");
    }

    #[test]
    fn soi_message_holds_and_clears_without_repeats() {
        let ship = ship();
        let clock = CompressionClock::default();
        let mut hud = Hud::new();
        let approaching = [HandoffEvent::Approaching {
            body: BodyId::EARTH,
            weight: APPROACH_WEIGHT,
        }];
        let first = hud.update(&HudInputs {
            soi_events: &approaching,
            soi_weight: 0.2,
            ..inputs(&ship, &clock)
        });
        assert_eq!(first.soi.as_ref().unwrap().message, "Approaching earth SOI");
        let held = hud.update(&HudInputs {
            soi_weight: 0.5,
            ..inputs(&ship, &clock)
        });
        assert_eq!(held.soi.as_ref().unwrap().message, "Approaching earth SOI");
        let exited = [HandoffEvent::Exited {
            body: BodyId::EARTH,
        }];
        let cleared = hud.update(&HudInputs {
            soi_events: &exited,
            soi_weight: EXIT_WEIGHT - 0.01,
            ..inputs(&ship, &clock)
        });
        assert!(cleared.soi.is_none());
    }

    #[test]
    fn target_label_and_coordinate_fallback_are_safe() {
        let ship = ship();
        let mut clock = CompressionClock::default();
        clock.slew(100.0, 1.0);
        let target = Target::new(FrameId::SolarSystem, DVec3::new(3.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, ship.chain.position(), &target, 0.0).unwrap();
        let mut executor = FlyToExec::new(plan);
        executor.commit().unwrap();
        let mut hud = Hud::new();
        let labeled = hud.update(&HudInputs {
            executor: Some(&executor),
            target_label: Some("Earth orbit"),
            ..inputs(&ship, &clock)
        });
        assert_eq!(labeled.target.unwrap().label, "Earth orbit");
        let fallback = hud.update(&HudInputs {
            executor: Some(&executor),
            target_label: Some("  "),
            ..inputs(&ship, &clock)
        });
        assert!(fallback.target.unwrap().label.starts_with("solar-system@"));
    }

    #[test]
    fn free_flight_hides_target_and_hud_is_read_only() {
        let ship = ship();
        let before = ship.clone();
        let clock = CompressionClock::default();
        let before_clock = (clock.ratio(), clock.sim_time_s());
        let mut hud = Hud::new();
        let frame = hud.update(&inputs(&ship, &clock));
        assert!(frame.target.is_none());
        assert_eq!(ship, before);
        assert_eq!((clock.ratio(), clock.sim_time_s()), before_clock);
    }
}
