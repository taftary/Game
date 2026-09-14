//! UI text: `fontdue` glyph atlas + text layout, CPU side. The windowed
//! binary uploads [`GlyphAtlas::texture`] to a GPU image and re-uploads
//! whenever [`GlyphAtlas::version`] changes (new glyphs only).
//!
//! Screen space is float pixels, y-down. `push_text` takes a baseline y
//! and emits one quad per glyph.

use std::collections::HashMap;

use fontdue::{Font, FontSettings};

/// Vendored UI font (DejaVu Sans 2.37, TrueType outlines — see
/// `assets/fonts/README.md`).
pub const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans.ttf");

/// Atlas start size (grows to [`MAX_ATLAS`] on overflow).
pub const START_ATLAS: u32 = 1024;
/// Atlas size cap; unreachable for a dev-tool glyph set.
pub const MAX_ATLAS: u32 = 2048;

/// One laid-out glyph quad: screen rect + atlas UVs (all y-down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextQuad {
    /// Screen rect `(x0, y0, x1, y1)`, y-down pixels.
    pub pos: [f32; 4],
    /// Atlas UVs `(u0, v0, u1, v1)`; `v0` is the top row in memory.
    pub uv: [f32; 4],
}

/// Cached glyph placement inside the atlas.
#[derive(Clone, Copy, Debug)]
struct GlyphEntry {
    px: u32,
    py: u32,
    w: u32,
    h: u32,
    advance: f32,
    xoff: f32,
    yoff: f32,
}

/// Grayscale glyph atlas with on-demand rasterization.
pub struct GlyphAtlas {
    font: Font,
    px: f32,
    line_height: f32,
    extent: u32,
    texture: Vec<u8>,
    entries: HashMap<char, GlyphEntry>,
    order: Vec<char>,
    cursor_x: u32,
    cursor_y: u32,
    row_h: u32,
    version: u64,
}

impl GlyphAtlas {
    /// Build the atlas and pre-rasterize printable ASCII (32..=126).
    pub fn new(px: f32) -> Self {
        let font =
            Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("vendored font parses");
        let line_height = font
            .horizontal_line_metrics(px)
            .map(|m| m.ascent - m.descent + m.line_gap)
            .unwrap_or(px * 1.2);
        let mut atlas = GlyphAtlas {
            font,
            px,
            line_height,
            extent: START_ATLAS,
            texture: vec![0; (START_ATLAS * START_ATLAS) as usize],
            entries: HashMap::new(),
            order: Vec::new(),
            cursor_x: 1,
            cursor_y: 1,
            row_h: 0,
            version: 0,
        };
        for code in 32u8..=126u8 {
            atlas.glyph(code as char);
        }
        atlas
    }

    /// Raster size in pixels.
    pub fn px(&self) -> f32 {
        self.px
    }

    /// Line step in pixels (ascent − descent + gap).
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Atlas edge length in texels (square).
    pub fn extent(&self) -> u32 {
        self.extent
    }

    /// Grayscale texels, row-major, top row first.
    pub fn texture(&self) -> &[u8] {
        &self.texture
    }

    /// Bumped on every insertion; the binary re-uploads on change.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Count of cached glyphs (tests/diagnostics).
    pub fn rasterized_count(&self) -> usize {
        self.entries.len()
    }

    /// Cached entry, rasterizing on demand. Missing glyphs fall back to
    /// `'?'` (always pre-rasterized); a `'?'` miss skips the char.
    pub fn glyph(&mut self, ch: char) -> Option<GlyphPlacement> {
        if !self.entries.contains_key(&ch) {
            self.insert(ch);
        }
        self.entries.get(&ch).map(|e| self.placement(*e))
    }

    /// `(width, line_height)` of a single-line string (no kerning).
    pub fn measure(&mut self, text: &str) -> (f32, f32) {
        let mut width = 0.0;
        for ch in text.chars() {
            if let Some(g) = self.glyph(ch) {
                width += g.advance;
            }
        }
        (width, self.line_height)
    }

    /// Append one quad per glyph; `baseline` is the y-down baseline.
    pub fn push_text(&mut self, quads: &mut Vec<TextQuad>, text: &str, x: f32, baseline: f32) {
        let mut pen = x;
        for ch in text.chars() {
            if let Some(g) = self.glyph(ch) {
                quads.push(TextQuad {
                    pos: [
                        pen + g.xoff,
                        baseline + g.yoff,
                        pen + g.xoff + g.w,
                        baseline + g.yoff + g.h,
                    ],
                    uv: g.uv,
                });
                pen += g.advance;
            }
        }
    }

    fn placement(&self, e: GlyphEntry) -> GlyphPlacement {
        let s = self.extent as f32;
        GlyphPlacement {
            uv: [
                e.px as f32 / s,
                e.py as f32 / s,
                (e.px + e.w) as f32 / s,
                (e.py + e.h) as f32 / s,
            ],
            advance: e.advance,
            xoff: e.xoff,
            yoff: e.yoff,
            w: e.w as f32,
            h: e.h as f32,
        }
    }

    fn insert(&mut self, ch: char) {
        let (metrics, bitmap) = self.font.rasterize(ch, self.px);
        // Zero-size = blank (space) or missing glyph. Blanks keep their
        // advance with an empty rect; the rest fall back to '?'.
        if metrics.width == 0 || metrics.height == 0 {
            if ch.is_whitespace() {
                self.entries.insert(
                    ch,
                    GlyphEntry {
                        px: 0,
                        py: 0,
                        w: 0,
                        h: 0,
                        advance: metrics.advance_width,
                        xoff: 0.0,
                        yoff: 0.0,
                    },
                );
                self.order.push(ch);
                self.version += 1;
            } else if ch != '?' {
                self.insert('?');
                let entry = self.entries[&'?'];
                self.entries.insert(ch, entry);
            }
            return;
        }
        let w = metrics.width as u32;
        let h = metrics.height as u32;
        let advance = metrics.advance_width;
        // fontdue bounds are y-up around the baseline: the bitmap top
        // sits at `ymin + height` above the baseline.
        let xoff = metrics.bounds.xmin;
        let yoff = -(metrics.bounds.ymin + metrics.bounds.height);
        if !self.fits(w, h) {
            self.grow();
        }
        let (px, py) = (self.cursor_x, self.cursor_y);
        self.blit(&bitmap, px, py, w, h);
        self.cursor_x += w + 1;
        self.row_h = self.row_h.max(h);
        self.entries.insert(
            ch,
            GlyphEntry {
                px,
                py,
                w,
                h,
                advance,
                xoff,
                yoff,
            },
        );
        self.order.push(ch);
        self.version += 1;
    }

    fn fits(&mut self, w: u32, h: u32) -> bool {
        if self.cursor_x + w + 1 > self.extent {
            self.cursor_x = 1;
            self.cursor_y += self.row_h + 1;
            self.row_h = 0;
        }
        self.cursor_y + h < self.extent
    }

    fn grow(&mut self) {
        if self.extent >= MAX_ATLAS {
            return;
        }
        self.extent = (self.extent * 2).min(MAX_ATLAS);
        self.texture = vec![0; (self.extent * self.extent) as usize];
        self.entries.clear();
        let order = std::mem::take(&mut self.order);
        self.cursor_x = 1;
        self.cursor_y = 1;
        self.row_h = 0;
        for ch in order {
            if !self.entries.contains_key(&ch) {
                self.insert(ch);
            }
        }
        self.version += 1;
    }

    fn blit(&mut self, bitmap: &[u8], px: u32, py: u32, w: u32, h: u32) {
        let stride = self.extent as usize;
        for row in 0..h as usize {
            let dst = ((py as usize + row) * stride + px as usize)..;
            let src = row * w as usize..(row + 1) * w as usize;
            self.texture[dst]
                .iter_mut()
                .zip(&bitmap[src])
                .for_each(|(d, s)| *d = *s);
        }
    }
}

/// Screen-space placement of one cached glyph.
#[derive(Clone, Copy, Debug)]
pub struct GlyphPlacement {
    pub uv: [f32; 4],
    pub advance: f32,
    pub xoff: f32,
    pub yoff: f32,
    pub w: f32,
    pub h: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_font_loads_with_ascii() {
        let atlas = GlyphAtlas::new(16.0);
        assert_eq!(atlas.rasterized_count(), 95);
        assert_eq!(atlas.extent(), START_ATLAS);
        assert!(atlas.line_height() > 16.0);
    }

    #[test]
    fn glyph_metrics_are_sane() {
        let mut atlas = GlyphAtlas::new(16.0);
        let a = atlas.glyph('A').unwrap();
        assert!(a.w > 0.0 && a.h > 0.0 && a.advance > 0.0);
        assert!(a.uv[0] < a.uv[2] && a.uv[1] < a.uv[3]);
        assert!(a.uv.iter().all(|&v| (0.0..=1.0).contains(&v)));
        // Descender sits below the baseline; cap-top sits on/above it.
        let g = atlas.glyph('g').unwrap();
        assert!(g.yoff + g.h > 0.0);
    }

    #[test]
    fn bitmaps_are_nonempty() {
        let atlas = GlyphAtlas::new(16.0);
        assert!(atlas.texture().iter().any(|&b| b > 0));
    }

    #[test]
    fn space_advances_without_ink() {
        let mut atlas = GlyphAtlas::new(16.0);
        let space = atlas.glyph(' ').unwrap();
        assert!(space.advance > 0.0);
        assert_eq!((space.w, space.h), (0.0, 0.0));
        let (narrow, _) = atlas.measure("AB");
        let (wide, _) = atlas.measure("A B");
        assert!(wide > narrow);
    }

    #[test]
    fn unmapped_char_still_yields_valid_placement() {
        // fontdue synthesizes the font's .notdef box for unmapped chars;
        // either way the atlas must hand back in-range UVs, never None.
        let mut atlas = GlyphAtlas::new(16.0);
        let g = atlas.glyph('\u{10FFFF}').unwrap();
        assert!(g.uv.iter().all(|&v| (0.0..=1.0).contains(&v)));
        assert!(g.w >= 0.0 && g.h >= 0.0);
    }

    #[test]
    fn en_dash_in_hints_rasterizes() {
        // Hint strings use '–'; it must produce ink, not '?'.
        let mut atlas = GlyphAtlas::new(16.0);
        let dash = atlas.glyph('–').unwrap();
        let q = atlas.glyph('?').unwrap();
        assert_ne!(dash.uv, q.uv);
    }

    #[test]
    fn measure_accumulates_advances() {
        let mut atlas = GlyphAtlas::new(16.0);
        let (w, h) = atlas.measure("AB");
        let (wa, _) = atlas.measure("A");
        let (wb, _) = atlas.measure("B");
        assert!((w - (wa + wb)).abs() < 1e-4);
        assert_eq!(h, atlas.line_height());
    }

    #[test]
    fn push_text_emits_monotonic_quads() {
        let mut atlas = GlyphAtlas::new(16.0);
        let mut quads = Vec::new();
        atlas.push_text(&mut quads, "Hi!", 10.0, 100.0);
        assert_eq!(quads.len(), 3);
        for window in quads.windows(2) {
            assert!(window[0].pos[0] <= window[1].pos[0]);
        }
        for q in &quads {
            assert!(q.pos[0] >= 10.0 && q.pos[2] > q.pos[0]);
        }
    }

    #[test]
    fn new_glyph_bumps_version() {
        let mut atlas = GlyphAtlas::new(16.0);
        let v0 = atlas.version();
        atlas.glyph('–');
        assert!(atlas.version() > v0);
        let v1 = atlas.version();
        atlas.glyph('–');
        assert_eq!(atlas.version(), v1);
    }
}
