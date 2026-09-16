//! Seeded deterministic RNG for all generation paths.
//!
//! SplitMix64 (Steele–Lea–O'Neill) with domain-separated streams: one
//! [`SeededRng`] per `(seed, domain)`, so the galaxy stage and the system
//! stage never share a sequence even for the same runtime seed. No
//! external RNG crate is used (none in the workspace deps); the algorithm
//! is ~15 lines and pinned by a reference-vector test below.
//!
//! Rules for generation code (risks #2):
//!
//! - Derive [`SeededRng::stream`] per stage/domain. Never `SystemTime`,
//!   thread ids, or pointer hashes in generation paths.
//! - Derive integers first ([`SeededRng::below`], [`SeededRng::range_i64`]);
//!   scale to floats only at the edge, then quantize
//!   ([`quantize_f64`](super::quant::quantize_f64)) before hashing or
//!   storing. Platform `libm` transcendentals (`sin`, `exp`, …) may
//!   differ 1 ulp across ARM/x86 — keep them out of hashed outputs, or
//!   quantize after.
//!
//! ```
//! use game_engine::core::SeededRng;
//!
//! let mut galaxy = SeededRng::stream(42, "galaxy/stars");
//! let mut system = SeededRng::stream(42, "system/planets");
//! // Same (seed, domain) replays the same stream on every platform.
//! let mut replay = SeededRng::stream(42, "galaxy/stars");
//! assert_eq!(galaxy.next_u64(), replay.next_u64());
//! // Different domains never share a sequence.
//! assert_ne!(galaxy.next_u64(), system.next_u64());
//! ```

/// Golden-ratio increment of the SplitMix64 state (Steele et al.).
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// 64-bit FNV-1a offset basis / prime: hashes stream domain labels.
/// Plain integer ops — identical on every platform.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    hash
}

/// SplitMix64 finalizer (Steele et al.): avalanches 64 bits of state
/// into 64 bits of output. Also used to mix `(seed, domain)` into a
/// stream's initial state.
fn avalanche(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic seeded RNG. Not `Sync`-shared across threads by design:
/// hand one stream per (seed, domain) to each job instead of sharing
/// state, so output never depends on thread scheduling.
#[derive(Clone, Debug)]
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    /// Raw stream from a 64-bit seed. Prefer [`SeededRng::stream`] in
    /// generation code so stages stay independent.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Domain-separated stream for `(seed, domain)` — e.g.
    /// `"galaxy/stars"`, `"system/planets"`. Same pair replays the same
    /// sequence on every platform; different domains are independent
    /// even for the same seed.
    pub fn stream(seed: u64, domain: &str) -> Self {
        let mixed = seed.wrapping_add(fnv1a64(domain.as_bytes()));
        Self {
            state: avalanche(mixed),
        }
    }

    /// Next 64 bits of the stream.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        avalanche(self.state)
    }

    /// Next 32 bits (upper half of [`SeededRng::next_u64`]).
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform value in `0..n`, unbiased (Lemire multiply-high — exact
    /// `u128` math, no modulo bias).
    ///
    /// # Panics
    ///
    /// Debug-asserts `n > 0`: an empty range is a caller bug.
    pub fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0, "range must be non-empty");
        (((self.next_u64() as u128) * (n as u128)) >> 64) as u64
    }

    /// Uniform integer in `lo..hi` (inclusive `lo`, exclusive `hi`).
    ///
    /// # Panics
    ///
    /// Debug-asserts `lo < hi`.
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(lo < hi, "range must be non-empty");
        let span = (hi - lo) as u64;
        lo + self.below(span) as i64
    }

    /// Uniform float in `[0, 1)` from the top 53 bits: the value lies on
    /// the `f64` dyadic grid, identical on every IEEE-754 platform.
    /// Scale at the edge, then quantize before storing or hashing.
    pub fn unit_f64(&mut self) -> f64 {
        const TWO_POW_53: f64 = 9_007_199_254_740_992.0;
        (self.next_u64() >> 11) as f64 / TWO_POW_53
    }

    /// Uniform float in `[lo, hi)`: `lo + t * (hi - lo)`. Deterministic
    /// for fixed inputs; the caller quantizes the result
    /// ([`quantize_f64`](super::quant::quantize_f64)) before it reaches
    /// a descriptor hash or a save.
    ///
    /// # Panics
    ///
    /// Debug-asserts `lo < hi` and finite bounds.
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        debug_assert!(lo < hi && lo.is_finite() && hi.is_finite());
        lo + self.unit_f64() * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference vector: first output of the SplitMix64 sequence for
    /// seed 0 (Steele–Lea–O'Neill). Pins the port — any drift in the
    /// finalizer fails `splitmix64_matches_reference_vector` loudly
    /// instead of silently re-rolling every universe.
    const REFERENCE_VECTOR: u64 = 0xE220_A839_7B1D_CDAF;

    #[test]
    fn splitmix64_matches_reference_vector() {
        assert_eq!(SeededRng::new(0).next_u64(), REFERENCE_VECTOR);
    }

    #[test]
    fn same_seed_replays_same_stream() {
        let mut a = SeededRng::new(0xDEAD_BEEF);
        let mut b = SeededRng::new(0xDEAD_BEEF);
        for _ in 0..1_000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn stream_domains_are_stable_and_independent() {
        let mut a = SeededRng::stream(42, "galaxy/stars");
        let mut replay = SeededRng::stream(42, "galaxy/stars");
        let mut other = SeededRng::stream(42, "system/planets");
        assert_eq!(a.next_u64(), replay.next_u64());
        assert_ne!(a.next_u64(), other.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_all_buckets() {
        let mut rng = SeededRng::new(7);
        let mut seen = [false; 7];
        for _ in 0..10_000 {
            let v = rng.below(7);
            assert!(v < 7);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn range_helpers_stay_in_bounds() {
        let mut rng = SeededRng::new(99);
        for _ in 0..1_000 {
            let i = rng.range_i64(-5, 6);
            assert!((-5..6).contains(&i));
            let x = rng.range_f64(2.0, 8.0);
            assert!((2.0..8.0).contains(&x));
            let u = rng.unit_f64();
            assert!((0.0..1.0).contains(&u));
        }
    }
}
