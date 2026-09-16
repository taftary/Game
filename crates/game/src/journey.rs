//! Journey state machine, top segment: `GalaxyMap` → `SystemMap` → `Orbit`.
//!
//! The v1 machine (`docs/game/journey.md`) runs GalaxyMap (L2, L1 as
//! backdrop) → SystemMap (L3, L4 planet focus as a zoom mode) → Orbit
//! (L5 arrival) → Descent (M2) → Surface (M3). This module owns the top
//! segment; M2 appends `Descent` below `Orbit` — `Orbit` never assumes
//! it is the root.
//!
//! Rules enforced by construction:
//!
//! - **One active layer:** the machine holds exactly one
//!   [`JourneyState`] (an enum, not a set) — surface XOR orbit-far XOR
//!   maps, always.
//! - **Selection ≠ transition:** `SelectStar`/`SelectPlanet` only arm a
//!   selection; `EnterSystem`/`EnterOrbit` commit the layer swap.
//! - **Fades exactly on layer swaps:** [`JourneyEffect::Fade`] is
//!   emitted on every swap, never on selection or L4 focus changes (no
//!   hard loading screens in the ideal path).
//! - **Enter/exit hooks:** every swap emits [`JourneyEffect::Prefetch`]
//!   for the incoming layer and [`JourneyEffect::Evict`] for the
//!   outgoing one.
//! - **Headless:** pure state + events, no window, no GPU, no wall-clock.
//!   `game` stays `vulkano`-free (architecture rule).
//!
//! ```
//! use game::journey::{Journey, JourneyEvent, Layer};
//!
//! let mut journey = Journey::new(99);
//! assert_eq!(journey.active_layer(), Layer::Galaxy);
//! journey.update(JourneyEvent::SelectStar(2));
//! let effects = journey.update(JourneyEvent::EnterSystem);
//! assert_eq!(journey.active_layer(), Layer::System);
//! assert!(effects.iter().any(|e| matches!(e,
//!     game::journey::JourneyEffect::Fade { .. })));
//! ```

/// The one active high-detail layer. Exactly one is live at a time —
/// the machine owns the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Galaxy,
    System,
    Orbit,
}

/// `GalaxyMap` state (L2; L1 cosmic-web backdrop rides along as a flag).
#[derive(Clone, Debug, PartialEq)]
pub struct GalaxyMapState {
    /// Runtime seed; a new save rolls a new one (regeneration until M4).
    pub seed: u64,
    /// Armed star selection; `EnterSystem` commits it.
    pub selected_star: Option<u32>,
    /// L1 is backdrop-only in v1 — always true; the flag exists so a
    /// traversable L1 (post-v1 candidate) toggles data, not shape.
    pub backdrop: bool,
}

/// `SystemMap` state (L3; L4 planet focus as a zoom mode).
#[derive(Clone, Debug, PartialEq)]
pub struct SystemMapState {
    pub seed: u64,
    pub star_index: u32,
    /// Armed planet selection; `EnterOrbit` commits it.
    pub selected_planet: Option<u32>,
    /// L4 planet focus: within-layer zoom mode, not a layer swap — no
    /// fade, no prefetch; `None` is the wide system view.
    pub focus: Option<u32>,
}

/// `Orbit` state (L5 arrival): the holding state (survey, pick landing
/// site, descend). M2 appends `Descent` below — never a root.
#[derive(Clone, Debug, PartialEq)]
pub struct OrbitState {
    pub seed: u64,
    pub star_index: u32,
    pub planet_index: u32,
}

/// The machine's one active state.
#[derive(Clone, Debug, PartialEq)]
pub enum JourneyState {
    GalaxyMap(GalaxyMapState),
    SystemMap(SystemMapState),
    Orbit(OrbitState),
}

/// Selection-driven inputs. Events are layer-scoped: an event that makes
/// no sense in the active layer is a silent no-op (the Phase-4 travel UI
/// only offers valid ones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JourneyEvent {
    /// Arm a star (GalaxyMap only).
    SelectStar(u32),
    /// Commit the armed star → SystemMap (needs a selection).
    EnterSystem,
    /// Arm a planet (SystemMap only).
    SelectPlanet(u32),
    /// Enter/exit the L4 planet-focus zoom mode (SystemMap only).
    FocusPlanet(Option<u32>),
    /// Commit the armed planet → Orbit arrival (needs a selection).
    EnterOrbit,
    /// Back one layer: Orbit → SystemMap → GalaxyMap. No-op at the top.
    Ascend,
}

/// What the views prefetch on layer enter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefetchTarget {
    GalaxyMap,
    SystemMap { star_index: u32 },
    Orbit { star_index: u32, planet_index: u32 },
}

/// Effects of one [`Journey::update`], in emission order: evict the
/// outgoing layer, fade across, prefetch the incoming one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JourneyEffect {
    /// Fade across a layer swap. Emitted exactly on swaps.
    Fade { from: Layer, to: Layer },
    /// Stream/prefetch a layer's assets on enter.
    Prefetch(PrefetchTarget),
    /// Drop a layer's assets on exit.
    Evict(Layer),
}

fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    hash
}

fn mix(hash: u64, value: u64) -> u64 {
    fnv1a(&value.to_le_bytes(), hash)
}

/// The journey state machine. Opens on `GalaxyMap` for `seed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Journey {
    state: JourneyState,
}

impl Journey {
    /// Open the machine on the galaxy map for `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            state: JourneyState::GalaxyMap(GalaxyMapState {
                seed,
                selected_star: None,
                backdrop: true,
            }),
        }
    }

    /// The one active state.
    pub fn state(&self) -> &JourneyState {
        &self.state
    }

    /// The one active layer (the one-active-layer rule, as an accessor).
    pub fn active_layer(&self) -> Layer {
        match &self.state {
            JourneyState::GalaxyMap(_) => Layer::Galaxy,
            JourneyState::SystemMap(_) => Layer::System,
            JourneyState::Orbit(_) => Layer::Orbit,
        }
    }

    /// Deterministic state hash for the fixed-step regression (UMAP-009):
    /// same event script → same hash on every platform.
    pub fn state_hash(&self) -> u64 {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        match &self.state {
            JourneyState::GalaxyMap(s) => {
                h = mix(h, 0);
                h = mix(h, s.seed);
                // 0 = no selection; indices shift by one (never collide).
                h = mix(h, s.selected_star.map_or(0, |i| u64::from(i) + 1));
                h = mix(h, u64::from(s.backdrop));
            }
            JourneyState::SystemMap(s) => {
                h = mix(h, 1);
                h = mix(h, s.seed);
                h = mix(h, u64::from(s.star_index));
                h = mix(h, s.selected_planet.map_or(0, |i| u64::from(i) + 1));
                h = mix(h, s.focus.map_or(0, |i| u64::from(i) + 1));
            }
            JourneyState::Orbit(s) => {
                h = mix(h, 2);
                h = mix(h, s.seed);
                h = mix(h, u64::from(s.star_index));
                h = mix(h, u64::from(s.planet_index));
            }
        }
        h
    }

    /// Apply one event; returns the emitted effects (possibly empty —
    /// selections, focus changes, and wrong-layer/no-op events emit
    /// nothing).
    pub fn update(&mut self, event: JourneyEvent) -> Vec<JourneyEffect> {
        match (&mut self.state, event) {
            (JourneyState::GalaxyMap(s), JourneyEvent::SelectStar(star_index)) => {
                s.selected_star = Some(star_index);
                Vec::new()
            }
            (JourneyState::GalaxyMap(s), JourneyEvent::EnterSystem) => match s.selected_star {
                Some(star_index) => {
                    let seed = s.seed;
                    self.state = JourneyState::SystemMap(SystemMapState {
                        seed,
                        star_index,
                        selected_planet: None,
                        focus: None,
                    });
                    vec![
                        JourneyEffect::Evict(Layer::Galaxy),
                        JourneyEffect::Fade {
                            from: Layer::Galaxy,
                            to: Layer::System,
                        },
                        JourneyEffect::Prefetch(PrefetchTarget::SystemMap { star_index }),
                    ]
                }
                None => Vec::new(),
            },
            (JourneyState::SystemMap(s), JourneyEvent::SelectPlanet(planet_index)) => {
                s.selected_planet = Some(planet_index);
                Vec::new()
            }
            (JourneyState::SystemMap(s), JourneyEvent::FocusPlanet(focus)) => {
                s.focus = focus;
                Vec::new()
            }
            (JourneyState::SystemMap(s), JourneyEvent::EnterOrbit) => match s.selected_planet {
                Some(planet_index) => {
                    let (seed, star_index) = (s.seed, s.star_index);
                    self.state = JourneyState::Orbit(OrbitState {
                        seed,
                        star_index,
                        planet_index,
                    });
                    vec![
                        JourneyEffect::Evict(Layer::System),
                        JourneyEffect::Fade {
                            from: Layer::System,
                            to: Layer::Orbit,
                        },
                        JourneyEffect::Prefetch(PrefetchTarget::Orbit {
                            star_index,
                            planet_index,
                        }),
                    ]
                }
                None => Vec::new(),
            },
            (JourneyState::SystemMap(s), JourneyEvent::Ascend) => {
                let (seed, star_index) = (s.seed, s.star_index);
                self.state = JourneyState::GalaxyMap(GalaxyMapState {
                    seed,
                    selected_star: Some(star_index),
                    backdrop: true,
                });
                vec![
                    JourneyEffect::Evict(Layer::System),
                    JourneyEffect::Fade {
                        from: Layer::System,
                        to: Layer::Galaxy,
                    },
                    JourneyEffect::Prefetch(PrefetchTarget::GalaxyMap),
                ]
            }
            (JourneyState::Orbit(s), JourneyEvent::Ascend) => {
                let (seed, star_index, planet_index) = (s.seed, s.star_index, s.planet_index);
                self.state = JourneyState::SystemMap(SystemMapState {
                    seed,
                    star_index,
                    selected_planet: Some(planet_index),
                    focus: None,
                });
                vec![
                    JourneyEffect::Evict(Layer::Orbit),
                    JourneyEffect::Fade {
                        from: Layer::Orbit,
                        to: Layer::System,
                    },
                    JourneyEffect::Prefetch(PrefetchTarget::SystemMap { star_index }),
                ]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full scripted traversal with pinned hashes (UMAP-009): the
    /// fixed-step regression. Any intentional machine change updates
    /// these alongside review — they are change-detectors.
    #[test]
    fn scripted_traversal_replays_pinned_hashes() {
        let mut journey = Journey::new(99);
        assert_eq!(journey.state_hash(), 9_020_526_733_289_774_599);

        assert!(journey.update(JourneyEvent::SelectStar(2)).is_empty());
        assert_eq!(journey.state_hash(), 17_310_629_064_963_469_700);

        let fx = journey.update(JourneyEvent::EnterSystem);
        assert_eq!(
            fx,
            vec![
                JourneyEffect::Evict(Layer::Galaxy),
                JourneyEffect::Fade {
                    from: Layer::Galaxy,
                    to: Layer::System,
                },
                JourneyEffect::Prefetch(PrefetchTarget::SystemMap { star_index: 2 }),
            ]
        );
        assert_eq!(journey.active_layer(), Layer::System);
        assert_eq!(journey.state_hash(), 1_156_220_397_230_407_301);

        assert!(journey.update(JourneyEvent::SelectPlanet(1)).is_empty());
        assert_eq!(journey.state_hash(), 12_540_242_253_431_762_695);

        // L4 focus is a within-layer zoom: no effects, layer unchanged.
        assert!(
            journey
                .update(JourneyEvent::FocusPlanet(Some(1)))
                .is_empty()
        );
        assert_eq!(journey.active_layer(), Layer::System);
        assert_eq!(journey.state_hash(), 8_075_611_439_496_583_877);
        assert!(journey.update(JourneyEvent::FocusPlanet(None)).is_empty());
        assert_eq!(journey.state_hash(), 12_540_242_253_431_762_695);

        let fx = journey.update(JourneyEvent::EnterOrbit);
        assert_eq!(fx.len(), 3);
        assert_eq!(journey.active_layer(), Layer::Orbit);
        assert_eq!(journey.state_hash(), 16_074_160_238_727_206_535);

        let fx = journey.update(JourneyEvent::Ascend);
        assert_eq!(fx.len(), 3);
        assert_eq!(journey.active_layer(), Layer::System);
        assert_eq!(journey.state_hash(), 12_540_242_253_431_762_695);

        journey.update(JourneyEvent::Ascend);
        assert_eq!(journey.active_layer(), Layer::Galaxy);
        assert_eq!(journey.state_hash(), 17_310_629_064_963_469_700);
    }

    #[test]
    fn commits_need_armed_selections() {
        let mut journey = Journey::new(5);
        assert!(journey.update(JourneyEvent::EnterSystem).is_empty());
        assert_eq!(journey.active_layer(), Layer::Galaxy);
        journey.update(JourneyEvent::SelectStar(0));
        journey.update(JourneyEvent::EnterSystem);
        assert!(journey.update(JourneyEvent::EnterOrbit).is_empty());
        assert_eq!(journey.active_layer(), Layer::System);
    }

    #[test]
    fn wrong_layer_events_and_top_ascend_are_noops() {
        let mut journey = Journey::new(5);
        assert!(journey.update(JourneyEvent::Ascend).is_empty());
        assert!(journey.update(JourneyEvent::SelectPlanet(0)).is_empty());
        assert!(journey.update(JourneyEvent::EnterOrbit).is_empty());
        assert_eq!(journey.active_layer(), Layer::Galaxy);

        journey.update(JourneyEvent::SelectStar(1));
        journey.update(JourneyEvent::EnterSystem);
        journey.update(JourneyEvent::SelectPlanet(0));
        journey.update(JourneyEvent::EnterOrbit);
        // Orbit ignores map-layer events.
        assert!(journey.update(JourneyEvent::SelectStar(9)).is_empty());
        assert!(
            journey
                .update(JourneyEvent::FocusPlanet(Some(0)))
                .is_empty()
        );
        assert_eq!(
            journey.state(),
            &JourneyState::Orbit(OrbitState {
                seed: 5,
                star_index: 1,
                planet_index: 0,
            })
        );
    }

    #[test]
    fn ascend_remembers_where_you_came_from() {
        let mut journey = Journey::new(5);
        journey.update(JourneyEvent::SelectStar(4));
        journey.update(JourneyEvent::EnterSystem);
        journey.update(JourneyEvent::SelectPlanet(2));
        journey.update(JourneyEvent::EnterOrbit);
        journey.update(JourneyEvent::Ascend);
        assert_eq!(
            journey.state(),
            &JourneyState::SystemMap(SystemMapState {
                seed: 5,
                star_index: 4,
                selected_planet: Some(2),
                focus: None,
            })
        );
        journey.update(JourneyEvent::Ascend);
        assert_eq!(
            journey.state(),
            &JourneyState::GalaxyMap(GalaxyMapState {
                seed: 5,
                selected_star: Some(4),
                backdrop: true,
            })
        );
    }
}
