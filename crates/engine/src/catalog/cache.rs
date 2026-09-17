//! Resident tile cache with byte accounting + priority eviction
//! (spec §4, ADR-017: 2 GB active star-tile budget).
//!
//! The cache is pure planning state: the threaded loader ([`crate::catalog::io`])
//! fills it, the scheduler ([`crate::catalog::scheduler`]) ranks it.
//! Eviction removes the lowest `(priority, recency)` first; the 2 GB
//! ceiling is enforced on every insert, and `memory_bytes()` reports the
//! accounted total (the `hexsphere` instrumentation pattern).
//!
//! ```
//! use game_engine::catalog::cache::{TileCache, TileKey};
//! use game_engine::catalog::format::{StarRecord, TileHeader, CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR};
//!
//! let mut cache = TileCache::new(1_000_000);
//! let key = TileKey { order: 4, pixel: 9 };
//! let tile = game_engine::catalog::format::DecodedTile {
//!     header: TileHeader {
//!         order: 4,
//!         pixel: 9,
//!         mean_g_mag: 15.0,
//!         mean_bp_rp: 1.0,
//!         epoch_yr: GAIA_EPOCH_YR,
//!         catalog_version: CATALOG_VERSION_SYNTH.to_string(),
//!     },
//!     stars: Vec::new(),
//! };
//! assert!(cache.insert(key, tile).is_ok());
//! assert!(cache.get(&key).is_some());
//! assert!(cache.memory_bytes() > 0);
//! ```

use crate::catalog::format::DecodedTile;
use std::collections::HashMap;
use std::fmt;

/// Default active star-tile budget: 2 GB (ADR-017).
pub const DEFAULT_BUDGET_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Address of one resident tile: HEALPix order + nested pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    /// HEALPix order.
    pub order: u8,
    /// Nested pixel id at [`order`](TileKey::order).
    pub pixel: u64,
}

impl fmt::Display for TileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "o{}:p{}", self.order, self.pixel)
    }
}

/// Rejection reason for [`TileCache::insert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheFull {
    /// Bytes the tile would have added.
    pub tile_bytes: usize,
    /// Budget it would have exceeded.
    pub budget_bytes: usize,
}

impl fmt::Display for CacheFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tile ({} bytes) exceeds cache budget ({} bytes)",
            self.tile_bytes, self.budget_bytes
        )
    }
}

impl std::error::Error for CacheFull {}

struct Slot {
    tile: DecodedTile,
    bytes: usize,
    /// Scheduler rank (frustum 3 → travel 2 → near 1 → stale 0).
    priority: u8,
    /// Recency stamp (bumps on insert + access).
    tick: u64,
}

/// Byte-accounted tile store with priority eviction.
pub struct TileCache {
    slots: HashMap<TileKey, Slot>,
    bytes: usize,
    budget_bytes: usize,
    tick: u64,
}

impl TileCache {
    /// Empty cache with `budget_bytes` ceiling.
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            slots: HashMap::new(),
            bytes: 0,
            budget_bytes,
            tick: 0,
        }
    }

    /// Cache with the ADR-017 default budget ([`DEFAULT_BUDGET_BYTES`]).
    pub fn with_default_budget() -> Self {
        Self::new(DEFAULT_BUDGET_BYTES)
    }

    /// Budget ceiling in bytes.
    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    /// Accounted resident bytes (decoded tiles; GPU-resident estimates
    /// derive from the same stride via [`DecodedTile::memory_bytes`]).
    pub fn memory_bytes(&self) -> usize {
        self.bytes
    }

    /// Resident tile count.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when no tile is resident.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// True when `key` is resident (no recency touch).
    pub fn contains(&self, key: &TileKey) -> bool {
        self.slots.contains_key(key)
    }

    /// Fetch a resident tile, touching its recency.
    pub fn get(&mut self, key: &TileKey) -> Option<&DecodedTile> {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;
        self.slots.get_mut(key).map(|slot| {
            slot.tick = tick;
            &slot.tile
        })
    }

    /// Insert a tile, evicting lowest `(priority, recency)` first until
    /// it fits. Rejects (without evicting) tiles larger than the whole
    /// budget — the decoder cap keeps real tiles far below 2 GB, so a
    /// rejection always signals a misconfigured test budget.
    pub fn insert(&mut self, key: TileKey, tile: DecodedTile) -> Result<(), CacheFull> {
        let bytes = tile.memory_bytes();
        if bytes > self.budget_bytes {
            return Err(CacheFull {
                tile_bytes: bytes,
                budget_bytes: self.budget_bytes,
            });
        }
        self.tick = self.tick.wrapping_add(1);
        if let Some(old) = self.slots.remove(&key) {
            self.bytes -= old.bytes;
        }
        while self.bytes + bytes > self.budget_bytes {
            let victim = self
                .slots
                .iter()
                .min_by_key(|(_, slot)| (slot.priority, slot.tick))
                .map(|(key, _)| *key);
            match victim {
                Some(victim) => {
                    let removed = self.slots.remove(&victim).expect("victim present");
                    self.bytes -= removed.bytes;
                }
                None => break,
            }
        }
        let tick = self.tick;
        self.bytes += bytes;
        self.slots.insert(
            key,
            Slot {
                tile,
                bytes,
                priority: 0,
                tick,
            },
        );
        Ok(())
    }

    /// Remove a tile, returning it when present.
    pub fn remove(&mut self, key: &TileKey) -> Option<DecodedTile> {
        self.slots.remove(key).map(|slot| {
            self.bytes -= slot.bytes;
            slot.tile
        })
    }

    /// Rank the resident set from the scheduler's latest plan: listed
    /// keys take the given priority, unlisted residents decay to stale
    /// (priority 0) so the next insert evicts them first.
    pub fn note_priorities(&mut self, ranked: &[(TileKey, u8)]) {
        let ranks: HashMap<TileKey, u8> = ranked.iter().copied().collect();
        for (key, slot) in self.slots.iter_mut() {
            slot.priority = ranks.get(key).copied().unwrap_or(0);
        }
    }

    /// Resident keys with their priorities (debug overlay + tests).
    pub fn resident(&self) -> Vec<(TileKey, u8)> {
        let mut out: Vec<(TileKey, u8)> = self
            .slots
            .iter()
            .map(|(key, slot)| (*key, slot.priority))
            .collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::format::{CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader};

    fn tile(order: u8, pixel: u64, count: usize) -> DecodedTile {
        DecodedTile {
            header: TileHeader {
                order,
                pixel,
                mean_g_mag: 15.0,
                mean_bp_rp: 1.0,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            stars: vec![
                StarRecord {
                    source_id: 1,
                    ra_rad: 0.1,
                    dec_rad: 0.1,
                    pm_ra_mas_yr: 0.0,
                    pm_dec_mas_yr: 0.0,
                    g_mag: 15.0,
                    bp_rp: 1.0,
                };
                count
            ],
        }
    }

    fn key(order: u8, pixel: u64) -> TileKey {
        TileKey { order, pixel }
    }

    #[test]
    fn insert_get_remove_account_bytes() {
        let mut cache = TileCache::new(1_000_000);
        let t = tile(4, 9, 10);
        let bytes = t.memory_bytes();
        assert!(bytes > 0);
        assert!(cache.insert(key(4, 9), t).is_ok());
        assert_eq!(cache.memory_bytes(), bytes);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&key(4, 9)));
        assert!(cache.get(&key(4, 9)).is_some());
        assert!(cache.remove(&key(4, 9)).is_some());
        assert!(cache.is_empty());
        assert_eq!(cache.memory_bytes(), 0);
    }

    #[test]
    fn eviction_takes_lowest_priority_then_oldest() {
        // Budget fits two small tiles; priorities decide the victim.
        let probe = tile(4, 0, 5);
        let per_tile = probe.memory_bytes();
        let mut cache = TileCache::new(2 * per_tile + per_tile / 2);
        cache.insert(key(4, 1), tile(4, 1, 5)).expect("fits");
        cache.insert(key(4, 2), tile(4, 2, 5)).expect("fits");
        cache.note_priorities(&[(key(4, 1), 3), (key(4, 2), 1)]);
        cache.insert(key(4, 3), tile(4, 3, 5)).expect("evicts");
        assert!(cache.contains(&key(4, 1)), "high priority survives");
        assert!(!cache.contains(&key(4, 2)), "low priority evicted");
        assert!(cache.contains(&key(4, 3)));
        // Unlisted residents decay to stale.
        cache.note_priorities(&[(key(4, 3), 2)]);
        let resident = cache.resident();
        assert!(resident.contains(&(key(4, 1), 0)));
        assert!(resident.contains(&(key(4, 3), 2)));
    }

    #[test]
    fn oversize_tile_rejected_without_evicting() {
        let mut cache = TileCache::new(1000);
        cache.insert(key(4, 1), tile(4, 1, 0)).expect("fits");
        let big = tile(4, 2, 1000);
        assert!(big.memory_bytes() > 1000);
        assert!(cache.insert(key(4, 2), big).is_err());
        assert!(cache.contains(&key(4, 1)), "nothing evicted");
    }

    #[test]
    fn key_display_is_stable() {
        assert_eq!(key(12, 42).to_string(), "o12:p42");
    }
}
