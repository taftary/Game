//! Named debug actions: the single source for fixed key bindings,
//! button labels, and the Settings → Controls listing.
//!
//! Parity rule (ADR-022): every action has exactly one key and one
//! button; the button label shows the key. Bindings are fixed for
//! v0.3.1 — this registry is the one place a future remap editor
//! would read and write.
//!
//! Chrome keys never shadow content (game) keys: chrome lives on
//! `F1`–`F3`, backquote, `F6`–`F8`, `F9`–`F10`, dropdown-captured
//! digits and `Esc`. Content keys (`WASD`/arrows, `U`, `P`, `E`,
//! `T`, `Q`, `F`, `R`, `G`/`B`, `Home`, `1`–`6` shader modes, `F5`
//! twilight) are reserved for the tab content.

use game_engine::waypoints::WaypointId;

/// Dropdown order: largest scale first (reverse of
/// [`WaypointId::ALL`], which runs interior → cosmic web).
pub const DROPDOWN_ORDER: [WaypointId; 10] = [
    WaypointId::CosmicWeb,
    WaypointId::Supercluster,
    WaypointId::LocalGroup,
    WaypointId::MilkyWay,
    WaypointId::Neighborhood,
    WaypointId::SolarSystem,
    WaypointId::Earth,
    WaypointId::Aerial,
    WaypointId::Exterior,
    WaypointId::Interior,
];

/// Digit key selecting dropdown entry `index`: `1`–`9` → 0–8,
/// `0` → 9.
pub const fn digit_for_dimension_index(index: usize) -> Option<u8> {
    match index {
        0..=8 => Some((index + 1) as u8),
        9 => Some(0),
        _ => None,
    }
}

/// Dropdown entry for a digit key (`1`–`9` → 0–8, `0` → 9).
pub const fn dimension_index_for_digit(digit: u8) -> Option<usize> {
    match digit {
        1..=9 => Some((digit - 1) as usize),
        0 => Some(9),
        _ => None,
    }
}

/// Listing group for the Controls section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionGroup {
    Chrome,
    Navigate,
    Camera,
    Travel,
}

impl ActionGroup {
    pub const ALL: [ActionGroup; 4] = [
        ActionGroup::Chrome,
        ActionGroup::Navigate,
        ActionGroup::Camera,
        ActionGroup::Travel,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            ActionGroup::Chrome => "Chrome",
            ActionGroup::Navigate => "Navigation",
            ActionGroup::Camera => "Camera & walk",
            ActionGroup::Travel => "Travel",
        }
    }
}

/// One named debug action. `SelectDimension(i)` addresses
/// [`DROPDOWN_ORDER`] (`i` is 0–9); `ShaderMode(i)` addresses the
/// planet debug-mode list (`i` is 0–5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Chrome (F-keys, backquote, Esc). The top bar is always visible,
    // so it has no toggle.
    ToggleLeftDock,
    ToggleRightDock,
    ToggleDevWidget,
    ShowWidgetFps,
    ShowWidgetConsole,
    ShowWidgetInspector,
    UnwindUi,
    // Navigation (F1–F3, dropdown-captured digits).
    NavGameDemo,
    NavDimensions,
    NavSettings,
    SelectDimension(usize),
    // Camera & walk (content keys, per-tab context).
    WalkNorth,
    WalkSouth,
    WalkWest,
    WalkEast,
    PlayerToggle,
    CameraCycle,
    PresetPerspective,
    PresetTop,
    PresetBottom,
    PresetRight,
    RerollSeed,
    TopDownSnap,
    TwilightCycle,
    ShaderMode(usize),
    // Travel (content keys, map contexts).
    TravelOffer,
    TravelBegin,
    AscendLayer,
    FocusToggle,
    ConfirmField,
}

impl Action {
    /// Fixed key label shown on buttons and in Controls.
    pub const fn key_label(self) -> &'static str {
        match self {
            Action::ToggleLeftDock => "F9",
            Action::ToggleRightDock => "F10",
            Action::ToggleDevWidget => "`",
            Action::ShowWidgetFps => "F6",
            Action::ShowWidgetConsole => "F7",
            Action::ShowWidgetInspector => "F8",
            Action::UnwindUi => "Esc",
            Action::NavGameDemo => "F1",
            Action::NavDimensions => "F2",
            Action::NavSettings => "F3",
            Action::SelectDimension(_) => "1-0",
            Action::WalkNorth => "W / Up",
            Action::WalkSouth => "S / Down",
            Action::WalkWest => "A / Left",
            Action::WalkEast => "D / Right",
            Action::PlayerToggle => "U",
            Action::CameraCycle => "P",
            Action::PresetPerspective => "G",
            Action::PresetTop => "T",
            Action::PresetBottom => "B",
            Action::PresetRight => "R",
            Action::RerollSeed => "R",
            Action::TopDownSnap => "Home",
            Action::TwilightCycle => "F5",
            Action::ShaderMode(_) => "1-6",
            Action::TravelOffer => "T",
            Action::TravelBegin => "E",
            Action::AscendLayer => "Q",
            Action::FocusToggle => "F",
            Action::ConfirmField => "Enter",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Action::ToggleLeftDock => "Toggle left dock",
            Action::ToggleRightDock => "Toggle right dock",
            Action::ToggleDevWidget => "Toggle dev widget",
            Action::ShowWidgetFps => "Widget: FPS",
            Action::ShowWidgetConsole => "Widget: Console",
            Action::ShowWidgetInspector => "Widget: Inspector",
            Action::UnwindUi => "Close menus / unfocus",
            Action::NavGameDemo => "Game Demo tab",
            Action::NavDimensions => "Dimensions menu",
            Action::NavSettings => "Settings tab",
            Action::SelectDimension(_) => "Select dimension",
            Action::WalkNorth => "Walk north",
            Action::WalkSouth => "Walk south",
            Action::WalkWest => "Walk west",
            Action::WalkEast => "Walk east",
            Action::PlayerToggle => "Toggle player mode",
            Action::CameraCycle => "Cycle camera",
            Action::PresetPerspective => "Camera preset: perspective",
            Action::PresetTop => "Camera preset: top",
            Action::PresetBottom => "Camera preset: bottom",
            Action::PresetRight => "Camera preset: right",
            Action::RerollSeed => "Re-roll galaxy seed",
            Action::TopDownSnap => "Top-down snap",
            Action::TwilightCycle => "Cycle twilight stage",
            Action::ShaderMode(_) => "Debug shader mode",
            Action::TravelOffer => "Arm/withdraw travel",
            Action::TravelBegin => "Begin travel",
            Action::AscendLayer => "Ascend journey layer",
            Action::FocusToggle => "Toggle planet focus",
            Action::ConfirmField => "Confirm field",
        }
    }

    pub const fn group(self) -> ActionGroup {
        match self {
            Action::ToggleLeftDock
            | Action::ToggleRightDock
            | Action::ToggleDevWidget
            | Action::ShowWidgetFps
            | Action::ShowWidgetConsole
            | Action::ShowWidgetInspector
            | Action::UnwindUi => ActionGroup::Chrome,
            Action::NavGameDemo
            | Action::NavDimensions
            | Action::NavSettings
            | Action::SelectDimension(_) => ActionGroup::Navigate,
            Action::WalkNorth
            | Action::WalkSouth
            | Action::WalkWest
            | Action::WalkEast
            | Action::PlayerToggle
            | Action::CameraCycle
            | Action::PresetPerspective
            | Action::PresetTop
            | Action::PresetBottom
            | Action::PresetRight
            | Action::RerollSeed
            | Action::TopDownSnap
            | Action::TwilightCycle
            | Action::ShaderMode(_) => ActionGroup::Camera,
            Action::TravelOffer
            | Action::TravelBegin
            | Action::AscendLayer
            | Action::FocusToggle
            | Action::ConfirmField => ActionGroup::Travel,
        }
    }

    /// Button label: title plus key hint (S5 self-documenting rule).
    pub fn button_label(self) -> String {
        if matches!(self, Action::SelectDimension(i) if digit_for_dimension_index(i).is_none()) {
            return self.title().to_owned();
        }
        match self {
            Action::SelectDimension(i) => {
                let d = digit_for_dimension_index(i).expect("index < 10");
                let key = if d == 0 {
                    "0".to_owned()
                } else {
                    d.to_string()
                };
                let waypoint = DROPDOWN_ORDER[i];
                format!("{} [{key}]", waypoint.name())
            }
            Action::ShaderMode(i) => format!("{} {} [{i}]", self.title(), i + 1),
            _ => format!("{} [{}]", self.title(), self.key_label()),
        }
    }

    /// Every static (non-parameterized) action, for the Controls list
    /// and the parity test.
    pub const ALL_STATIC: [Action; 28] = [
        Action::ToggleLeftDock,
        Action::ToggleRightDock,
        Action::ToggleDevWidget,
        Action::ShowWidgetFps,
        Action::ShowWidgetConsole,
        Action::ShowWidgetInspector,
        Action::UnwindUi,
        Action::NavGameDemo,
        Action::NavDimensions,
        Action::NavSettings,
        Action::WalkNorth,
        Action::WalkSouth,
        Action::WalkWest,
        Action::WalkEast,
        Action::PlayerToggle,
        Action::CameraCycle,
        Action::PresetPerspective,
        Action::PresetTop,
        Action::PresetBottom,
        Action::PresetRight,
        Action::RerollSeed,
        Action::TopDownSnap,
        Action::TwilightCycle,
        Action::TravelOffer,
        Action::TravelBegin,
        Action::AscendLayer,
        Action::FocusToggle,
        Action::ConfirmField,
    ];
}

/// Full registry: static actions plus the parameterized ranges
/// (`SelectDimension` 0–9, `ShaderMode` 0–5). Used by the parity
/// test and the Settings Controls list.
pub fn all_actions() -> Vec<Action> {
    let mut actions: Vec<Action> = Action::ALL_STATIC.to_vec();
    actions.extend((0..DROPDOWN_ORDER.len()).map(Action::SelectDimension));
    actions.extend((0..6).map(Action::ShaderMode));
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropdown_order_covers_all_waypoints_largest_first() {
        assert_eq!(DROPDOWN_ORDER.len(), 10);
        assert_eq!(DROPDOWN_ORDER[0], WaypointId::CosmicWeb);
        assert_eq!(DROPDOWN_ORDER[9], WaypointId::Interior);
        let mut sorted = DROPDOWN_ORDER;
        sorted.sort_by_key(|w| w.number());
        let mut all = WaypointId::ALL;
        all.sort_by_key(|w| w.number());
        assert_eq!(sorted, all);
    }

    #[test]
    fn digits_round_trip_dimension_indices() {
        for i in 0..10 {
            let d = digit_for_dimension_index(i).expect("index < 10");
            assert_eq!(dimension_index_for_digit(d), Some(i));
        }
        assert_eq!(dimension_index_for_digit(10), None);
        assert_eq!(digit_for_dimension_index(10), None);
    }

    #[test]
    fn every_action_has_key_label_title_and_group() {
        for action in all_actions() {
            assert!(!action.key_label().is_empty(), "{action:?}");
            assert!(!action.title().is_empty(), "{action:?}");
            assert!(!action.button_label().is_empty(), "{action:?}");
            let _ = action.group().title();
        }
    }

    #[test]
    fn registry_size_is_pinned() {
        // 28 static + 10 dimensions + 6 shader modes.
        assert_eq!(all_actions().len(), 44);
    }
}
