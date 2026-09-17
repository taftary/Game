//! Tile-load latency statistics (notion NFR: p50/p95 logged via
//! `tracing`, ADR-009).
//!
//! No metrics framework exists in the tree, so this module is the whole
//! story on purpose: a fixed-bucket histogram over per-tile load times,
//! percentile estimates from bucket edges, and a `flush()` that emits
//! one structured `tracing::info!` event per interval and resets.
//! Percentiles are bucket-edge estimates (conservative: the reported
//! value is the top of the bucket holding the quantile), documented
//! here so ANALYST never mistakes them for exact quantiles.
//!
//! ```
//! use game_engine::catalog::metrics::LatencyStats;
//!
//! let mut stats = LatencyStats::new();
//! for _ in 0..100 {
//!     stats.record(12.0);
//! }
//! assert_eq!(stats.count(), 100);
//! let (p50, p95) = stats.percentiles().expect("samples");
//! assert!(p50 <= 25.0 && p95 <= 25.0);
//! ```

/// Upper bucket edges in milliseconds; the last bucket is the
/// overflow (`> 1000 ms` — the fallback already rendered by then).
const BUCKETS_MS: [f64; 10] = [1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0];

/// Fixed-bucket histogram of tile-load latencies (milliseconds).
#[derive(Clone, Debug)]
pub struct LatencyStats {
    buckets: [u64; 11],
    count: u64,
    sum_ms: f64,
    max_ms: f64,
}

impl LatencyStats {
    /// Empty statistics.
    pub fn new() -> Self {
        Self {
            buckets: [0; 11],
            count: 0,
            sum_ms: 0.0,
            max_ms: 0.0,
        }
    }

    /// Samples recorded so far.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Mean latency in milliseconds (`None` when empty).
    pub fn mean_ms(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum_ms / self.count as f64)
        }
    }

    /// Record one tile-load latency. Non-finite or negative samples are
    /// caller bugs — debug-asserted, then clamped into bucket 0 rather
    /// than poisoning the histogram.
    pub fn record(&mut self, ms: f64) {
        debug_assert!(ms.is_finite() && ms >= 0.0, "latency must be finite");
        let ms = if ms.is_finite() && ms >= 0.0 { ms } else { 0.0 };
        let bucket = BUCKETS_MS.iter().position(|&edge| ms <= edge);
        match bucket {
            Some(index) => self.buckets[index] += 1,
            None => self.buckets[BUCKETS_MS.len()] += 1,
        }
        self.count += 1;
        self.sum_ms += ms;
        self.max_ms = self.max_ms.max(ms);
    }

    /// Estimated (p50, p95) in milliseconds: the top edge of the bucket
    /// holding each quantile (`None` when empty).
    pub fn percentiles(&self) -> Option<(f64, f64)> {
        if self.count == 0 {
            return None;
        }
        Some((self.quantile(0.5), self.quantile(0.95)))
    }

    fn quantile(&self, q: f64) -> f64 {
        debug_assert!((0.0..=1.0).contains(&q));
        let rank = (q * self.count as f64).ceil().max(1.0) as u64;
        let mut seen = 0u64;
        for (index, bucket) in self.buckets.iter().enumerate() {
            seen += *bucket;
            if seen >= rank {
                return if index < BUCKETS_MS.len() {
                    BUCKETS_MS[index]
                } else {
                    self.max_ms
                };
            }
        }
        self.max_ms
    }

    /// One-line interval summary (`n=… mean=…ms p50≈…ms p95≈…ms
    /// max=…ms`), for logs and the debug overlay.
    pub fn report(&self) -> String {
        match (self.percentiles(), self.mean_ms()) {
            (Some((p50, p95)), Some(mean)) => format!(
                "n={} mean={:.1}ms p50~{:.0}ms p95~{:.0}ms max={:.1}ms",
                self.count, mean, p50, p95, self.max_ms
            ),
            _ => "n=0 (no samples)".to_string(),
        }
    }

    /// Emit the interval summary as a structured `tracing::info!` event
    /// and reset (per-interval statistics, ADR-009).
    pub fn flush(&mut self) {
        if self.count == 0 {
            return;
        }
        let (p50, p95) = self.percentiles().expect("counted");
        tracing::info!(
            samples = self.count,
            mean_ms = self.mean_ms().expect("counted"),
            p50_ms = p50,
            p95_ms = p95,
            max_ms = self.max_ms,
            "catalog tile latency"
        );
        *self = Self::new();
    }
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stats_report_cleanly() {
        let stats = LatencyStats::new();
        assert_eq!(stats.count(), 0);
        assert_eq!(stats.percentiles(), None);
        assert_eq!(stats.mean_ms(), None);
        assert_eq!(stats.report(), "n=0 (no samples)");
    }

    #[test]
    fn quantiles_track_bucket_edges() {
        let mut stats = LatencyStats::new();
        for _ in 0..100 {
            stats.record(12.0);
        }
        // 12 ms lands in the ≤25 ms bucket: estimates are bucket tops.
        assert_eq!(stats.percentiles(), Some((25.0, 25.0)));
        assert_eq!(stats.mean_ms(), Some(12.0));
        assert!(stats.report().starts_with("n=100"));
        // One slow sample does not move p95 (still the 12 ms bucket)…
        stats.record(5000.0);
        assert_eq!(stats.percentiles(), Some((25.0, 25.0)));
        // …but six do: the 95th rank falls in the overflow bucket,
        // which reports the observed max.
        for _ in 0..5 {
            stats.record(5000.0);
        }
        let (_, p95) = stats.percentiles().expect("samples");
        assert_eq!(p95, 5000.0);
    }

    #[test]
    fn flush_resets_after_emitting() {
        let mut stats = LatencyStats::new();
        stats.record(3.0);
        stats.flush();
        assert_eq!(stats.count(), 0);
        // Flushing empty stats is a silent no-op (no event, no panic).
        stats.flush();
    }
}
