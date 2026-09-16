//! Frame-health overlay (FPS, frame time, hitch markers).
//!
//! Window- and GPU-free: the binary feeds one sample per event-loop
//! iteration and the tools window reads the aggregates. A fixed ring
//! buffer keeps memory bounded and old samples age out on their own.

use std::collections::VecDeque;

/// Ring-buffer capacity: 240 samples ≈ 4 s at 60 fps.
pub const FPS_SAMPLES: usize = 240;
/// Sparkline resolution on the FPS tab (newest N samples).
pub const FPS_SPARKLINE: usize = 120;

/// Frame-health recorder: ring buffer of per-frame durations.
#[derive(Clone, Debug)]
pub struct FpsOverlay {
    samples: VecDeque<f32>,
}

impl FpsOverlay {
    pub fn new() -> Self {
        FpsOverlay {
            samples: VecDeque::with_capacity(FPS_SAMPLES),
        }
    }

    /// Record one frame (`dt_secs` clamped to finite, non-negative).
    pub fn record(&mut self, dt_secs: f32) {
        if !dt_secs.is_finite() || dt_secs < 0.0 {
            return;
        }
        if self.samples.len() == FPS_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(dt_secs);
    }

    /// Samples currently held.
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// Mean frame rate over the window (0 with no samples).
    pub fn fps(&self) -> f32 {
        let total: f32 = self.samples.iter().sum();
        if total > 0.0 {
            self.samples.len() as f32 / total
        } else {
            0.0
        }
    }

    /// Mean frame time, milliseconds (0 with no samples).
    pub fn avg_ms(&self) -> f32 {
        if self.samples.is_empty() {
            0.0
        } else {
            self.samples.iter().sum::<f32>() / self.samples.len() as f32 * 1000.0
        }
    }

    /// Worst frame time in the window, milliseconds (hitch marker).
    pub fn max_ms(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, &s| m.max(s)) * 1000.0
    }

    /// Newest-first sample slice for the sparkline (up to `n`).
    pub fn recent_ms(&self, n: usize) -> Vec<f32> {
        self.samples
            .iter()
            .rev()
            .take(n)
            .map(|s| s * 1000.0)
            .collect()
    }
}

impl Default for FpsOverlay {
    fn default() -> Self {
        FpsOverlay::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reads_zero() {
        let fps = FpsOverlay::new();
        assert_eq!(fps.count(), 0);
        assert_eq!(fps.fps(), 0.0);
        assert_eq!(fps.avg_ms(), 0.0);
        assert_eq!(fps.max_ms(), 0.0);
        assert!(fps.recent_ms(10).is_empty());
    }

    #[test]
    fn steady_sixty() {
        let mut fps = FpsOverlay::new();
        for _ in 0..60 {
            fps.record(1.0 / 60.0);
        }
        assert_eq!(fps.count(), 60);
        assert!((fps.fps() - 60.0).abs() < 0.01);
        assert!((fps.avg_ms() - 1000.0 / 60.0).abs() < 0.01);
        assert!((fps.max_ms() - 1000.0 / 60.0).abs() < 0.01);
    }

    #[test]
    fn hitch_drives_max_not_mean() {
        let mut fps = FpsOverlay::new();
        for _ in 0..59 {
            fps.record(1.0 / 60.0);
        }
        fps.record(0.25);
        assert!((fps.max_ms() - 250.0).abs() < 0.01);
        assert!(fps.avg_ms() < 25.0);
    }

    #[test]
    fn ring_caps_and_ages_out() {
        let mut fps = FpsOverlay::new();
        for _ in 0..FPS_SAMPLES {
            fps.record(0.1);
        }
        assert_eq!(fps.count(), FPS_SAMPLES);
        fps.record(0.001);
        assert_eq!(fps.count(), FPS_SAMPLES);
        // The 0.1 s samples still dominate: mean stays near 100 ms.
        assert!(fps.avg_ms() > 99.0);
        // `recent_ms` is newest-first and capped.
        let recent = fps.recent_ms(FPS_SPARKLINE);
        assert_eq!(recent.len(), FPS_SPARKLINE);
        assert!((recent[0] - 1.0).abs() < 0.01);
    }

    #[test]
    fn rejects_garbage_samples() {
        let mut fps = FpsOverlay::new();
        fps.record(f32::NAN);
        fps.record(f32::INFINITY);
        fps.record(-1.0);
        assert_eq!(fps.count(), 0);
        fps.record(0.016);
        assert_eq!(fps.count(), 1);
    }
}
