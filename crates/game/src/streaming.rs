//! Chunk streaming cache: distance loads now, unloads later.
//!
//! The caller decides *what* should be visible with the same rule the flat
//! map uses today — [`visible_hemisphere`](game_engine::render::visible_hemisphere)
//! centered on the player — and this cache decides *when* chunks leave:
//! a chunk that drops out of the desired set stays loaded for `grace_ticks`
//! more ticks. Walking forward therefore loads chunks ahead immediately
//! while the trail behind unloads after the delay.
//!
//! Tick-based (not wall-clock) so the sim stays deterministic: same input
//! script, same load/unload sequence on every platform.

use std::collections::BTreeMap;

/// What changed on one [`ChunkStreamer::update`] call. Ids are cell-chunk
/// indices ([`ChunkId`](game_engine::hexsphere::ChunkId)), sorted ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamDelta {
    /// Chunks that became loaded on this tick.
    pub loaded: Vec<u32>,
    /// Chunks evicted on this tick (out of sight past the grace period).
    pub unloaded: Vec<u32>,
}

/// Time-delayed chunk cache.
#[derive(Clone, Debug)]
pub struct ChunkStreamer {
    /// Loaded chunk → last tick it was in the desired set.
    last_seen: BTreeMap<u32, u64>,
    /// Ticks an unseen chunk survives before eviction.
    grace_ticks: u64,
}

impl ChunkStreamer {
    /// Creates an empty cache. A chunk unseen for more than `grace_ticks`
    /// ticks is evicted on the next update.
    pub fn new(grace_ticks: u64) -> Self {
        Self {
            last_seen: BTreeMap::new(),
            grace_ticks,
        }
    }

    /// Grace period in ticks.
    pub fn grace_ticks(&self) -> u64 {
        self.grace_ticks
    }

    /// Marks `desired` visible at `tick`: loads newcomers immediately,
    /// refreshes survivors, evicts chunks unseen for longer than the grace
    /// period. `tick` must be monotonic within a run.
    ///
    /// ```
    /// use game::streaming::ChunkStreamer;
    ///
    /// let mut streamer = ChunkStreamer::new(2);
    /// let first = streamer.update(&[1, 2, 3], 0);
    /// assert_eq!(first.loaded, vec![1, 2, 3]);
    /// let gone = streamer.update(&[3], 3);
    /// assert_eq!(gone.unloaded, vec![1, 2]);
    /// ```
    pub fn update(&mut self, desired: &[u32], tick: u64) -> StreamDelta {
        let mut delta = StreamDelta::default();
        for &chunk in desired {
            if self.last_seen.insert(chunk, tick).is_none() {
                delta.loaded.push(chunk);
            }
        }
        delta.loaded.sort_unstable();
        let stale: Vec<u32> = self
            .last_seen
            .iter()
            .filter(|entry| tick.saturating_sub(*entry.1) > self.grace_ticks)
            .map(|entry| *entry.0)
            .collect();
        for chunk in stale {
            self.last_seen.remove(&chunk);
            delta.unloaded.push(chunk);
        }
        delta
    }

    /// Currently loaded chunks, sorted ascending.
    pub fn loaded(&self) -> Vec<u32> {
        self.last_seen.keys().copied().collect()
    }

    /// Number of currently loaded chunks.
    pub fn loaded_count(&self) -> usize {
        self.last_seen.len()
    }

    /// Whether `chunk` is currently loaded.
    pub fn is_loaded(&self, chunk: u32) -> bool {
        self.last_seen.contains_key(&chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_desired_set_immediately() {
        let mut streamer = ChunkStreamer::new(5);
        let delta = streamer.update(&[3, 1, 2], 0);
        assert_eq!(delta.loaded, vec![1, 2, 3]);
        assert!(delta.unloaded.is_empty());
        assert_eq!(streamer.loaded_count(), 3);
    }

    #[test]
    fn unseen_chunks_survive_within_grace_then_evict() {
        let mut streamer = ChunkStreamer::new(2);
        streamer.update(&[1, 2], 0);
        // Still loaded while the grace period runs.
        let delta = streamer.update(&[2], 2);
        assert!(delta.unloaded.is_empty());
        assert!(streamer.is_loaded(1));
        // Past the grace period, the trail unloads.
        let delta = streamer.update(&[2], 3);
        assert_eq!(delta.unloaded, vec![1]);
        assert!(!streamer.is_loaded(1));
        assert!(streamer.is_loaded(2));
    }

    #[test]
    fn reappearing_chunk_is_not_reloaded() {
        let mut streamer = ChunkStreamer::new(5);
        streamer.update(&[7], 0);
        let delta = streamer.update(&[7, 8], 3);
        assert_eq!(delta.loaded, vec![8]);
        assert!(delta.unloaded.is_empty());
    }

    #[test]
    fn evicted_chunk_reloads_when_seen_again() {
        let mut streamer = ChunkStreamer::new(0);
        streamer.update(&[4], 0);
        let delta = streamer.update(&[], 1);
        assert_eq!(delta.unloaded, vec![4]);
        let delta = streamer.update(&[4], 2);
        assert_eq!(delta.loaded, vec![4]);
    }

    #[test]
    fn update_is_deterministic() {
        let run = || {
            let mut streamer = ChunkStreamer::new(3);
            let mut log = Vec::new();
            for tick in 0..10_u64 {
                let desired: Vec<u32> = (tick as u32..tick as u32 + 4).collect();
                log.push(streamer.update(&desired, tick));
            }
            log
        };
        assert_eq!(run(), run());
    }
}
