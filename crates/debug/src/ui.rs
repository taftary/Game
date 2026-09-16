//! Immediate-mode widget logic: layout rects + hit-test/focus/edit
//! primitives. All state lives in caller-owned structs; this module only
//! computes rects and mutates them through explicit events, so every
//! behavior is unit-testable without a window.
//!
//! Layout: a top nav bar, a center viewport, a left dock (per-screen
//! controls) and a right data dock (INPUTS / SELECTION / STATS).
//! The tools window uses the full width content area (no docks) so
//! viewer inputs only ever appear on the viewer window.

/// Top nav bar height, pixels.
pub const NAV_H: f32 = 28.0;
/// Right data dock width, pixels (INPUTS / SELECTION / STATS).
pub const PANEL_W: f32 = 260.0;
/// Left view dock width, pixels (per-screen controls + shader).
pub const LEFT_PANEL_W: f32 = 220.0;
/// Nav button width, pixels.
pub const NAV_BTN_W: f32 = 140.0;
/// Screen-space rect, y-down pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Top-level window regions: nav bar, left dock, center
/// viewport, right data dock (`panel`). The tools window collapses both
/// docks to zero width (see [`layout_full`]) so the content area
/// reclaims the full window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub nav: Rect,
    pub left: Rect,
    pub viewport: Rect,
    pub panel: Rect,
}

/// Split a window into nav bar + left dock + viewport + right dock
/// (viewer-window layout). Degenerate sizes clamp to zero — never
/// negative.
pub fn layout(win_w: f32, win_h: f32) -> Layout {
    layout_viewer(win_w, win_h)
}

/// Sphere Viewer / UV Net layout: left dock + center viewport +
/// right data dock.
pub fn layout_viewer(win_w: f32, win_h: f32) -> Layout {
    let nav_h = NAV_H.min(win_h.max(0.0));
    let w = win_w.max(0.0);
    let left_w = LEFT_PANEL_W.min(w);
    let right_w = PANEL_W.min((w - left_w).max(0.0));
    Layout {
        nav: Rect {
            x: 0.0,
            y: 0.0,
            w,
            h: nav_h,
        },
        left: Rect {
            x: 0.0,
            y: nav_h,
            w: left_w,
            h: (win_h - nav_h).max(0.0),
        },
        viewport: Rect {
            x: left_w,
            y: nav_h,
            w: (w - left_w - right_w).max(0.0),
            h: (win_h - nav_h).max(0.0),
        },
        panel: Rect {
            x: (w - right_w).max(0.0),
            y: nav_h,
            w: right_w,
            h: (win_h - nav_h).max(0.0),
        },
    }
}

/// Full-width layout for the tools window: both docks collapse so
/// the content area (reported as `viewport`) fills the window. This
/// keeps viewer inputs attached to the viewer window only.
pub fn layout_full(win_w: f32, win_h: f32) -> Layout {
    let nav_h = NAV_H.min(win_h.max(0.0));
    let w = win_w.max(0.0);
    Layout {
        nav: Rect {
            x: 0.0,
            y: 0.0,
            w,
            h: nav_h,
        },
        left: Rect {
            x: 0.0,
            y: nav_h,
            w: 0.0,
            h: (win_h - nav_h).max(0.0),
        },
        viewport: Rect {
            x: 0.0,
            y: nav_h,
            w,
            h: (win_h - nav_h).max(0.0),
        },
        panel: Rect {
            x: w,
            y: nav_h,
            w: 0.0,
            h: (win_h - nav_h).max(0.0),
        },
    }
}

/// Nav button rect for item `index` (0-based, left-aligned).
pub fn nav_button(nav: Rect, index: usize) -> Rect {
    Rect {
        x: nav.x + index as f32 * NAV_BTN_W,
        y: nav.y,
        w: NAV_BTN_W,
        h: nav.h,
    }
}

/// Split a row into 2 equal buttons with `gap` between them (2×2
/// camera preset grid). Degenerate widths clamp to zero.
pub fn split_row_2(row: Rect, gap: f32) -> [Rect; 2] {
    let w = ((row.w - gap) / 2.0).max(0.0);
    std::array::from_fn(|i| Rect {
        x: row.x + i as f32 * (w + gap),
        y: row.y,
        w,
        h: row.h,
    })
}

/// Split a panel row into 4 equal buttons with `gap` between them
/// (global camera presets). Degenerate widths clamp to zero.
pub fn split_row_4(row: Rect, gap: f32) -> [Rect; 4] {
    let w = ((row.w - 3.0 * gap) / 4.0).max(0.0);
    std::array::from_fn(|i| Rect {
        x: row.x + i as f32 * (w + gap),
        y: row.y,
        w,
        h: row.h,
    })
}

/// Map F1–F4 (as `1..=4`) to a viewer-window nav index; anything else
/// is `None`.
pub fn nav_index_for_fkey(f: u8) -> Option<usize> {
    (1..=4).contains(&f).then(|| (f - 1) as usize)
}

/// Map `1`–`3` digit keys (as `1..=3`) to a tools-window nav index;
/// anything else is `None`.
pub fn nav_index_for_digit(d: u8) -> Option<usize> {
    (1..=3).contains(&d).then(|| (d - 1) as usize)
}

/// Vertical cursor handing out panel rows.
#[derive(Clone, Copy, Debug)]
pub struct PanelRows {
    pub x: f32,
    pub w: f32,
    y: f32,
}

impl PanelRows {
    pub fn new(panel: Rect, pad: f32) -> Self {
        PanelRows {
            x: panel.x + pad,
            w: (panel.w - 2.0 * pad).max(0.0),
            y: panel.y + pad,
        }
    }

    /// Reserve a row of height `h` (plus `gap` below) and return its rect.
    pub fn next(&mut self, h: f32, gap: f32) -> Rect {
        let row = Rect {
            x: self.x,
            y: self.y,
            w: self.w,
            h,
        };
        self.y += h + gap;
        row
    }
}

/// Single-line text field state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextField {
    pub text: String,
    pub focused: bool,
}

impl TextField {
    pub fn new(text: &str) -> Self {
        TextField {
            text: text.to_owned(),
            focused: false,
        }
    }

    /// Click routing: focus iff the click lands inside `rect`.
    pub fn click(&mut self, rect: Rect, px: f32, py: f32) {
        self.focused = rect.contains(px, py);
    }

    /// Type a char (printable ASCII only; the panel needs digits,
    /// `.`, `-`, `e` for floats).
    pub fn insert_char(&mut self, ch: char) {
        if self.focused && ch.is_ascii_graphic() {
            self.text.push(ch);
        }
    }

    pub fn backspace(&mut self) {
        if self.focused {
            self.text.pop();
        }
    }
}

/// Integer slider state; the track rect maps linearly to `min..=max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slider {
    pub min: u32,
    pub max: u32,
    pub value: u32,
}

impl Slider {
    pub fn new(min: u32, max: u32, value: u32) -> Self {
        Slider {
            min,
            max,
            value: value.clamp(min, max),
        }
    }

    /// Drag handling: clamp the cursor x into the track and snap.
    pub fn drag_to(&mut self, track: Rect, x: f32) {
        let t = ((x - track.x) / track.w).clamp(0.0, 1.0);
        self.value = self.min + ((self.max - self.min) as f32 * t).round() as u32;
    }

    /// Knob center x for the current value.
    pub fn knob_x(&self, track: Rect) -> f32 {
        let span = (self.max - self.min).max(1) as f32;
        track.x + track.w * (self.value - self.min) as f32 / span
    }
}

/// Checkbox state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkbox {
    pub checked: bool,
}

impl Checkbox {
    /// Click routing: toggle iff the click lands inside `rect`.
    pub fn click(&mut self, rect: Rect, px: f32, py: f32) {
        if rect.contains(px, py) {
            self.checked = !self.checked;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_splits_wireframe_regions() {
        let l = layout(1280.0, 720.0);
        assert_eq!(
            l.nav,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1280.0,
                h: NAV_H
            }
        );
        assert_eq!(
            l.left,
            Rect {
                x: 0.0,
                y: NAV_H,
                w: LEFT_PANEL_W,
                h: 720.0 - NAV_H
            }
        );
        assert_eq!(
            l.viewport,
            Rect {
                x: LEFT_PANEL_W,
                y: NAV_H,
                w: 1280.0 - LEFT_PANEL_W - PANEL_W,
                h: 720.0 - NAV_H
            }
        );
        assert_eq!(
            l.panel,
            Rect {
                x: 1280.0 - PANEL_W,
                y: NAV_H,
                w: PANEL_W,
                h: 720.0 - NAV_H
            }
        );
    }

    #[test]
    fn layout_never_goes_negative() {
        let l = layout(100.0, 10.0);
        assert!(l.left.w >= 0.0 && l.left.h >= 0.0);
        assert!(l.viewport.w >= 0.0 && l.viewport.h >= 0.0);
        assert!(l.panel.w >= 0.0 && l.panel.h >= 0.0);
    }

    #[test]
    fn layout_full_reclaims_docks() {
        let l = layout_full(1280.0, 720.0);
        assert_eq!(l.left.w, 0.0);
        assert_eq!(l.panel.w, 0.0);
        assert_eq!(
            l.viewport,
            Rect {
                x: 0.0,
                y: NAV_H,
                w: 1280.0,
                h: 720.0 - NAV_H
            }
        );
    }

    #[test]
    fn nav_buttons_tile_left() {
        let l = layout(1280.0, 720.0);
        let b0 = nav_button(l.nav, 0);
        let b1 = nav_button(l.nav, 1);
        assert_eq!(b0.x, 0.0);
        assert_eq!(b1.x, NAV_BTN_W);
        assert_eq!((b0.y, b0.h), (0.0, NAV_H));
        assert!(b0.contains(10.0, 10.0));
        assert!(!b0.contains(NAV_BTN_W + 1.0, 10.0));
    }

    #[test]
    fn fkeys_map_to_nav() {
        assert_eq!(nav_index_for_fkey(1), Some(0));
        assert_eq!(nav_index_for_fkey(2), Some(1));
        assert_eq!(nav_index_for_fkey(3), Some(2));
        assert_eq!(nav_index_for_fkey(4), Some(3));
        assert_eq!(nav_index_for_fkey(0), None);
        assert_eq!(nav_index_for_fkey(5), None);
    }

    #[test]
    fn digits_map_to_tools_nav() {
        assert_eq!(nav_index_for_digit(1), Some(0));
        assert_eq!(nav_index_for_digit(3), Some(2));
        assert_eq!(nav_index_for_digit(0), None);
        assert_eq!(nav_index_for_digit(4), None);
    }

    #[test]
    fn panel_rows_stack_downward() {
        let l = layout(1280.0, 720.0);
        let mut rows = PanelRows::new(l.panel, 8.0);
        let r0 = rows.next(20.0, 4.0);
        let r1 = rows.next(20.0, 4.0);
        assert_eq!(r0.x, l.panel.x + 8.0);
        assert_eq!(r1.y, r0.y + 24.0);
        assert_eq!(r0.w, PANEL_W - 16.0);
    }

    #[test]
    fn text_field_focus_and_edit() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        };
        let mut f = TextField::new("6");
        f.click(rect, 200.0, 10.0);
        assert!(!f.focused);
        f.insert_char('7');
        assert_eq!(f.text, "6");
        f.click(rect, 10.0, 10.0);
        assert!(f.focused);
        f.insert_char('7');
        f.backspace();
        assert_eq!(f.text, "6");
        f.insert_char('é');
        assert_eq!(f.text, "6");
    }

    #[test]
    fn slider_drags_and_clamps() {
        let track = Rect {
            x: 100.0,
            y: 0.0,
            w: 200.0,
            h: 20.0,
        };
        let mut s = Slider::new(0, 8, 6);
        s.drag_to(track, 100.0);
        assert_eq!(s.value, 0);
        s.drag_to(track, 300.0);
        assert_eq!(s.value, 8);
        s.drag_to(track, -50.0);
        assert_eq!(s.value, 0);
        s.drag_to(track, 500.0);
        assert_eq!(s.value, 8);
        s.drag_to(track, 200.0);
        assert_eq!(s.value, 4);
        assert!((s.knob_x(track) - 200.0).abs() < 1e-4);
    }

    #[test]
    fn preset_grid_splits_rows_into_two() {
        let row = Rect {
            x: 8.0,
            y: 300.0,
            w: 204.0,
            h: 28.0,
        };
        let buttons = split_row_2(row, 6.0);
        assert_eq!(buttons.len(), 2);
        let total = buttons.iter().map(|b| b.w).sum::<f32>() + 6.0;
        assert!((total - row.w).abs() < 1e-3, "{total}");
        assert!(buttons[1].x >= buttons[0].x + buttons[0].w);
    }

    #[test]
    fn preset_row_splits_into_four_inside_buttons() {
        let row = Rect {
            x: 1020.0,
            y: 600.0,
            w: 244.0,
            h: 24.0,
        };
        let buttons = split_row_4(row, 4.0);
        assert_eq!(buttons.len(), 4);
        for (i, b) in buttons.iter().enumerate() {
            assert_eq!((b.y, b.h), (row.y, row.h));
            assert!(b.x >= row.x && b.x + b.w <= row.x + row.w + 1e-3, "{b:?}");
            if i > 0 {
                assert!(b.x >= buttons[i - 1].x + buttons[i - 1].w);
            }
        }
        let total = buttons.iter().map(|b| b.w).sum::<f32>() + 3.0 * 4.0;
        assert!((total - row.w).abs() < 1e-3, "{total}");
    }

    #[test]
    fn checkbox_toggles_on_hit_only() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        };
        let mut c = Checkbox { checked: true };
        c.click(rect, 50.0, 50.0);
        assert!(c.checked);
        c.click(rect, 5.0, 5.0);
        assert!(!c.checked);
    }
}
