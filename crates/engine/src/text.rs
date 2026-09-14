//! CPU-side text data: validated runs for the renderer-neutral pipeline.
//!
//! Layout and rasterization inputs stay CPU-side here; the renderer consumes
//! validated draw data without owning text policy.

use core::fmt;

/// Validated point size of a text run in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontSize(u32);

impl FontSize {
    /// Creates a font size. Rejects zero.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::InvalidFontSize`] when `points` is zero.
    pub fn new(points: u32) -> Result<Self, TextError> {
        if points == 0 {
            return Err(TextError::InvalidFontSize(points));
        }
        Ok(Self(points))
    }

    /// Returns the size in logical pixels.
    #[must_use]
    pub const fn points(self) -> u32 {
        self.0
    }
}

/// CPU-side text run: content plus layout inputs, no rasterized output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    text: String,
    size: FontSize,
}

impl TextRun {
    /// Creates a text run. Rejects empty content.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::EmptyText`] when `text` is empty.
    pub fn new(text: impl Into<String>, size: FontSize) -> Result<Self, TextError> {
        let text = text.into();
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        Ok(Self { text, size })
    }

    /// Returns the run content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the run font size.
    #[must_use]
    pub const fn size(&self) -> FontSize {
        self.size
    }
}

/// Recoverable text validation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    /// Font size must be greater than zero; carries the rejected value.
    InvalidFontSize(u32),
    /// Text content must not be empty.
    EmptyText,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFontSize(points) => write!(f, "invalid font size: {points}"),
            Self::EmptyText => write!(f, "text content must not be empty"),
        }
    }
}

impl std::error::Error for TextError {}
