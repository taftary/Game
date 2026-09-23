//! Per-pass GPU + CPU frame timing (`cosmic-frame-timing`, ADR-027).
//!
//! Window- and GPU-free: the binary owns the Vulkan timestamp pool
//! and only feeds milliseconds here; the FPS widget reads the rolling
//! avg/max rings. Tick-to-ms conversion is the single place the
//! timestamp period enters the codebase.

use std::collections::VecDeque;

/// Ring-buffer capacity per slot/phase: 120 samples ≈ 2 s at 60 fps
/// (the [`crate::fps::FPS_SPARKLINE`] scale).
pub const TIMING_SAMPLES: usize = 120;

/// GPU-timed passes, in frame order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuSlot {
    /// HDR scene: glow points + procedural splats (`record_cosmic_hdr_prepass`
    /// scene pass).
    Prepass,
    /// Mip-bloom pyramid (`record_bloom_chain`).
    Bloom,
    /// Gas-veil raymarch (`record_veil_march`; skipped in sprites mode).
    March,
    /// Main pass: cosmic resolve + 3D view + UI.
    Main,
}

impl GpuSlot {
    pub const ALL: [GpuSlot; 4] = [
        GpuSlot::Prepass,
        GpuSlot::Bloom,
        GpuSlot::March,
        GpuSlot::Main,
    ];

    pub const fn index(self) -> usize {
        match self {
            GpuSlot::Prepass => 0,
            GpuSlot::Bloom => 1,
            GpuSlot::March => 2,
            GpuSlot::Main => 3,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            GpuSlot::Prepass => "prepass",
            GpuSlot::Bloom => "bloom",
            GpuSlot::March => "march",
            GpuSlot::Main => "main",
        }
    }
}

/// CPU-timed phases, in frame order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuPhase {
    /// Frame UI build + atlas sync + vertex upload.
    UiBuild,
    /// `cosmic_frame` precompute (MVP, march push, proc params).
    CosmicFrame,
    /// Command-buffer recording (HDR prepass + main pass + UI).
    Record,
}

impl CpuPhase {
    pub const ALL: [CpuPhase; 3] = [CpuPhase::UiBuild, CpuPhase::CosmicFrame, CpuPhase::Record];

    pub const fn index(self) -> usize {
        match self {
            CpuPhase::UiBuild => 0,
            CpuPhase::CosmicFrame => 1,
            CpuPhase::Record => 2,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            CpuPhase::UiBuild => "ui",
            CpuPhase::CosmicFrame => "cosmic",
            CpuPhase::Record => "record",
        }
    }
}

/// Timestamp ticks → milliseconds. `None` when the period is not a
/// positive finite number (device without timestamp support reports
/// period 0 — NFR4).
pub fn ms_from_ticks(ticks: u64, period_ns: f32) -> Option<f32> {
    if !period_ns.is_finite() || period_ns <= 0.0 {
        return None;
    }
    let ms = ticks as f64 * f64::from(period_ns) / 1e6;
    if ms.is_finite() && ms <= f64::from(f32::MAX) {
        Some(ms as f32)
    } else {
        None
    }
}

/// Rolling timing state: one ring per GPU slot + CPU phase. A slot
/// with no samples reads `None` (the widget prints `n/a`).
#[derive(Clone, Debug)]
pub struct FrameTiming {
    gpu: [VecDeque<f32>; 4],
    cpu: [VecDeque<f32>; 3],
    /// False when the device has no timestamp support — GPU rows stay
    /// `n/a` forever; CPU phases still record.
    pub gpu_supported: bool,
}

impl FrameTiming {
    pub fn new() -> Self {
        FrameTiming {
            gpu: [
                VecDeque::with_capacity(TIMING_SAMPLES),
                VecDeque::with_capacity(TIMING_SAMPLES),
                VecDeque::with_capacity(TIMING_SAMPLES),
                VecDeque::with_capacity(TIMING_SAMPLES),
            ],
            cpu: [
                VecDeque::with_capacity(TIMING_SAMPLES),
                VecDeque::with_capacity(TIMING_SAMPLES),
                VecDeque::with_capacity(TIMING_SAMPLES),
            ],
            gpu_supported: true,
        }
    }

    /// Timing state for a device without timestamp queries: GPU rows
    /// read `n/a`; CPU phases behave normally.
    pub fn unsupported() -> Self {
        let mut timing = FrameTiming::new();
        timing.gpu_supported = false;
        timing
    }

    fn push_ring(ring: &mut VecDeque<f32>, ms: f32) {
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        if ring.len() == TIMING_SAMPLES {
            ring.pop_front();
        }
        ring.push_back(ms);
    }

    /// Record one GPU pass sample (`ms` clamped to finite,
    /// non-negative; ignored when unsupported).
    pub fn push_gpu(&mut self, slot: GpuSlot, ms: f32) {
        if !self.gpu_supported {
            return;
        }
        Self::push_ring(&mut self.gpu[slot.index()], ms);
    }

    /// Record one CPU phase sample.
    pub fn push_cpu(&mut self, phase: CpuPhase, ms: f32) {
        Self::push_ring(&mut self.cpu[phase.index()], ms);
    }

    fn avg_ms(ring: &VecDeque<f32>) -> Option<f32> {
        if ring.is_empty() {
            None
        } else {
            Some(ring.iter().sum::<f32>() / ring.len() as f32)
        }
    }

    fn max_ms(ring: &VecDeque<f32>) -> Option<f32> {
        ring.iter()
            .fold(None, |m: Option<f32>, &s| Some(m.map_or(s, |m| m.max(s))))
    }

    /// Mean GPU ms for a pass (`None` = no samples yet / unsupported).
    pub fn gpu_avg_ms(&self, slot: GpuSlot) -> Option<f32> {
        Self::avg_ms(&self.gpu[slot.index()])
    }

    /// Worst GPU ms for a pass in the window.
    pub fn gpu_max_ms(&self, slot: GpuSlot) -> Option<f32> {
        Self::max_ms(&self.gpu[slot.index()])
    }

    /// Mean CPU ms for a phase.
    pub fn cpu_avg_ms(&self, phase: CpuPhase) -> Option<f32> {
        Self::avg_ms(&self.cpu[phase.index()])
    }

    /// Worst CPU ms for a phase in the window.
    pub fn cpu_max_ms(&self, phase: CpuPhase) -> Option<f32> {
        Self::max_ms(&self.cpu[phase.index()])
    }

    /// Mean total GPU ms over slots that have samples (the frame's
    /// measured GPU cost; `None` before the first full sample set).
    pub fn gpu_total_avg_ms(&self) -> Option<f32> {
        let mut total = 0.0;
        let mut count = 0;
        for slot in GpuSlot::ALL {
            if let Some(avg) = self.gpu_avg_ms(slot) {
                total += avg;
                count += 1;
            }
        }
        if count > 0 { Some(total) } else { None }
    }
}

impl Default for FrameTiming {
    fn default() -> Self {
        FrameTiming::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_math_converts_by_period() {
        // 1M ticks at 1 ns/tick = 1 ms.
        assert_eq!(ms_from_ticks(1_000_000, 1.0), Some(1.0));
        // Typical Intel period (~83 ns): 12k ticks ≈ 1 ms.
        let ms = ms_from_ticks(12_048, 83.0).expect("converts");
        assert!((ms - 1.0).abs() < 0.01, "{ms}");
    }

    #[test]
    fn tick_math_rejects_bad_period() {
        assert_eq!(ms_from_ticks(100, 0.0), None);
        assert_eq!(ms_from_ticks(100, -1.0), None);
        assert_eq!(ms_from_ticks(100, f32::NAN), None);
        assert_eq!(ms_from_ticks(100, f32::INFINITY), None);
    }

    #[test]
    fn empty_reads_none() {
        let timing = FrameTiming::new();
        for slot in GpuSlot::ALL {
            assert_eq!(timing.gpu_avg_ms(slot), None);
            assert_eq!(timing.gpu_max_ms(slot), None);
        }
        for phase in CpuPhase::ALL {
            assert_eq!(timing.cpu_avg_ms(phase), None);
        }
        assert_eq!(timing.gpu_total_avg_ms(), None);
    }

    #[test]
    fn push_feeds_avg_and_max() {
        let mut timing = FrameTiming::new();
        timing.push_gpu(GpuSlot::Bloom, 2.0);
        timing.push_gpu(GpuSlot::Bloom, 4.0);
        assert_eq!(timing.gpu_avg_ms(GpuSlot::Bloom), Some(3.0));
        assert_eq!(timing.gpu_max_ms(GpuSlot::Bloom), Some(4.0));
        // Other slots untouched.
        assert_eq!(timing.gpu_avg_ms(GpuSlot::March), None);
        timing.push_cpu(CpuPhase::Record, 6.0);
        assert_eq!(timing.cpu_avg_ms(CpuPhase::Record), Some(6.0));
        assert_eq!(timing.cpu_max_ms(CpuPhase::Record), Some(6.0));
    }

    #[test]
    fn unsupported_ignores_gpu_but_keeps_cpu() {
        let mut timing = FrameTiming::unsupported();
        assert!(!timing.gpu_supported);
        timing.push_gpu(GpuSlot::Prepass, 5.0);
        assert_eq!(timing.gpu_avg_ms(GpuSlot::Prepass), None);
        timing.push_cpu(CpuPhase::UiBuild, 5.0);
        assert_eq!(timing.cpu_avg_ms(CpuPhase::UiBuild), Some(5.0));
    }

    #[test]
    fn ring_caps_and_ages_out() {
        let mut timing = FrameTiming::new();
        for _ in 0..TIMING_SAMPLES {
            timing.push_gpu(GpuSlot::Main, 10.0);
        }
        timing.push_gpu(GpuSlot::Main, 0.0);
        let avg = timing.gpu_avg_ms(GpuSlot::Main).expect("samples");
        assert!(avg > 9.9 && avg < 10.0, "{avg}");
        assert_eq!(timing.gpu_max_ms(GpuSlot::Main), Some(10.0));
    }

    #[test]
    fn rejects_garbage_samples() {
        let mut timing = FrameTiming::new();
        timing.push_gpu(GpuSlot::Main, f32::NAN);
        timing.push_gpu(GpuSlot::Main, f32::INFINITY);
        timing.push_gpu(GpuSlot::Main, -1.0);
        assert_eq!(timing.gpu_avg_ms(GpuSlot::Main), None);
    }

    #[test]
    fn slot_and_phase_labels_are_stable() {
        assert_eq!(GpuSlot::ALL.len(), 4);
        assert_eq!(CpuPhase::ALL.len(), 3);
        for (i, slot) in GpuSlot::ALL.iter().enumerate() {
            assert_eq!(slot.index(), i);
            assert!(!slot.label().is_empty());
        }
        // Total sums sampled slots only.
        let mut timing = FrameTiming::new();
        timing.push_gpu(GpuSlot::Prepass, 1.0);
        timing.push_gpu(GpuSlot::Bloom, 2.0);
        assert_eq!(timing.gpu_total_avg_ms(), Some(3.0));
    }
}
