//! Immediate-mode widget logic: layout rects + hit-test/focus/edit
//! primitives. All state lives in caller-owned structs; this module only
//! computes rects and mutates them through explicit events, so every
//! behavior is unit-testable without a window.
//!
//! Layout: a top nav bar, a center viewport, a left dock (per-content
//! controls) and a right data dock (INPUTS / SELECTION / STATS).
//! [`layout_unified`] collapses each region independently per the
//! chrome toggles (ADR-022).

/// Top nav bar height, pixels (always visible — ADR-022 polish).
pub const NAV_H: f32 = 32.0;
/// Right data dock width, pixels (INPUTS / SELECTION / STATS).
pub const PANEL_W: f32 = 300.0;
/// Left view dock width, pixels (per-content controls + shader).
pub const LEFT_PANEL_W: f32 = 260.0;
/// Dock content padding, pixels.
pub const DOCK_PAD: f32 = 12.0;
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
/// viewport, right data dock (`panel`). [`layout_unified`] collapses
/// each region to zero width when its chrome toggle is off so the
/// content area reclaims the window.
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

/// Planet View layout: left dock + center viewport + right data dock.
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

/// Unified single-window layout (ADR-022): each dock collapses
/// independently per the chrome toggles; hidden regions report zero
/// size so content reclaims the space. The nav bar is always on.
pub fn layout_unified(win_w: f32, win_h: f32, left_dock: bool, right_dock: bool) -> Layout {
    let nav_h = NAV_H.min(win_h.max(0.0));
    let w = win_w.max(0.0);
    let left_w = if left_dock { LEFT_PANEL_W.min(w) } else { 0.0 };
    let right_w = if right_dock {
        PANEL_W.min((w - left_w).max(0.0))
    } else {
        0.0
    };
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

/// Top-bar item widths: Game Demo | Dimensions breadcrumb | Settings.
pub const TOP_DEMO_W: f32 = 170.0;
pub const TOP_DIM_W: f32 = 320.0;
pub const TOP_SET_W: f32 = 170.0;
/// Top-bar label inset, pixels.
pub const TOP_PAD: f32 = 14.0;

/// Top-bar button rect for item `index` (0 = demo, 1 = dimensions,
/// 2 = settings).
pub fn topbar_button(nav: Rect, index: usize) -> Rect {
    let (x, w) = match index {
        0 => (nav.x, TOP_DEMO_W),
        1 => (nav.x + TOP_DEMO_W, TOP_DIM_W),
        _ => (nav.x + TOP_DEMO_W + TOP_DIM_W, TOP_SET_W),
    };
    Rect {
        x,
        y: nav.y,
        w,
        h: nav.h,
    }
}

/// Dimensions dropdown: opaque panel under the Dimensions item +
/// one padded row per waypoint entry.
pub const DROPDOWN_W: f32 = 360.0;
pub const DROPDOWN_ROW_H: f32 = 30.0;
pub const DROPDOWN_PAD: f32 = 12.0;

pub fn dropdown_panel(nav: Rect) -> Rect {
    Rect {
        x: nav.x + TOP_DEMO_W,
        y: nav.y + nav.h,
        w: DROPDOWN_W,
        h: 10.0 * DROPDOWN_ROW_H + 2.0 * DROPDOWN_PAD,
    }
}

pub fn dropdown_row(panel: Rect, index: usize) -> Rect {
    Rect {
        x: panel.x + DROPDOWN_PAD,
        y: panel.y + DROPDOWN_PAD + index as f32 * DROPDOWN_ROW_H,
        w: (panel.w - 2.0 * DROPDOWN_PAD).max(0.0),
        h: DROPDOWN_ROW_H,
    }
}

/// Transition pill: floating bottom-center overlay, drawn only while
/// a transition is in flight. Overlays content — layout never shifts.
pub const TRANSITION_PILL_W: f32 = 460.0;
pub const TRANSITION_PILL_H: f32 = 26.0;
pub const TRANSITION_PILL_BOTTOM: f32 = 64.0;

pub fn transition_strip(win_w: f32, win_h: f32) -> Rect {
    let w = TRANSITION_PILL_W.min(win_w.max(0.0));
    Rect {
        x: ((win_w - w) / 2.0).max(0.0),
        y: (win_h - TRANSITION_PILL_BOTTOM - TRANSITION_PILL_H).max(0.0),
        w,
        h: TRANSITION_PILL_H,
    }
}

/// Dev widget: fixed bottom-right overlay above the docks.
pub const WIDGET_W: f32 = 400.0;
pub const WIDGET_H: f32 = 280.0;
pub const WIDGET_MARGIN: f32 = 12.0;
pub const WIDGET_TAB_H: f32 = 24.0;

pub fn widget_rect(win_w: f32, win_h: f32) -> Rect {
    Rect {
        x: (win_w - WIDGET_W - WIDGET_MARGIN).max(0.0),
        y: (win_h - WIDGET_H - WIDGET_MARGIN).max(0.0),
        w: WIDGET_W.min(win_w.max(0.0)),
        h: WIDGET_H.min(win_h.max(0.0)),
    }
}

/// Widget sub-tab button (`index` 0–2) in the widget header row.
pub fn widget_tab_button(widget: Rect, index: usize) -> Rect {
    let w = (widget.w / 3.0).max(0.0);
    Rect {
        x: widget.x + index as f32 * w,
        y: widget.y,
        w,
        h: WIDGET_TAB_H,
    }
}

/// Corner strip: chrome toggle buttons living inside the top bar,
/// right-aligned (always visible because the bar is). Three buttons:
/// left dock, right dock, dev widget.
pub const STRIP_BTN_W: f32 = 110.0;
pub const STRIP_BTN_H: f32 = 24.0;

pub fn corner_strip(win_w: f32) -> Rect {
    let w = 3.0 * STRIP_BTN_W;
    Rect {
        x: (win_w - w - DOCK_PAD).max(0.0),
        y: ((NAV_H - STRIP_BTN_H) / 2.0).max(0.0),
        w,
        h: STRIP_BTN_H,
    }
}

/// Corner-strip toggle button (`index` 0–3: tab bar, left dock,
/// right dock, dev widget).
pub fn corner_button(strip: Rect, index: usize) -> Rect {
    Rect {
        x: strip.x + index as f32 * STRIP_BTN_W,
        y: strip.y,
        w: STRIP_BTN_W,
        h: strip.h,
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
    fn layout_unified_matches_viewer_when_all_visible() {
        let l = layout_unified(1280.0, 720.0, true, true);
        assert_eq!(l, layout_viewer(1280.0, 720.0));
    }

    #[test]
    fn layout_unified_reclaims_hidden_chrome() {
        let l = layout_unified(1280.0, 720.0, false, false);
        assert_eq!(l.nav.h, NAV_H);
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
        // Docks visible, full width viewport split.
        let l = layout_unified(1280.0, 720.0, true, true);
        assert_eq!(l.nav.h, NAV_H);
        assert_eq!(l.viewport.x, LEFT_PANEL_W);
        assert_eq!(l.viewport.w, 1280.0 - LEFT_PANEL_W - PANEL_W);
    }

    #[test]
    fn topbar_buttons_tile_without_overlap() {
        let nav = Rect {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: NAV_H,
        };
        let b0 = topbar_button(nav, 0);
        let b1 = topbar_button(nav, 1);
        let b2 = topbar_button(nav, 2);
        assert_eq!((b0.x, b0.w), (0.0, TOP_DEMO_W));
        assert_eq!((b1.x, b1.w), (TOP_DEMO_W, TOP_DIM_W));
        assert_eq!((b2.x, b2.w), (TOP_DEMO_W + TOP_DIM_W, TOP_SET_W));
        assert!(b0.contains(10.0, 10.0));
        assert!(!b0.contains(TOP_DEMO_W + 1.0, 10.0));
    }

    #[test]
    fn dropdown_rows_stack_inside_panel() {
        let nav = Rect {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: NAV_H,
        };
        let panel = dropdown_panel(nav);
        assert_eq!((panel.x, panel.y), (TOP_DEMO_W, NAV_H));
        let r0 = dropdown_row(panel, 0);
        let r9 = dropdown_row(panel, 9);
        assert!(r0.y >= panel.y && r9.y + r9.h <= panel.y + panel.h);
        assert!(r9.y > r0.y);
    }

    #[test]
    fn widget_and_corner_rects_stay_on_screen() {
        let widget = widget_rect(1280.0, 720.0);
        assert!(widget.x + widget.w <= 1280.0 && widget.y + widget.h <= 720.0);
        // Corner strip lives inside the always-visible top bar.
        let strip = corner_strip(1280.0);
        assert!(strip.x + strip.w <= 1280.0);
        assert!(strip.y + strip.h <= NAV_H);
        for i in 0..3 {
            let b = corner_button(strip, i);
            assert!(b.x >= strip.x && b.x + b.w <= strip.x + strip.w + 1e-3);
        }
        for i in 0..3 {
            let t = widget_tab_button(widget, i);
            assert!(t.x >= widget.x && t.x + t.w <= widget.x + widget.w + 1e-3);
        }
    }

    #[test]
    fn transition_pill_floats_bottom_center() {
        let pill = transition_strip(1280.0, 720.0);
        let center = pill.x + pill.w / 2.0;
        assert!((center - 640.0).abs() < 1e-3);
        assert!(pill.y + pill.h <= 720.0);
        assert!(pill.y > 720.0 / 2.0);
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
