//! Thread-pool tile loader (spec §4: <100 ms async tile-load once data
//! is local).
//!
//! `std::thread` workers + `mpsc` channels — no async runtime, no new
//! dependency (spec §10 PO decision). File reads and decode happen
//! off-thread; the main thread drains completions with the non-blocking
//! [`TileLoader::poll`], so the loader never couples to the sim tick.
//! Decode is pure, so arrival order is the only timing-dependent
//! quantity: tile *content* and the fallback are deterministic, and no
//! loader output ever reaches simulation state (determinism invariant).
//!
//! ```
//! use game_engine::catalog::cache::TileKey;
//! use game_engine::catalog::io::TileLoader;
//! use std::path::PathBuf;
//!
//! let mut loader = TileLoader::try_spawn(1).expect("threads spawn");
//! loader.request(game_engine::catalog::io::LoadRequest {
//!     key: TileKey { order: 4, pixel: 9 },
//!     path: PathBuf::from("/nonexistent/9.tile"),
//!     generation: 1,
//! });
//! assert_eq!(loader.in_flight_count(), 1);
//! // Missing file completes as an Io error, never a hang.
//! let mut results = Vec::new();
//! for _ in 0..500 {
//!     results.extend(loader.poll());
//!     if !results.is_empty() {
//!         break;
//!     }
//!     std::thread::sleep(std::time::Duration::from_millis(1));
//! }
//! assert_eq!(results.len(), 1);
//! assert!(results[0].outcome.is_err());
//! ```

use crate::catalog::cache::TileKey;
use crate::catalog::format::{DecodedTile, TileError, decode_tile};
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Instant;

/// One tile fetch: explicit path (the scheduler builds cooker-layout
/// paths; the loader holds no layout policy).
#[derive(Clone, Debug)]
pub struct LoadRequest {
    /// Tile to load.
    pub key: TileKey,
    /// Tile file path.
    pub path: PathBuf,
    /// Scheduler generation that issued the request (callers drop
    /// stale generations; content is generation-independent).
    pub generation: u64,
}

/// Completed fetch with its wall-clock cost.
#[derive(Debug)]
pub struct LoadResult {
    /// Requested tile.
    pub key: TileKey,
    /// Echo of the request generation.
    pub generation: u64,
    /// Decoded tile or the failure.
    pub outcome: Result<DecodedTile, LoadError>,
    /// File-read + decode time in milliseconds (feeds `LatencyStats`).
    pub elapsed_ms: f64,
}

/// Loader failure modes (transport only — corrupt bytes are
/// [`TileError`], reported here without panicking).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Filesystem failure (message).
    Io(String),
    /// Tile failed [`decode_tile`] validation.
    Decode(TileError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(message) => write!(f, "tile io: {message}"),
            LoadError::Decode(error) => write!(f, "tile decode: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Fixed worker pool over an `mpsc` request channel.
pub struct TileLoader {
    tx: Option<mpsc::Sender<LoadRequest>>,
    rx: mpsc::Receiver<LoadResult>,
    in_flight: HashSet<TileKey>,
    workers: Vec<JoinHandle<()>>,
}

impl TileLoader {
    /// Spawn `worker_count` loader threads (clamped to 1–8; 2 is the
    /// mobile-safe floor per the plan). Fails cleanly when threads
    /// cannot spawn (resource exhaustion is an error, never a panic).
    pub fn try_spawn(worker_count: usize) -> Result<Self, String> {
        let count = worker_count.clamp(1, 8);
        let (request_tx, request_rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>();
        // Share the receiver across workers (Mutex, std only).
        let shared_rx = std::sync::Arc::new(std::sync::Mutex::new(request_rx));
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            let rx = std::sync::Arc::clone(&shared_rx);
            let tx = result_tx.clone();
            let worker = std::thread::Builder::new()
                .name(format!("catalog-loader-{index}"))
                .spawn(move || {
                    worker_loop(&rx, &tx);
                })
                .map_err(|error| format!("loader thread {index} failed to spawn: {error}"))?;
            workers.push(worker);
        }
        Ok(Self {
            tx: Some(request_tx),
            rx: result_rx,
            in_flight: HashSet::new(),
            workers,
        })
    }

    /// Queue a fetch. Returns false (without queuing) when the tile is
    /// already in flight — the scheduler replans freely, the loader
    /// never double-reads.
    pub fn request(&mut self, request: LoadRequest) -> bool {
        if !self.in_flight.insert(request.key) {
            return false;
        }
        match &self.tx {
            Some(tx) => tx.send(request).is_ok(),
            None => false,
        }
    }

    /// Drain completed fetches without blocking. Callers insert fresh
    /// results into the cache and feed `elapsed_ms` to `LatencyStats`.
    pub fn poll(&mut self) -> Vec<LoadResult> {
        let mut out = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            self.in_flight.remove(&result.key);
            out.push(result);
        }
        out
    }

    /// Fetches with no result yet.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
}

impl Drop for TileLoader {
    fn drop(&mut self) {
        // Disconnect first so workers see EOF, then join (never detach:
        // a hanging filesystem must not outlive the loader silently —
        // workers only block on the channel and on reads).
        self.tx = None;
        while let Some(worker) = self.workers.pop() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(rx: &std::sync::Mutex<mpsc::Receiver<LoadRequest>>, tx: &mpsc::Sender<LoadResult>) {
    loop {
        let request = rx.lock().expect("loader channel mutex").recv();
        let request = match request {
            Ok(request) => request,
            Err(_) => break,
        };
        let started = Instant::now();
        let outcome = load_tile(&request.path);
        let result = LoadResult {
            key: request.key,
            generation: request.generation,
            outcome,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        };
        if tx.send(result).is_err() {
            break;
        }
    }
}

fn load_tile(path: &std::path::Path) -> Result<DecodedTile, LoadError> {
    let bytes = std::fs::read(path)
        .map_err(|error| LoadError::Io(format!("{}: {error}", path.display())))?;
    decode_tile(&bytes).map_err(LoadError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::format::{
        CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader, encode_tile,
    };

    fn tile_bytes() -> Vec<u8> {
        encode_tile(
            &TileHeader {
                order: 4,
                pixel: 9,
                mean_g_mag: 15.0,
                mean_bp_rp: 1.0,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            &[StarRecord {
                source_id: 3,
                ra_rad: 0.5,
                dec_rad: 0.1,
                pm_ra_mas_yr: 0.0,
                pm_dec_mas_yr: 0.0,
                g_mag: 15.0,
                bp_rp: 1.0,
            }],
        )
    }

    fn drain(loader: &mut TileLoader) -> Vec<LoadResult> {
        let mut out = Vec::new();
        for _ in 0..500 {
            out.extend(loader.poll());
            if !out.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        out
    }

    fn temp_tile(name: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("game_loader_{name}_{}", std::process::id()));
        std::fs::write(&path, bytes).expect("fixture writes");
        path
    }

    #[test]
    fn round_trip_load_reports_latency() {
        let path = temp_tile("ok", &tile_bytes());
        let mut loader = TileLoader::try_spawn(1).expect("spawns");
        let key = TileKey { order: 4, pixel: 9 };
        assert!(loader.request(LoadRequest {
            key,
            path: path.clone(),
            generation: 7,
        }));
        assert_eq!(loader.in_flight_count(), 1);
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, key);
        assert_eq!(results[0].generation, 7);
        let tile = results[0].outcome.as_ref().expect("decodes");
        assert_eq!(tile.stars.len(), 1);
        assert!(results[0].elapsed_ms >= 0.0);
        assert_eq!(loader.in_flight_count(), 0);
        std::fs::remove_file(&path).expect("cleanup");
    }

    #[test]
    fn duplicate_requests_dedupe_in_flight() {
        let path = temp_tile("dupe", &tile_bytes());
        let mut loader = TileLoader::try_spawn(2).expect("spawns");
        let key = TileKey { order: 4, pixel: 9 };
        let req = || LoadRequest {
            key,
            path: path.clone(),
            generation: 1,
        };
        assert!(loader.request(req()));
        assert!(!loader.request(req()), "second queues nothing");
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        std::fs::remove_file(&path).expect("cleanup");
    }

    #[test]
    fn corrupt_bytes_and_missing_files_report_cleanly() {
        let bad_path = temp_tile("bad", b"not a tile");
        let mut loader = TileLoader::try_spawn(1).expect("spawns");
        loader.request(LoadRequest {
            key: TileKey { order: 4, pixel: 1 },
            path: bad_path.clone(),
            generation: 1,
        });
        loader.request(LoadRequest {
            key: TileKey { order: 4, pixel: 2 },
            path: PathBuf::from("/nonexistent-catalog/2.tile"),
            generation: 1,
        });
        let mut results = drain(&mut loader);
        results.extend(drain(&mut loader));
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|r| matches!(r.outcome, Err(LoadError::Decode(_)))),
            "corrupt bytes decode-fail"
        );
        assert!(
            results
                .iter()
                .any(|r| matches!(r.outcome, Err(LoadError::Io(_)))),
            "missing file io-fails"
        );
        std::fs::remove_file(&bad_path).expect("cleanup");
    }
}
