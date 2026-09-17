//! Integration tests for star-catalog streaming (DoD evidence,
//! `plans/v0.2.0/star-catalog-streaming`).
//!
//! These tests drive the engine pieces the way the debug viewer does â€”
//! schedule â†’ load â†’ cache â†’ resolve â€” over a small order-12 fixture
//! cooked inline (no `game_tools` dependency: tiles encode straight
//! from [`game_engine::catalog::format`]).
//!
//! DoD mapping:
//! - DoD-1 (`dod1_...`): order-12 tiles stream within budget with
//!   memory + latency evidence.
//! - DoD-2 (`dod2_...`): a forced tile miss falls back inside 200 ms
//!   and the same [`StarId`](game_engine::catalog::ids::StarId)
//!   resolves stably across the swap.
//! - DoD-3 (`dod3_...`): epoch/proper-motion fields plumb end to end
//!   into `scale-physics` extrapolation + validity flags.
//!
//! NOTE: everything below lives in `#[cfg(test)]`, so the non-test
//!   build stays an empty crate (the workspace `-D warnings` clippy
//!   gate treats test-only imports as unused otherwise).

#[cfg(test)]
mod tests {
    use game_engine::catalog::cache::{TileCache, TileKey};
    use game_engine::catalog::fallback::{FALLBACK_DELAY_MS, TileMeta, model_meta};
    use game_engine::catalog::format::{
        CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader, decode_tile, encode_tile,
    };
    use game_engine::catalog::healpix::{ang2pix, pix2ang, tiles_in_cone};
    use game_engine::catalog::ids::{StarId, StarRegistry};
    use game_engine::catalog::io::{LoadRequest, TileLoader};
    use game_engine::catalog::metrics::LatencyStats;
    use game_engine::catalog::scheduler::{Scheduler, SkyView};
    use game_engine::physics::{PROPER_MOTION_SPAN_YR, extrapolate};
    use glam::dcamera::rh::proj::directx::perspective;
    use glam::dcamera::rh::view::look_at_mat4;
    use glam::{DMat3, DVec3};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const ORDER: u8 = 12;
    const CENTER_RA: f64 = 1.0;
    const CENTER_DEC: f64 = 0.2;

    /// Scratch tile root, unique per test (best-effort cleanup).
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("game_dod_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(format!("order{ORDER}"))).expect("scratch dir");
        dir
    }

    /// Deterministic dense records, all inside `pixel` (micro-jitter around
    /// the tile center â€” orders of magnitude below the 51â€³ tile scale).
    fn make_records(pixel: u64, count: usize, first_id: u64) -> Vec<StarRecord> {
        let (cra, cdec) = pix2ang(ORDER, pixel).expect("valid pixel");
        let mut rng = game_engine::core::SeededRng::stream(11, "tests/fixture");
        (0..count)
            .map(|index| {
                let mut jitter = || (rng.unit_f64() - 0.5) * 2e-6;
                StarRecord {
                    source_id: first_id + index as u64,
                    ra_rad: cra + jitter(),
                    dec_rad: cdec + jitter(),
                    pm_ra_mas_yr: 2.5,
                    pm_dec_mas_yr: -1.5,
                    g_mag: 12.0 + (index % 50) as f32 * 0.1,
                    bp_rp: 1.0,
                }
            })
            .collect()
    }

    fn write_tile(root: &Path, pixel: u64, stars: &[StarRecord]) -> PathBuf {
        let header = TileHeader {
            order: ORDER,
            pixel,
            mean_g_mag: 14.0,
            mean_bp_rp: 1.0,
            epoch_yr: GAIA_EPOCH_YR,
            catalog_version: CATALOG_VERSION_SYNTH.to_string(),
        };
        let bytes = encode_tile(&header, stars);
        let decoded = decode_tile(&bytes).expect("fixture re-decodes");
        assert_eq!(decoded.stars.len(), stars.len());
        let path = root
            .join(format!("order{ORDER}"))
            .join(format!("{pixel}.tile"));
        std::fs::write(&path, &bytes).expect("tile writes");
        path
    }

    /// Narrow-fov view at the fixture center (keeps the order-12 plan
    /// small and fast while exercising the real pipeline).
    fn fixture_view() -> SkyView {
        let forward = DVec3::new(
            CENTER_DEC.cos() * CENTER_RA.cos(),
            CENTER_DEC.cos() * CENTER_RA.sin(),
            CENTER_DEC.sin(),
        )
        .normalize();
        let eye = -forward * 5.0;
        let view = look_at_mat4(eye, DVec3::ZERO, DVec3::Y);
        let proj = perspective(5.0_f64.to_radians(), 1.0, 0.05, 1000.0);
        SkyView {
            view_proj: proj * view,
            cam_forward_world: forward,
            fov_y_rad: 5.0_f64.to_radians(),
            aspect: 1.0,
            world_to_equatorial: DMat3::IDENTITY,
            velocity_world: DVec3::ZERO,
        }
    }

    fn drain(loader: &mut TileLoader) -> Vec<game_engine::catalog::io::LoadResult> {
        let mut out = Vec::new();
        for _ in 0..2000 {
            out.extend(loader.poll());
            if !out.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        out
    }

    #[test]
    fn dod1_order12_tile_set_streams_within_budget() {
        let root = scratch("dod1");
        // Eight dense tiles (3000 records â‰ˆ 120 KB each) around the view.
        let cone = tiles_in_cone(ORDER, CENTER_RA, CENTER_DEC, 0.01).expect("cone");
        let flat: Vec<u64> = cone.iter().flat_map(|r| r.clone()).take(8).collect();
        assert_eq!(flat.len(), 8, "cone yields eight tiles");
        let mut next_id = 1u64;
        for pixel in &flat {
            let stars = make_records(*pixel, 3000, next_id);
            next_id += 3000;
            // Placement self-check: every record hashes home.
            for star in &stars {
                assert_eq!(ang2pix(ORDER, star.ra_rad, star.dec_rad), Some(*pixel));
            }
            write_tile(&root, *pixel, &stars);
        }
        // The scheduler covers all eight fixture tiles.
        let mut scheduler = Scheduler::new(ORDER, root.clone()).expect("scheduler");
        let plan = scheduler.plan(&fixture_view()).expect("valid view");
        for pixel in &flat {
            assert!(
                plan.ranges.iter().any(|r| r.range.contains(pixel)),
                "tile {pixel} planned"
            );
        }
        // Stream through the threaded loader into the default-budget cache.
        let mut loader = TileLoader::try_spawn(2).expect("spawns");
        let mut cache = TileCache::with_default_budget();
        let mut registry = StarRegistry::new(42);
        let mut stats = LatencyStats::new();
        for pixel in &flat {
            let key = TileKey {
                order: ORDER,
                pixel: *pixel,
            };
            assert!(loader.request(LoadRequest {
                key,
                path: scheduler.tile_path(&key),
                generation: plan.generation,
            }));
        }
        let mut results = Vec::new();
        while results.len() < flat.len() {
            results.extend(drain(&mut loader));
            assert!(
                results.len() <= flat.len(),
                "no duplicate completions (dedupe holds)"
            );
            if results.len() < flat.len() && results.is_empty() {
                panic!("loader stalled");
            }
        }
        let mut expected_bytes = 0usize;
        for result in &results {
            // DoD-1 latency bound: < 100 ms once data is local (temp-dir
            // reads land in microseconds; 100 ms is 100Ã— headroom, not a
            // tautology â€” it trips on thread-pool or decode regressions).
            assert!(
                result.elapsed_ms < 100.0,
                "tile {} took {:.1} ms",
                result.key,
                result.elapsed_ms
            );
            stats.record(result.elapsed_ms);
            let tile = result.outcome.as_ref().expect("tile decodes");
            expected_bytes += tile.memory_bytes();
            cache.insert(result.key, tile.clone()).expect("fits 2 GB");
            registry.index_tile(result.key, tile);
        }
        assert_eq!(cache.memory_bytes(), expected_bytes);
        assert!(cache.memory_bytes() < TileCache::with_default_budget().budget_bytes());
        let (p50, p95) = stats.percentiles().expect("samples");
        assert!(p95 < 100.0, "p95 {p95} ms");
        eprintln!(
            "DoD-1: 8 tiles, {} records, {:.2} MB, p50 {p50} ms p95 {p95} ms",
            8 * 3000,
            cache.memory_bytes() as f64 / 1_048_576.0
        );
        assert_eq!(registry.len(), 8 * 3000);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dod2_forced_miss_falls_back_fast_and_swaps_stably() {
        let root = scratch("dod2");
        let pixel = ang2pix(ORDER, CENTER_RA, CENTER_DEC).expect("coords");
        let key = TileKey {
            order: ORDER,
            pixel,
        };
        let meta = TileMeta {
            count: 64,
            mean_g_mag: 14.0,
            mean_bp_rp: 1.0,
        };
        let id = StarId::fallback(ORDER, pixel, 7);
        let mut cache = TileCache::new(10_000_000);
        let registry = StarRegistry::new(42);
        // Force the miss: request a tile whose file was never written.
        let mut loader = TileLoader::try_spawn(1).expect("spawns");
        let missing = root.join(format!("order{ORDER}/{pixel}.tile"));
        let started = Instant::now();
        assert!(loader.request(LoadRequest {
            key,
            path: missing,
            generation: 1,
        }));
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        assert!(results[0].outcome.is_err(), "absent file fails");
        // Fallback resolves inside the 200 ms policy deadline, from the
        // same StarId the selection holds.
        let before = registry
            .resolve(id, &mut cache, Some(&meta))
            .expect("fallback resolves");
        assert!(!before.from_catalog);
        assert!(
            started.elapsed().as_secs_f64() * 1000.0 < FALLBACK_DELAY_MS,
            "miss â†’ fallback inside {FALLBACK_DELAY_MS} ms"
        );
        // The tile arrives late: same id refines to its catalog counterpart
        // (slot 7 â†’ source index 7). Positions agree coarsely â€” lattice and
        // catalog stars are independent points, so this bounds wild
        // mis-resolution (a tile diagonal), not astrometric identity.
        let stars = make_records(pixel, 64, 5000);
        write_tile(&root, pixel, &stars);
        assert!(loader.request(LoadRequest {
            key,
            path: root.join(format!("order{ORDER}/{pixel}.tile")),
            generation: 2,
        }));
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        let tile = results[0].outcome.as_ref().expect("tile arrives");
        cache.insert(key, tile.clone()).expect("fits");
        let after = StarRegistry::new(42)
            .resolve(id, &mut cache, Some(&meta))
            .expect("still resolves");
        assert!(after.from_catalog, "backs catalog after arrival");
        let dra = (after.ra_rad - before.ra_rad).abs();
        let ddec = (after.dec_rad - before.dec_rad).abs();
        assert!(
            dra < 0.002 && ddec < 0.002,
            "swap stays in-tile ({dra}, {ddec})"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dod3_epoch_proper_motion_plumbs_end_to_end() {
        // Fixture record â†’ ProperMotion â†’ extrapolate at three game years:
        // displacement within the 1% PO tolerance, validity flags the
        // Â±1000 yr window (ADR-020).
        let star = StarRecord {
            source_id: 1,
            ra_rad: 2.0,
            dec_rad: -0.3,
            pm_ra_mas_yr: 50.0,
            pm_dec_mas_yr: -25.0,
            g_mag: 10.0,
            bp_rp: 0.7,
        };
        let pm = star.proper_motion();
        assert_eq!((pm.ra_rad, pm.dec_rad), (2.0, -0.3));
        let mas_per_rad = game_engine::physics::proper_motion::MAS_PER_RAD;
        for (years, in_window) in [
            (0.0, true),
            (PROPER_MOTION_SPAN_YR, true),
            (PROPER_MOTION_SPAN_YR + 1.0, false),
        ] {
            let (ra, dec, validity) = extrapolate(pm, years);
            assert_eq!(validity.in_window(), in_window, "year offset {years}");
            let got =
                (((ra - 2.0) * mas_per_rad).powi(2) + ((dec + 0.3) * mas_per_rad).powi(2)).sqrt();
            let expected = (50.0f64.powi(2) + 25.0f64.powi(2)).sqrt() * years;
            if years > 0.0 {
                assert!((got - expected).abs() / expected < 0.01, "1% tolerance");
            } else {
                assert_eq!(got, 0.0);
            }
        }
        // Tile-epoch helper agrees with the span constant.
        assert!(game_engine::catalog::format::epoch_validity(GAIA_EPOCH_YR, 2016.0).in_window());
        assert!(!game_engine::catalog::format::epoch_validity(GAIA_EPOCH_YR, 5000.0).in_window());
        // Model metadata keeps an uncooked tile's fallback alive (DoD-2
        // companion: no manifest needed for the sky to stay complete).
        let meta = model_meta(7, ORDER, ang2pix(ORDER, 0.5, 0.1).expect("coords"));
        assert!((1..=20_000).contains(&meta.count));
    }
}
