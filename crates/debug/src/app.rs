//! App shell: single-window screen navigation + owned screen state.
//! Switching screens only flips the [`Screen`] field; content state
//! (viewer, galaxy, system, journey) is never reset, so state
//! survives switching by construction (ADR-022).
//!
//! One OS window, one [`Screen`]: [`Screen::GameDemo`] (default
//! landing tab), [`Screen::Dimensions`] (one of the ten waypoints,
//! picked from the dropdown), [`Screen::Settings`]. The top bar is
//! always visible; docks and the dev widget toggle independently.

use crate::actions::{DROPDOWN_ORDER, dimension_index_for_digit};
use crate::fx::FxState;
use crate::galaxy_map::{DEFAULT_GALAXY_SEED, GalaxyMapView};
use crate::planet_viewer::PlanetViewerState;
use crate::system_map::{OrbitArrival, SystemMapView};
use crate::transitions::TransitionPanel;
use crate::{console::LogConsole, fps::FpsOverlay, inspector::StateInspector};
use game::journey::{Journey, Layer};
use game_engine::waypoints::WaypointId;

/// Top-level screens in nav-bar order (`F1`–`F3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    GameDemo,
    Dimensions(WaypointId),
    Settings,
}

impl Screen {
    /// F-key number (1–3) for the top-level item.
    pub const fn fkey(self) -> u8 {
        match self {
            Screen::GameDemo => 1,
            Screen::Dimensions(_) => 2,
            Screen::Settings => 3,
        }
    }

    /// Top bar label; the Dimensions label carries the live
    /// breadcrumb (S1: the active selection is always visible).
    pub fn title(self, active: WaypointId) -> String {
        match self {
            Screen::GameDemo => "GAME DEMO".to_owned(),
            Screen::Dimensions(selected) => {
                let mark = if selected == active { " ●" } else { "" };
                format!("DIMENSIONS: {}{mark}", selected.name())
            }
            Screen::Settings => "SETTINGS".to_owned(),
        }
    }
}

/// 3D content mounted in the viewport (`None` = flat UI only).
/// Replaces the old viewer-window screen enum: the same three
/// renders, now addressed by the unified [`Screen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewContent {
    GalaxyMap,
    SystemMap,
    PlanetView,
}

/// Dev-widget sub-tabs in order (`F6`–`F8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetTab {
    Fps,
    Console,
    Inspector,
}

impl WidgetTab {
    pub const ALL: [WidgetTab; 3] = [WidgetTab::Fps, WidgetTab::Console, WidgetTab::Inspector];

    pub const fn index(self) -> usize {
        match self {
            WidgetTab::Fps => 0,
            WidgetTab::Console => 1,
            WidgetTab::Inspector => 2,
        }
    }

    pub const fn from_index(index: usize) -> Option<WidgetTab> {
        match index {
            0 => Some(WidgetTab::Fps),
            1 => Some(WidgetTab::Console),
            2 => Some(WidgetTab::Inspector),
            _ => None,
        }
    }

    /// F-key selecting this sub-tab (also opens the widget).
    pub const fn fkey(self) -> u8 {
        match self {
            WidgetTab::Fps => 6,
            WidgetTab::Console => 7,
            WidgetTab::Inspector => 8,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            WidgetTab::Fps => "FPS",
            WidgetTab::Console => "Console",
            WidgetTab::Inspector => "Inspector",
        }
    }
}

/// Independent dock visibility: left dock, right dock. The top bar
/// is always visible (no toggle); the dev widget has its own
/// visibility on [`App`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeState {
    pub left_dock: bool,
    pub right_dock: bool,
}

impl ChromeState {
    pub const fn all_visible() -> Self {
        ChromeState {
            left_dock: true,
            right_dock: true,
        }
    }

    pub const fn all_hidden() -> Self {
        ChromeState {
            left_dock: false,
            right_dock: false,
        }
    }
}

/// Whole debug-tool state.
pub struct App {
    pub screen: Screen,
    pub chrome: ChromeState,
    /// Dev-widget overlay: visible on every tab when set.
    pub widget_visible: bool,
    pub widget_tab: WidgetTab,
    /// Click-to-focus for the widget (keyboard `Esc` unwinds it).
    pub widget_focused: bool,
    /// Dimensions dropdown open (captures digit keys).
    pub dropdown_open: bool,
    pub viewer: PlanetViewerState,
    /// Galaxy-map screen state (camera + selection survive switching,
    /// same as the viewer state).
    pub galaxy: GalaxyMapView,
    /// System-map screen state (loaded system + focus + selection).
    pub system: SystemMapView,
    /// Journey machine the demo tab and map screens drive:
    /// selections arm it, E/T/Q commit travel and layer changes.
    pub journey: Journey,
    /// Transition fades + notice banner.
    pub fx: FxState,
    /// Frame-health recorder (fed once per event-loop iteration).
    pub fps: FpsOverlay,
    /// Console log feed (transition events drained per frame).
    pub console: LogConsole,
    /// Backing state for the inspector (journey summary).
    pub inspector: StateInspector,
    pub transitions: TransitionPanel,
}

impl App {
    pub fn new() -> Self {
        App::with_viewer(PlanetViewerState::new())
    }

    /// App with an explicit viewer state (e.g. the windowed default).
    pub fn with_viewer(viewer: PlanetViewerState) -> Self {
        let galaxy = GalaxyMapView::new(DEFAULT_GALAXY_SEED);
        // The system screen opens on star 0 of the same galaxy (always
        // present at the v1 default count).
        let star0 = galaxy.galaxy.stars[0].clone();
        let system = SystemMapView::new(DEFAULT_GALAXY_SEED, &star0);
        App {
            screen: Screen::GameDemo,
            chrome: ChromeState::all_hidden(),
            widget_visible: false,
            widget_tab: WidgetTab::Fps,
            widget_focused: false,
            dropdown_open: false,
            viewer,
            galaxy,
            system,
            journey: Journey::new(DEFAULT_GALAXY_SEED),
            fx: FxState::default(),
            fps: FpsOverlay::new(),
            console: LogConsole::new(),
            inspector: StateInspector,
            transitions: {
                let mut panel = TransitionPanel::new();
                panel.preview();
                panel
            },
        }
    }

    /// Bind an orbit arrival: the planet view rebuilds at the
    /// descriptor radius (subdivisions unchanged) holding the
    /// arrival tint + seeded planet. Returns the applied radius for the
    /// arrival notice. Generator radii always validate, so the rebuild
    /// cannot fail — a manual regenerate later clears the binding.
    pub fn arrive(&mut self, arrival: OrbitArrival) -> f32 {
        use crate::ui::TextField;

        let radius = arrival.radius_km;
        self.viewer.radius_field = TextField::new(&radius.to_string());
        let _rebuilt = self.viewer.regenerate();
        self.viewer.arrival = Some(arrival);
        radius
    }

    /// Active waypoint for the current journey layer (breadcrumb +
    /// dropdown marker source).
    pub fn active_waypoint(&self) -> WaypointId {
        match self.journey.active_layer() {
            Layer::Galaxy => WaypointId::MilkyWay,
            Layer::System => WaypointId::SolarSystem,
            Layer::Orbit => WaypointId::Earth,
        }
    }

    /// 3D content for the current screen (`None` = flat UI only).
    /// The demo tab follows the journey layer; absorbed dimension
    /// tabs mount their view; everything else is flat UI.
    pub fn screen_content(&self) -> Option<ViewContent> {
        match self.screen {
            Screen::GameDemo => match self.journey.active_layer() {
                Layer::Galaxy => Some(ViewContent::GalaxyMap),
                Layer::System => Some(ViewContent::SystemMap),
                Layer::Orbit => Some(ViewContent::PlanetView),
            },
            Screen::Dimensions(WaypointId::MilkyWay) => Some(ViewContent::GalaxyMap),
            Screen::Dimensions(WaypointId::SolarSystem) => Some(ViewContent::SystemMap),
            Screen::Dimensions(WaypointId::Earth) => Some(ViewContent::PlanetView),
            Screen::Dimensions(_) | Screen::Settings => None,
        }
    }

    /// Dropdown rows in display order with the active flag.
    pub fn dropdown_rows(&self) -> [(WaypointId, bool); 10] {
        let active = self.active_waypoint();
        DROPDOWN_ORDER.map(|waypoint| (waypoint, waypoint == active))
    }

    /// Switch screens; per-tab chrome defaults apply (the demo tab
    /// opens clean — presentation-accurate with zero debug data).
    /// Content state is preserved (fields never touched).
    pub fn select_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.chrome = match screen {
            Screen::GameDemo => ChromeState::all_hidden(),
            Screen::Dimensions(_) | Screen::Settings => ChromeState::all_visible(),
        };
        self.widget_visible = !matches!(screen, Screen::GameDemo);
        self.dropdown_open = false;
        self.widget_focused = false;
    }

    /// `F1`–`F3` routing; returns false for other keys. `F2` lands on
    /// the active waypoint with the dropdown open.
    pub fn select_top_by_fkey(&mut self, f: u8) -> bool {
        match f {
            1 => {
                self.select_screen(Screen::GameDemo);
                true
            }
            2 => {
                let active = self.active_waypoint();
                self.select_screen(Screen::Dimensions(active));
                self.dropdown_open = true;
                true
            }
            3 => {
                self.select_screen(Screen::Settings);
                true
            }
            _ => false,
        }
    }

    /// Dropdown digit routing (`1`–`9` → entries 0–8, `0` → entry 9);
    /// selects the dimension and closes the dropdown. Returns false
    /// for other digits.
    pub fn select_dimension_by_digit(&mut self, d: u8) -> bool {
        match dimension_index_for_digit(d) {
            Some(index) => match DROPDOWN_ORDER.get(index) {
                Some(&waypoint) => {
                    self.select_screen(Screen::Dimensions(waypoint));
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    pub fn toggle_dropdown(&mut self) {
        self.dropdown_open = !self.dropdown_open;
    }

    pub fn close_dropdown(&mut self) {
        self.dropdown_open = false;
    }

    pub fn toggle_left_dock(&mut self) {
        self.chrome.left_dock = !self.chrome.left_dock;
    }

    pub fn toggle_right_dock(&mut self) {
        self.chrome.right_dock = !self.chrome.right_dock;
    }

    pub fn toggle_widget(&mut self) {
        self.widget_visible = !self.widget_visible;
        if !self.widget_visible {
            self.widget_focused = false;
        }
    }

    /// Show the widget on `tab` (also unfocus-safe: keeps focus as-is).
    pub fn select_widget_tab(&mut self, tab: WidgetTab) {
        self.widget_tab = tab;
        self.widget_visible = true;
    }

    /// `F6`–`F8` routing; returns false for other keys.
    pub fn select_widget_tab_by_fkey(&mut self, f: u8) -> bool {
        match f {
            6 => {
                self.select_widget_tab(WidgetTab::Fps);
                true
            }
            7 => {
                self.select_widget_tab(WidgetTab::Console);
                true
            }
            8 => {
                self.select_widget_tab(WidgetTab::Inspector);
                true
            }
            _ => false,
        }
    }

    /// `Esc` unwind: close the dropdown first, then unfocus the
    /// widget. Returns true when something changed (never quits).
    pub fn esc_unwind(&mut self) -> bool {
        if self.dropdown_open {
            self.dropdown_open = false;
            true
        } else if self.widget_focused {
            self.widget_focused = false;
            true
        } else {
            false
        }
    }

    /// Drain new transition history into the console feed. Returns
    /// lines added.
    pub fn sync_console(&mut self) -> usize {
        self.console.feed_transitions(&self.transitions.history)
    }
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fkey_routing() {
        let mut app = App::new();
        assert!(app.select_top_by_fkey(1));
        assert_eq!(app.screen, Screen::GameDemo);
        assert!(app.select_top_by_fkey(3));
        assert_eq!(app.screen, Screen::Settings);
        assert!(app.select_top_by_fkey(2));
        assert_eq!(app.screen, Screen::Dimensions(WaypointId::MilkyWay));
        assert!(app.dropdown_open);
        assert!(!app.select_top_by_fkey(4));
        assert!(!app.select_top_by_fkey(0));
    }

    #[test]
    fn digit_routing_selects_dropdown_entries_and_closes() {
        let mut app = App::new();
        app.dropdown_open = true;
        assert!(app.select_dimension_by_digit(1));
        assert_eq!(app.screen, Screen::Dimensions(WaypointId::CosmicWeb));
        assert!(!app.dropdown_open);
        assert!(app.select_dimension_by_digit(0));
        assert_eq!(app.screen, Screen::Dimensions(WaypointId::Interior));
        assert!(app.select_dimension_by_digit(4));
        assert_eq!(app.screen, Screen::Dimensions(WaypointId::MilkyWay));
        assert!(!app.select_dimension_by_digit(10));
    }

    #[test]
    fn widget_fkey_routing() {
        let mut app = App::new();
        assert!(!app.widget_visible);
        assert!(app.select_widget_tab_by_fkey(7));
        assert!(app.widget_visible);
        assert_eq!(app.widget_tab, WidgetTab::Console);
        assert!(app.select_widget_tab_by_fkey(6));
        assert_eq!(app.widget_tab, WidgetTab::Fps);
        assert!(app.select_widget_tab_by_fkey(8));
        assert_eq!(app.widget_tab, WidgetTab::Inspector);
        assert!(!app.select_widget_tab_by_fkey(9));
    }

    #[test]
    fn widget_tab_order_matches_fkeys() {
        assert_eq!(WidgetTab::ALL.len(), 3);
        for (i, tab) in WidgetTab::ALL.iter().enumerate() {
            assert_eq!(tab.index(), i);
            assert_eq!(WidgetTab::from_index(i), Some(*tab));
        }
        assert_eq!(WidgetTab::from_index(3), None);
        assert_eq!(WidgetTab::Fps.fkey(), 6);
        assert_eq!(WidgetTab::Console.fkey(), 7);
        assert_eq!(WidgetTab::Inspector.fkey(), 8);
    }

    #[test]
    fn esc_unwind_order() {
        let mut app = App::new();
        app.dropdown_open = true;
        app.widget_focused = true;
        assert!(app.esc_unwind());
        assert!(!app.dropdown_open);
        assert!(app.widget_focused);
        assert!(app.esc_unwind());
        assert!(!app.widget_focused);
        assert!(!app.esc_unwind());
    }

    #[test]
    fn toggles_are_independent() {
        let mut app = App::new();
        app.select_screen(Screen::Settings);
        app.toggle_left_dock();
        assert!(!app.chrome.left_dock);
        assert!(app.chrome.right_dock);
        app.toggle_widget();
        assert!(!app.widget_visible);
        app.toggle_widget();
        assert!(app.widget_visible);
        assert!(!app.chrome.left_dock);
        app.toggle_right_dock();
        assert!(!app.chrome.right_dock);
    }

    #[test]
    fn demo_tab_defaults_to_hidden_chrome() {
        let mut app = App::new();
        // Fresh app boots on the demo tab, clean.
        assert_eq!(app.screen, Screen::GameDemo);
        assert_eq!(app.chrome, ChromeState::all_hidden());
        assert!(!app.widget_visible);
        app.select_screen(Screen::Settings);
        assert_eq!(app.chrome, ChromeState::all_visible());
        assert!(app.widget_visible);
        app.select_screen(Screen::GameDemo);
        assert_eq!(app.chrome, ChromeState::all_hidden());
        assert!(!app.widget_visible);
    }

    #[test]
    fn dropdown_rows_mark_exactly_one_active() {
        let app = App::new();
        let rows = app.dropdown_rows();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows.iter().filter(|(_, active)| *active).count(), 1);
        assert_eq!(
            rows.iter().find(|(_, active)| *active).map(|(w, _)| *w),
            Some(WaypointId::MilkyWay)
        );
        assert_eq!(rows[0].0, WaypointId::CosmicWeb);
    }

    #[test]
    fn screen_content_mapping() {
        let mut app = App::new();
        // Demo follows the journey layer (fresh journey = Galaxy).
        assert_eq!(app.screen_content(), Some(ViewContent::GalaxyMap));
        app.select_screen(Screen::Dimensions(WaypointId::SolarSystem));
        assert_eq!(app.screen_content(), Some(ViewContent::SystemMap));
        app.select_screen(Screen::Dimensions(WaypointId::Earth));
        assert_eq!(app.screen_content(), Some(ViewContent::PlanetView));
        app.select_screen(Screen::Dimensions(WaypointId::CosmicWeb));
        assert_eq!(app.screen_content(), None);
        app.select_screen(Screen::Settings);
        assert_eq!(app.screen_content(), None);
    }

    #[test]
    fn arrival_rebuilds_viewer_at_descriptor_radius() {
        use crate::system_map::arrival_for;

        let mut app = App::new();
        let arrival = arrival_for(&app.system.system, 0).expect("planet 0");
        let radius = app.arrive(arrival);
        assert_eq!(radius, app.viewer.radius);
        assert!((2.0..=8.0).contains(&radius));
        let bound = app.viewer.arrival.as_ref().expect("arrival held");
        assert_eq!(bound.radius_km, radius);
        assert_eq!(bound.seeded.radius(), radius);
        // Manual regenerate clears the binding (fresh mesh, no target).
        app.viewer.regenerate().expect("fields valid");
        assert!(app.viewer.arrival.is_none());
    }

    #[test]
    fn switching_preserves_viewer_state() {
        let mut app = App::new();
        app.viewer.subdiv_field.text = "4".to_owned();
        app.viewer.wireframe = false;
        app.select_screen(Screen::Dimensions(WaypointId::SolarSystem));
        assert_eq!(app.screen, Screen::Dimensions(WaypointId::SolarSystem));
        app.select_screen(Screen::GameDemo);
        assert_eq!(app.viewer.subdiv_field.text, "4");
        assert!(!app.viewer.wireframe);
        app.select_widget_tab(WidgetTab::Console);
        assert_eq!(app.widget_tab, WidgetTab::Console);
        app.select_widget_tab(WidgetTab::Fps);
        assert_eq!(app.viewer.subdiv_field.text, "4");
    }

    #[test]
    fn console_sync_drains_transition_history() {
        let mut app = App::new();
        // The fresh panel carries the preview history.
        let history_len = app.transitions.history.len();
        assert!(history_len > 0);
        let added = app.sync_console();
        assert_eq!(added, history_len);
        assert!(!app.console.is_empty());
        // Second sync adds nothing (already drained).
        assert_eq!(app.sync_console(), 0);
    }
}
