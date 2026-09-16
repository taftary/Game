//! App shell: per-window screen navigation + owned screen state.
//! Switching screens only flips the screen field; the
//! [`SphereViewerState`] (mesh + camera inputs + panel texts) is never
//! reset, so viewer state survives switching by construction.
//!
//! Two OS windows, one screen enum each: [`MainScreen`] (sphere / UV
//! net, `F1`/`F2`) on the viewer window, [`ToolsScreen`] (FPS / console
//! / inspector, window-local `1/2/3`) on the tools window.

use crate::sphere_viewer::SphereViewerState;
use crate::{console::LogConsole, fps::FpsOverlay, inspector::StateInspector};

/// Viewer-window screens in nav-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainScreen {
    SphereViewer,
    UvNet,
}

impl MainScreen {
    /// Nav-bar order; index doubles as the F-key number minus one.
    pub const ALL: [MainScreen; 2] = [MainScreen::SphereViewer, MainScreen::UvNet];

    pub fn index(self) -> usize {
        match self {
            MainScreen::SphereViewer => 0,
            MainScreen::UvNet => 1,
        }
    }

    pub fn from_index(index: usize) -> Option<MainScreen> {
        MainScreen::ALL.get(index).copied()
    }

    /// Nav-bar label.
    pub fn title(self) -> &'static str {
        match self {
            MainScreen::SphereViewer => "Sphere Viewer",
            MainScreen::UvNet => "UV Net",
        }
    }
}

/// Tools-window screens in nav-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolsScreen {
    Fps,
    Console,
    Inspector,
}

impl ToolsScreen {
    /// Nav-bar order; index doubles as the digit key minus one.
    pub const ALL: [ToolsScreen; 3] = [
        ToolsScreen::Fps,
        ToolsScreen::Console,
        ToolsScreen::Inspector,
    ];

    pub fn index(self) -> usize {
        match self {
            ToolsScreen::Fps => 0,
            ToolsScreen::Console => 1,
            ToolsScreen::Inspector => 2,
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
        }
    }
}

/// Whole debug-tool state.
pub struct App {
    pub main_screen: MainScreen,
    pub tools_screen: ToolsScreen,
    pub viewer: SphereViewerState,
    /// Frame-health recorder (fed once per event-loop iteration).
    pub fps: FpsOverlay,
    /// Backing state for the console placeholder (captures nothing yet).
    pub console: LogConsole,
    /// Backing state for the inspector placeholder (inspects nothing yet).
    pub inspector: StateInspector,
}

impl App {
    pub fn new() -> Self {
        App::with_viewer(SphereViewerState::new())
    }

    /// App with an explicit viewer state (e.g. the windowed default).
    pub fn with_viewer(viewer: SphereViewerState) -> Self {
        App {
            main_screen: MainScreen::SphereViewer,
            tools_screen: ToolsScreen::Fps,
            viewer,
            fps: FpsOverlay::new(),
            console: LogConsole,
            inspector: StateInspector,
        }
    }

    /// Switch viewer screens; viewer state is preserved (field never touched).
    pub fn select_main(&mut self, screen: MainScreen) {
        self.main_screen = screen;
    }

    /// Switch tools screens.
    pub fn select_tools(&mut self, screen: ToolsScreen) {
        self.tools_screen = screen;
    }

    /// F1–F2 routing (`f` is 1–2); returns false for other keys.
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
        assert_eq!(MainScreen::ALL.len(), 2);
        for (i, screen) in MainScreen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), i);
            assert_eq!(MainScreen::from_index(i), Some(*screen));
        }
        assert_eq!(MainScreen::from_index(2), None);
    }

    #[test]
    fn tools_nav_order_matches_digits() {
        assert_eq!(ToolsScreen::ALL.len(), 3);
        for (i, screen) in ToolsScreen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), i);
            assert_eq!(ToolsScreen::from_index(i), Some(*screen));
        }
        assert_eq!(ToolsScreen::from_index(3), None);
    }

    #[test]
    fn switching_preserves_viewer_state() {
        let mut app = App::new();
        app.viewer.subdiv_field.text = "4".to_owned();
        app.viewer.wireframe = false;
        app.select_main(MainScreen::UvNet);
        assert_eq!(app.main_screen, MainScreen::UvNet);
        app.select_main(MainScreen::SphereViewer);
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
        assert!(app.select_main_by_fkey(2));
        assert_eq!(app.main_screen, MainScreen::UvNet);
        assert!(app.select_main_by_fkey(1));
        assert_eq!(app.main_screen, MainScreen::SphereViewer);
        assert!(!app.select_main_by_fkey(3));
        assert_eq!(app.main_screen, MainScreen::SphereViewer);
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
        assert!(!app.select_tools_by_digit(4));
        assert_eq!(app.tools_screen, ToolsScreen::Fps);
    }

    #[test]
    fn starts_on_viewer_and_fps() {
        assert_eq!(App::new().main_screen, MainScreen::SphereViewer);
        assert_eq!(App::new().tools_screen, ToolsScreen::Fps);
    }
}
