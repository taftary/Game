//! App shell: per-window screen navigation + owned screen state.
//! Switching screens only flips the screen field; the
//! [`PlanetViewerState`] (mesh + camera inputs + panel texts) is never
//! reset, so viewer state survives switching by construction.
//!
//! Two OS windows, one screen enum each: [`MainScreen`] (galaxy /
//! system / planet, `F1`/`F2`/`F3`) on the viewer window,
//! [`ToolsScreen`] (FPS / console / inspector, window-local `1/2/3`)
//! on the tools window.

use crate::fx::FxState;
use crate::galaxy_map::{DEFAULT_GALAXY_SEED, GalaxyMapView};
use crate::planet_viewer::PlanetViewerState;
use crate::scale_debug::ScaleDebugState;
use crate::system_map::{OrbitArrival, SystemMapView};
use crate::transitions::TransitionPanel;
use crate::{console::LogConsole, fps::FpsOverlay, inspector::StateInspector};
use game::journey::Journey;

/// Viewer-window screens in nav-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainScreen {
    GalaxyMap,
    SystemMap,
    PlanetView,
}

impl MainScreen {
    /// Nav-bar order; index doubles as the F-key number minus one.
    pub const ALL: [MainScreen; 3] = [
        MainScreen::GalaxyMap,
        MainScreen::SystemMap,
        MainScreen::PlanetView,
    ];

    pub fn index(self) -> usize {
        match self {
            MainScreen::GalaxyMap => 0,
            MainScreen::SystemMap => 1,
            MainScreen::PlanetView => 2,
        }
    }

    pub fn from_index(index: usize) -> Option<MainScreen> {
        MainScreen::ALL.get(index).copied()
    }

    /// Nav-bar label.
    pub fn title(self) -> &'static str {
        match self {
            MainScreen::GalaxyMap => "Galaxy Map",
            MainScreen::SystemMap => "System Map",
            MainScreen::PlanetView => "Planet View",
        }
    }
}

/// Tools-window screens in nav-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolsScreen {
    Fps,
    Console,
    Inspector,
    Transitions,
    Scale,
}

impl ToolsScreen {
    /// Nav-bar order; index doubles as the digit key minus one.
    pub const ALL: [ToolsScreen; 5] = [
        ToolsScreen::Fps,
        ToolsScreen::Console,
        ToolsScreen::Inspector,
        ToolsScreen::Transitions,
        ToolsScreen::Scale,
    ];

    pub fn index(self) -> usize {
        match self {
            ToolsScreen::Fps => 0,
            ToolsScreen::Console => 1,
            ToolsScreen::Inspector => 2,
            ToolsScreen::Transitions => 3,
            ToolsScreen::Scale => 4,
        }
    }

    pub fn from_index(index: usize) -> Option<ToolsScreen> {
        ToolsScreen::ALL.get(index).copied()
    }

    /// Nav-bar label.
    pub fn title(self) -> &'static str {
        match self {
            ToolsScreen::Fps => "FPS",
            ToolsScreen::Console => "Console",
            ToolsScreen::Inspector => "Inspector",
            ToolsScreen::Transitions => "Transitions",
            ToolsScreen::Scale => "Scale",
        }
    }
}

/// Whole debug-tool state.
pub struct App {
    pub main_screen: MainScreen,
    pub tools_screen: ToolsScreen,
    pub viewer: PlanetViewerState,
    /// Galaxy-map screen state (camera + selection survive switching,
    /// same as the viewer state).
    pub galaxy: GalaxyMapView,
    /// System-map screen state (loaded system + focus + selection).
    pub system: SystemMapView,
    /// Journey machine both map screens drive (UMAP-015): selections
    /// arm it, E/T/Q commit travel and layer changes.
    pub journey: Journey,
    /// Transition fades + notice banner (UMAP-017).
    pub fx: FxState,
    /// Frame-health recorder (fed once per event-loop iteration).
    pub fps: FpsOverlay,
    /// Backing state for the console placeholder (captures nothing yet).
    pub console: LogConsole,
    /// Backing state for the inspector placeholder (inspects nothing yet).
    pub inspector: StateInspector,
    pub transitions: TransitionPanel,
    pub scale: ScaleDebugState,
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
            main_screen: MainScreen::GalaxyMap,
            tools_screen: ToolsScreen::Fps,
            viewer,
            galaxy,
            system,
            journey: Journey::new(DEFAULT_GALAXY_SEED),
            fx: FxState::default(),
            fps: FpsOverlay::new(),
            console: LogConsole,
            inspector: StateInspector,
            transitions: {
                let mut panel = TransitionPanel::new();
                panel.preview();
                panel
            },
            scale: ScaleDebugState::new(),
        }
    }

    /// Bind an orbit arrival (UMAP-020): the planet view rebuilds at
    /// the descriptor radius (subdivisions unchanged) holding the
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

    /// Switch viewer screens; viewer state is preserved (field never touched).
    pub fn select_main(&mut self, screen: MainScreen) {
        self.main_screen = screen;
    }

    /// Switch tools screens.
    pub fn select_tools(&mut self, screen: ToolsScreen) {
        self.tools_screen = screen;
    }

    /// F1–F3 routing (`f` is 1–3); returns false for other keys.
    pub fn select_main_by_fkey(&mut self, f: u8) -> bool {
        match crate::ui::nav_index_for_fkey(f).and_then(MainScreen::from_index) {
            Some(screen) => {
                self.select_main(screen);
                true
            }
            None => false,
        }
    }

    /// `1`–`3` routing (`d` is 1–3); returns false for other keys.
    pub fn select_tools_by_digit(&mut self, d: u8) -> bool {
        match crate::ui::nav_index_for_digit(d).and_then(ToolsScreen::from_index) {
            Some(screen) => {
                self.select_tools(screen);
                true
            }
            None => false,
        }
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
    fn main_nav_order_matches_fkeys() {
        assert_eq!(MainScreen::ALL.len(), 3);
        for (i, screen) in MainScreen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), i);
            assert_eq!(MainScreen::from_index(i), Some(*screen));
        }
        assert_eq!(MainScreen::from_index(3), None);
    }

    #[test]
    fn tools_nav_order_matches_digits() {
        assert_eq!(ToolsScreen::ALL.len(), 5);
        for (i, screen) in ToolsScreen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), i);
            assert_eq!(ToolsScreen::from_index(i), Some(*screen));
        }
        assert_eq!(ToolsScreen::from_index(4), Some(ToolsScreen::Scale));
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
        app.select_main(MainScreen::SystemMap);
        assert_eq!(app.main_screen, MainScreen::SystemMap);
        app.select_main(MainScreen::PlanetView);
        assert_eq!(app.viewer.subdiv_field.text, "4");
        assert!(!app.viewer.wireframe);
        app.select_tools(ToolsScreen::Console);
        assert_eq!(app.tools_screen, ToolsScreen::Console);
        app.select_tools(ToolsScreen::Fps);
        assert_eq!(app.viewer.subdiv_field.text, "4");
    }

    #[test]
    fn fkey_routing() {
        let mut app = App::new();
        assert!(app.select_main_by_fkey(1));
        assert_eq!(app.main_screen, MainScreen::GalaxyMap);
        assert!(app.select_main_by_fkey(2));
        assert_eq!(app.main_screen, MainScreen::SystemMap);
        assert!(app.select_main_by_fkey(3));
        assert_eq!(app.main_screen, MainScreen::PlanetView);
        assert!(!app.select_main_by_fkey(4));
        assert_eq!(app.main_screen, MainScreen::PlanetView);
    }

    #[test]
    fn digit_routing() {
        let mut app = App::new();
        assert!(app.select_tools_by_digit(2));
        assert_eq!(app.tools_screen, ToolsScreen::Console);
        assert!(app.select_tools_by_digit(3));
        assert_eq!(app.tools_screen, ToolsScreen::Inspector);
        assert!(app.select_tools_by_digit(1));
        assert_eq!(app.tools_screen, ToolsScreen::Fps);
        assert!(app.select_tools_by_digit(4));
        assert_eq!(app.tools_screen, ToolsScreen::Transitions);
        assert!(app.select_tools_by_digit(5));
        assert_eq!(app.tools_screen, ToolsScreen::Scale);
    }

    #[test]
    fn starts_on_galaxy_and_fps() {
        assert_eq!(App::new().main_screen, MainScreen::GalaxyMap);
        assert_eq!(App::new().tools_screen, ToolsScreen::Fps);
    }
}
