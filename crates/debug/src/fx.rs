//! Transition FX + notice banner (UMAP-017): every layer swap fades,
//! every outcome surfaces — a map-data miss never hard-crashes.
//!
//! Wall-clock lives ONLY here, as animation timing: no descriptor, hash,
//! or save ever sees an `Instant`. The envelope math ([`fade_alpha`],
//! [`notice_visible`]) is pure and unit-tested; [`FxState`] just stamps
//! `Instant::now`.
//!
//! Photosensitivity (`docs/game/controls.md`): the fade is a single
//! black ramp — no flash-white, no strobe, no oscillation.
//!
//! Staged universe loads (v0.3.2 `settings-seed-loader`) run one
//! [`crate::loader::LoadStep`] per frame behind a modal determinate
//! progress bar, so there is no async gap to cover — the bar reports
//! honest per-step progress on the UI thread. A true async spinner
//! still belongs to M2 descent streaming, where chunk loads can miss
//! budget. The notice banner remains the fallback surface for
//! outcomes: invalid seeds, offers, and arrivals all land on it
//! instead of crashing or failing silently.

use std::time::{Duration, Instant};

/// Fade length in ms: one black ramp over the fresh layer.
pub const FADE_MS: u64 = 300;

/// Notice banner lifetime in ms.
pub const NOTICE_MS: u64 = 2_500;

/// Fade alpha for `elapsed` since the swap: 1 → 0 linear, clamped.
pub fn fade_alpha(elapsed: Duration) -> f32 {
    1.0 - elapsed.as_millis().min(u128::from(FADE_MS)) as f32 / FADE_MS as f32
}

/// Whether a notice posted `elapsed` ago is still visible.
pub fn notice_visible(elapsed: Duration) -> bool {
    elapsed < Duration::from_millis(NOTICE_MS)
}

/// Per-app FX state: one pending fade + one notice banner.
#[derive(Clone, Debug, Default)]
pub struct FxState {
    fade_start: Option<Instant>,
    notice: Option<(String, Instant)>,
}

impl FxState {
    /// Start the black ramp over the incoming layer.
    pub fn trigger_fade(&mut self) {
        self.fade_start = Some(Instant::now());
    }

    /// Post a banner (replaces any live one).
    pub fn notify(&mut self, text: String) {
        self.notice = Some((text, Instant::now()));
    }

    /// Current fade alpha (0 once expired or never triggered).
    pub fn fade_alpha(&self) -> f32 {
        self.fade_start.map_or(0.0, |t| fade_alpha(t.elapsed()))
    }

    /// Current banner text, if still live.
    pub fn notice_text(&self) -> Option<&str> {
        self.notice.as_ref().and_then(|(text, t)| {
            if notice_visible(t.elapsed()) {
                Some(text.as_str())
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_envelope_ramps_once() {
        assert_eq!(fade_alpha(Duration::ZERO), 1.0);
        assert_eq!(fade_alpha(Duration::from_millis(150)), 0.5);
        assert_eq!(fade_alpha(Duration::from_millis(300)), 0.0);
        assert_eq!(fade_alpha(Duration::from_secs(10)), 0.0);
    }

    #[test]
    fn notice_lifetime() {
        assert!(notice_visible(Duration::ZERO));
        assert!(notice_visible(Duration::from_millis(NOTICE_MS - 1)));
        assert!(!notice_visible(Duration::from_millis(NOTICE_MS)));
        assert!(!notice_visible(Duration::from_secs(60)));
    }

    #[test]
    fn state_triggers_and_reads_back() {
        let mut fx = FxState::default();
        assert_eq!(fx.fade_alpha(), 0.0);
        assert_eq!(fx.notice_text(), None);
        fx.trigger_fade();
        assert!(fx.fade_alpha() > 0.99);
        fx.notify("Seed 77".to_owned());
        assert_eq!(fx.notice_text(), Some("Seed 77"));
        fx.notify("System star 2".to_owned());
        assert_eq!(fx.notice_text(), Some("System star 2"));
    }
}
