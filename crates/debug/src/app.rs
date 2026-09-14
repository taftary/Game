//! App shell: screen navigation + owned screen state. Switching screens
//! only flips [`App::screen`]; the [`SphereViewerState`] (mesh + camera
//! inputs + panel texts) is never reset, so viewer state survives
//! switching by construction.

use crate::sphere_viewer::SphereViewerState;
use crate::{console::LogConsole, fps::FpsOverlay, inspector::StateInspector};

/// Debug screens in nav-bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    SphereViewer,
    Fps,
    Console,
    Inspector,
}

impl Screen {
    /// Nav-bar order; index doubles as the F-key number minus one.
    pub const ALL: [Screen; 4] = [
        Screen::SphereViewer,
        Screen::Fps,
        Screen::Console,
        Screen::Inspector,
    ];

    pub fn index(self) -> usize {
        match self {
            Screen::SphereViewer => 0,
            Screen::Fps => 1,
            Screen::Console => 2,
            Screen::Inspector => 3,
        }
    }

    pub fn from_index(index: usize) -> Option<Screen> {
        Screen::ALL.get(index).copied()
    }

    /// Nav-bar label.
    pub fn title(self) -> &'static str {
        match self {
            Screen::SphereViewer => "Sphere Viewer",
            Screen::Fps => "FPS",
            Screen::Console => "Console",
            Screen::Inspector => "Inspector",
        }
    }

    /// Placeholder body for the not-yet-implemented screens.
    pub fn placeholder_body(self) -> Option<&'static str> {
        match self {
            Screen::SphereViewer => None,
            Screen::Fps | Screen::Console | Screen::Inspector => Some("not implemented yet"),
        }
    }
}

/// Whole debug-tool state.
pub struct App {
    pub screen: Screen,
    pub viewer: SphereViewerState,
    /// Backing state for the FPS placeholder (records nothing yet).
    pub fps: FpsOverlay,
    /// Backing state for the console placeholder (captures nothing yet).
    pub console: LogConsole,
    /// Backing state for the inspector placeholder (inspects nothing yet).
    pub inspector: StateInspector,
}

impl App {
    pub fn new() -> Self {
        App {
            screen: Screen::SphereViewer,
            viewer: SphereViewerState::new(),
            fps: FpsOverlay,
            console: LogConsole,
            inspector: StateInspector,
        }
    }

    /// Switch screens; viewer state is preserved (field never touched).
    pub fn select(&mut self, screen: Screen) {
        self.screen = screen;
    }

    /// F1–F4 routing (`f` is 1–4); returns false for other keys.
    pub fn select_by_fkey(&mut self, f: u8) -> bool {
        match crate::ui::nav_index_for_fkey(f).and_then(Screen::from_index) {
            Some(screen) => {
                self.select(screen);
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
    fn nav_order_matches_fkeys() {
        assert_eq!(Screen::ALL.len(), 4);
        for (i, screen) in Screen::ALL.iter().enumerate() {
            assert_eq!(screen.index(), i);
            assert_eq!(Screen::from_index(i), Some(*screen));
        }
        assert_eq!(Screen::from_index(4), None);
    }

    #[test]
    fn only_viewer_is_functional() {
        assert_eq!(Screen::SphereViewer.placeholder_body(), None);
        for screen in [Screen::Fps, Screen::Console, Screen::Inspector] {
            assert_eq!(screen.placeholder_body(), Some("not implemented yet"));
        }
    }

    #[test]
    fn switching_preserves_viewer_state() {
        let mut app = App::new();
        app.viewer.subdiv_field.text = "4".to_owned();
        app.viewer.wireframe = false;
        app.select(Screen::Console);
        assert_eq!(app.screen, Screen::Console);
        app.select(Screen::SphereViewer);
        assert_eq!(app.viewer.subdiv_field.text, "4");
        assert!(!app.viewer.wireframe);
    }

    #[test]
    fn fkey_routing() {
        let mut app = App::new();
        assert!(app.select_by_fkey(3));
        assert_eq!(app.screen, Screen::Console);
        assert!(app.select_by_fkey(1));
        assert_eq!(app.screen, Screen::SphereViewer);
        assert!(!app.select_by_fkey(9));
        assert_eq!(app.screen, Screen::SphereViewer);
    }

    #[test]
    fn starts_on_viewer() {
        assert_eq!(App::new().screen, Screen::SphereViewer);
    }
}
