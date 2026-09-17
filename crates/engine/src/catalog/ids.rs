//! Stable star identity across catalog and fallback (ADR-017: one ID
//! space so visual replacement never affects selection, focus
//! targeting, or navigation state).
//!
//! [`StarId`] covers catalog sources (`source_id`) and fallback slots
//! (`order`/`pixel`/`slot`) in a single `Copy + Hash + Ord` type with a
//! canonical display form. [`StarRegistry`] indexes resident tiles by
//! source id and resolves any id against the live cache: catalog ids
//! hit resident tiles, fallback ids refine to their catalog counterpart
//! once the tile arrives (slot `s` → source `s % count`) or recompute
//! the deterministic lattice point while it hasn't. Held ids therefore
//! stay valid across the swap — positions refine, selection never
//! dangles.
//!
//! ```
//! use game_engine::catalog::ids::StarId;
//!
//! let catalog = StarId::catalog(12345);
//! let fallback = StarId::fallback(12, 678, 3);
//! assert_ne!(catalog, fallback);
//! assert_eq!(catalog.to_string(), "gaia-dr3/12345");
//! assert_eq!(fallback.to_string(), "fallback/o12p678s3");
//! ```

use crate::catalog::cache::{TileCache, TileKey};
use crate::catalog::fallback::{TileMeta, fallback_star};
use std::collections::HashMap;
use std::fmt;

/// One identifier for a star in either backing store: catalog record
/// or fallback lattice slot. Ordered with catalog ids first (stable
/// sort order for selection lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StarId {
    /// Cataloged source by its stable `source_id`.
    Catalog {
        /// Stable catalog identifier.
        source_id: u64,
    },
    /// Fallback lattice slot (tile + deterministic index).
    Fallback {
        /// HEALPix order of the tile.
        order: u8,
        /// Nested pixel id of the tile.
        pixel: u64,
        /// Lattice slot within the tile.
        slot: u32,
    },
}

impl StarId {
    /// Catalog id for `source_id`.
    pub fn catalog(source_id: u64) -> Self {
        StarId::Catalog { source_id }
    }

    /// Fallback id for lattice `slot` of tile (`order`, `pixel`).
    pub fn fallback(order: u8, pixel: u64, slot: u32) -> Self {
        StarId::Fallback { order, pixel, slot }
    }

    /// True for catalog-backed ids.
    pub fn is_catalog(self) -> bool {
        matches!(self, StarId::Catalog { .. })
    }

    /// True for fallback ids.
    pub fn is_fallback(self) -> bool {
        matches!(self, StarId::Fallback { .. })
    }
}

impl fmt::Display for StarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StarId::Catalog { source_id } => write!(f, "gaia-dr3/{source_id}"),
            StarId::Fallback { order, pixel, slot } => {
                write!(f, "fallback/o{order}p{pixel}s{slot}")
            }
        }
    }
}

/// Resolved astrometry for a [`StarId`]: the same shape regardless of
/// backing store, so consumers never branch on catalog vs fallback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedStar {
    /// Right ascension (rad).
    pub ra_rad: f64,
    /// Declination (rad).
    pub dec_rad: f64,
    /// G magnitude.
    pub g_mag: f32,
    /// BP–RP color index.
    pub bp_rp: f32,
    /// True when the astrometry comes from catalog records (false =
    /// deterministic fallback approximation).
    pub from_catalog: bool,
}

/// Live index of resident catalog content plus fallback resolution.
///
/// The registry never stores tiles — it indexes `source_id → tile` and
/// resolves against a caller-supplied [`TileCache`], so eviction needs
/// no notification: resolving through an evicted tile simply misses the
/// cache and falls back. Tile files are immutable, so a hit for the
/// same key is always the same content.
pub struct StarRegistry {
    by_source: HashMap<u64, (TileKey, usize)>,
    master_seed: u64,
}

impl StarRegistry {
    /// Empty registry; `master_seed` drives fallback recomputation.
    pub fn new(master_seed: u64) -> Self {
        Self {
            by_source: HashMap::new(),
            master_seed,
        }
    }

    /// Index every source of a resident tile (call after cache insert).
    /// Re-indexing the same key refreshes it; files are immutable, so
    /// this is idempotent.
    pub fn index_tile(&mut self, key: TileKey, tile: &crate::catalog::format::DecodedTile) {
        for (index, star) in tile.stars.iter().enumerate() {
            self.by_source.insert(star.source_id, (key, index));
        }
    }

    /// Forget an evicted tile's sources (call with the tile returned by
    /// cache removal; unknown keys are ignored).
    pub fn forget_tile(&mut self, tile: &crate::catalog::format::DecodedTile) {
        for star in &tile.stars {
            self.by_source.remove(&star.source_id);
        }
    }

    /// Indexed source count (debug overlay + tests).
    pub fn len(&self) -> usize {
        self.by_source.len()
    }

    /// True when no source is indexed.
    pub fn is_empty(&self) -> bool {
        self.by_source.is_empty()
    }

    /// Resolve `id` against `cache`:
    /// - catalog ids hit resident tiles (`None` once evicted);
    /// - fallback ids refine to their catalog counterpart when the tile
    ///   is resident, else recompute the lattice point from `meta`
    ///   (manifest/header metadata) or the procedural model.
    pub fn resolve(
        &self,
        id: StarId,
        cache: &mut TileCache,
        meta: Option<&TileMeta>,
    ) -> Option<ResolvedStar> {
        match id {
            StarId::Catalog { source_id } => {
                let (key, index) = self.by_source.get(&source_id)?;
                let tile = cache.get(key)?;
                let star = tile.stars.get(*index)?;
                if star.source_id != source_id {
                    return None;
                }
                Some(ResolvedStar {
                    ra_rad: star.ra_rad,
                    dec_rad: star.dec_rad,
                    g_mag: star.g_mag,
                    bp_rp: star.bp_rp,
                    from_catalog: true,
                })
            }
            StarId::Fallback { order, pixel, slot } => {
                let key = TileKey { order, pixel };
                if let Some(tile) = cache.get(&key)
                    && !tile.stars.is_empty()
                {
                    // Counts approximately match by construction, so
                    // slot s refines to source s (wrapping on skew).
                    let star = &tile.stars[slot as usize % tile.stars.len()];
                    return Some(ResolvedStar {
                        ra_rad: star.ra_rad,
                        dec_rad: star.dec_rad,
                        g_mag: star.g_mag,
                        bp_rp: star.bp_rp,
                        from_catalog: true,
                    });
                }
                let meta = meta.copied().unwrap_or_else(|| {
                    crate::catalog::fallback::model_meta(self.master_seed, order, pixel)
                });
                let star = fallback_star(self.master_seed, order, pixel, slot, &meta)?;
                Some(ResolvedStar {
                    ra_rad: star.ra_rad,
                    dec_rad: star.dec_rad,
                    g_mag: star.g_mag,
                    bp_rp: star.bp_rp,
                    from_catalog: false,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::format::{CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader};

    fn tile_with(stars: Vec<StarRecord>) -> crate::catalog::format::DecodedTile {
        crate::catalog::format::DecodedTile {
            header: TileHeader {
                order: 4,
                pixel: 9,
                mean_g_mag: 15.0,
                mean_bp_rp: 1.0,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            stars,
        }
    }

    fn star(source_id: u64, ra: f64) -> StarRecord {
        StarRecord {
            source_id,
            ra_rad: ra,
            dec_rad: 0.1,
            pm_ra_mas_yr: 0.0,
            pm_dec_mas_yr: 0.0,
            g_mag: 15.0,
            bp_rp: 1.0,
        }
    }

    #[test]
    fn display_forms_are_canonical() {
        assert_eq!(StarId::catalog(7).to_string(), "gaia-dr3/7");
        assert_eq!(StarId::fallback(12, 42, 3).to_string(), "fallback/o12p42s3");
        assert!(StarId::catalog(1).is_catalog());
        assert!(StarId::fallback(4, 9, 0).is_fallback());
    }

    #[test]
    fn catalog_ids_resolve_while_resident() {
        let mut cache = TileCache::new(1_000_000);
        let mut registry = StarRegistry::new(42);
        let key = TileKey { order: 4, pixel: 9 };
        let tile = tile_with(vec![star(101, 0.5), star(102, 0.6)]);
        cache.insert(key, tile.clone()).expect("fits");
        registry.index_tile(key, &tile);
        assert_eq!(registry.len(), 2);
        let resolved = registry
            .resolve(StarId::catalog(101), &mut cache, None)
            .expect("resident");
        assert!(resolved.from_catalog);
        assert!((resolved.ra_rad - 0.5).abs() < 1e-12);
        // Eviction invalidates without notification: resolve misses.
        let evicted = cache.remove(&key).expect("present");
        registry.forget_tile(&evicted);
        assert!(registry.is_empty());
        assert_eq!(
            registry.resolve(StarId::catalog(101), &mut cache, None),
            None
        );
    }

    #[test]
    fn fallback_ids_survive_the_catalog_swap() {
        // The DoD-2 contract in miniature: a held fallback id resolves
        // before the tile arrives (lattice) and after (catalog
        // counterpart) — selection never dangles across the swap.
        let mut cache = TileCache::new(1_000_000);
        let registry = StarRegistry::new(42);
        let key = TileKey { order: 4, pixel: 9 };
        let id = StarId::fallback(4, 9, 1);
        let meta = TileMeta {
            count: 2,
            mean_g_mag: 15.0,
            mean_bp_rp: 1.0,
        };
        let before = registry
            .resolve(id, &mut cache, Some(&meta))
            .expect("fallback resolves");
        assert!(!before.from_catalog);
        // Tile arrives: slot 1 refines to source index 1.
        let tile = tile_with(vec![star(101, 0.5), star(102, 0.6)]);
        cache.insert(key, tile).expect("fits");
        let after = registry
            .resolve(id, &mut cache, Some(&meta))
            .expect("still resolves");
        assert!(after.from_catalog);
        assert!((after.ra_rad - 0.6).abs() < 1e-12);
    }
}
