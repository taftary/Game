//! Catalog sky runtime for the Planet View backdrop (window- and
//! GPU-free; the `game_debug` binary owns buffers + draw).
//!
//! [`CatalogSky`] couples the engine catalog pieces into one live loop:
//! [`Scheduler`](game_engine::catalog::scheduler::Scheduler) plans,
//! [`TileLoader`](game_engine::catalog::io::TileLoader) fetches,
//! [`TileCache`](game_engine::catalog::cache::TileCache) holds,
//! [`StarRegistry`](game_engine::catalog::ids::StarRegistry) resolves,
//! and missing planned tiles expand through the deterministic fallback
//! (manifest metadata when cooked, procedural model otherwise) — so the
//! sky is never blank, with or without tile files on disk.
//!
//! Model-only mode (no manifest at the tile root): the loader stays
//! idle and every planned tile renders fallback. Failed keys are
//! remembered and never re-requested (tiles cook offline; a missing
//! file will not appear at runtime).
//!
//! Budgets (tier gates, Low must ship): at most
//! [`MAX_REQUESTS_PER_UPDATE`] loader requests per update, fallback
//! units past [`MAX_FALLBACK_UNITS`] aggregate to coarser ancestors,
//! [`MAX_POINTS_PER_FRAME`] points total shared fairly across units
//! (even coverage, never a center patch over blank edges).

use game_engine::catalog::cache::{TileCache, TileKey};
use game_engine::catalog::fallback::{TileMeta, fallback_star, model_meta, parse_manifest};
use game_engine::catalog::ids::StarRegistry;
use game_engine::catalog::io::{LoadRequest, TileLoader};
use game_engine::catalog::metrics::LatencyStats;
use game_engine::catalog::scheduler::{PRIORITY_FRUSTUM, PlannedRange, Scheduler, SkyView};
use game_engine::render::stars::{
    SKY_SHELL_RADIUS, StarPoint, crossfade_alpha, expand_fallback_star,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

/// Catalog order served to the Planet View backdrop.
pub const SKY_ORDER: u8 = 12;
/// Loader requests issued per update (I/O pacing).
pub const MAX_REQUESTS_PER_UPDATE: usize = 8;
/// Missing-tile units expanded individually past this count; more
/// units aggregate to coarser ancestors (full-sky coverage at any
/// frustum width).
pub const MAX_FALLBACK_UNITS: usize = 8192;
/// Total points per frame (tier gate).
pub const MAX_POINTS_PER_FRAME: usize = 500_000;

/// Dev-overlay summary (text rows only — tile state never leaks past
/// the debug viewer).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkySummary {
    /// Resident catalog tiles.
    pub resident_tiles: usize,
    /// Planned tiles rendering fallback.
    pub fallback_tiles: usize,
    /// Catalog stars expanded.
    pub resident_stars: usize,
    /// Fallback stars expanded.
    pub fallback_stars: usize,
    /// Cache bytes.
    pub cache_bytes: usize,
    /// Load p50 in ms (0.0 = no samples yet).
    pub p50_ms: f64,
    /// Load p95 in ms (0.0 = no samples yet).
    pub p95_ms: f64,
    /// True when no manifest was found (procedural sky only).
    pub model_only: bool,
}

/// Sorted range-set minus sorted points (single sweep, no per-point
/// binary search): resident tiles punch holes in plan ranges.
fn subtract_sorted_points(
    ranges: &[std::ops::Range<u64>],
    points: &[u64],
) -> Vec<std::ops::Range<u64>> {
    let mut out = Vec::with_capacity(ranges.len());
    let mut pi = 0usize;
    for range in ranges {
        let mut start = range.start;
        while pi < points.len() && points[pi] < start {
            pi += 1;
        }
        while pi < points.len() && points[pi] < range.end {
            if points[pi] > start {
                out.push(start..points[pi]);
            }
            start = points[pi] + 1;
            pi += 1;
            if start >= range.end {
                break;
            }
        }
        if start < range.end {
            out.push(start..range.end);
        }
    }
    out
}

/// Decompose `[start, end)` into aligned nested blocks `(pixel, depth)`
/// at `order`: largest power-of-four-aligned blocks first (each block
/// is one subtree — the scheduler's range primitive, inverted).
fn decompose_range(start: u64, end: u64, order: u8) -> Vec<(u64, u8)> {
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let remaining = end - cursor;
        let mut level = 0u32;
        while level < u32::from(order)
            && 4u64.pow(level + 1) <= remaining
            && cursor.is_multiple_of(4u64.pow(level + 1))
        {
            level += 1;
        }
        out.push((cursor >> (2 * level), order - level as u8));
        cursor += 4u64.pow(level);
    }
    out
}

/// Planner worker: latest-view coalescing (intermediate views skip —
/// only the newest pending view plans), one [`Scheduler`] per worker,
/// results posted back unordered (the frame loop keeps the latest).
fn planner_loop(
    order: u8,
    tile_root: PathBuf,
    views: &std::sync::mpsc::Receiver<SkyView>,
    results: &std::sync::mpsc::Sender<game_engine::catalog::scheduler::CatalogPlan>,
) {
    let scheduler = match Scheduler::new(order, tile_root) {
        Some(scheduler) => scheduler,
        None => return,
    };
    let mut scheduler = scheduler;
    while let Ok(mut view) = views.recv() {
        // Coalesce: a newer view supersedes everything pending.
        while let Ok(newer) = views.try_recv() {
            view = newer;
        }
        if let Some(plan) = scheduler.plan(&view)
            && results.send(plan).is_err()
        {
            break;
        }
    }
}

impl Drop for CatalogSky {
    fn drop(&mut self) {
        // Disconnect first so the planner sees EOF, then join (never
        // detach: a planning thread must not outlive the sky silently).
        self.plan_tx = None;
        if let Some(planner) = self.planner.take() {
            let _ = planner.join();
        }
    }
}

/// Live catalog sky: plan → fetch → hold → expand.
pub struct CatalogSky {
    loader: TileLoader,
    cache: TileCache,
    registry: StarRegistry,
    manifest: Option<game_engine::catalog::fallback::ManifestIndex>,
    stats: LatencyStats,
    master_seed: u64,
    tile_root: PathBuf,
    order: u8,
    frame: u64,
    failed: HashSet<TileKey>,
    birth: HashMap<TileKey, Instant>,
    last_plan: Vec<PlannedRange>,
    plan_tx: Option<std::sync::mpsc::Sender<SkyView>>,
    plan_rx: std::sync::mpsc::Receiver<game_engine::catalog::scheduler::CatalogPlan>,
    plan_pending: bool,
    planner: Option<std::thread::JoinHandle<()>>,
    expanded: Vec<StarPoint>,
    expanded_dirty: bool,
    summary: SkySummary,
}

impl CatalogSky {
    /// Open the sky over `tile_root` (`<root>/orderNN/*.tile` +
    /// `<root>/manifest.json`, the cooker layout). Missing manifest ⇒
    /// model-only mode (logged, never an error). Returns `None` for bad
    /// orders or thread spawn failure. Planning runs on a worker thread
    /// (order-12 plans cost ~0.3 s — never on the frame): the frame loop
    /// posts views and consumes the latest completed plan.
    pub fn open(tile_root: PathBuf, order: u8, master_seed: u64) -> Option<Self> {
        Scheduler::new(order, tile_root.clone())?;
        let loader = TileLoader::try_spawn(2).ok()?;
        let (plan_tx, plan_view_rx) = std::sync::mpsc::channel::<SkyView>();
        let (plan_result_tx, plan_rx) =
            std::sync::mpsc::channel::<game_engine::catalog::scheduler::CatalogPlan>();
        let root = tile_root.clone();
        let planner = std::thread::Builder::new()
            .name("catalog-planner".to_string())
            .spawn(move || {
                planner_loop(order, root, &plan_view_rx, &plan_result_tx);
            })
            .ok()?;
        let manifest_path = tile_root.join("manifest.json");
        let manifest = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|text| parse_manifest(&text).ok());
        if manifest.is_none() {
            tracing::warn!(
                path = %manifest_path.display(),
                "catalog manifest missing: procedural fallback sky only"
            );
        }
        Some(Self {
            loader,
            cache: TileCache::with_default_budget(),
            registry: StarRegistry::new(master_seed),
            manifest,
            stats: LatencyStats::new(),
            master_seed,
            tile_root,
            order,
            frame: 0,
            failed: HashSet::new(),
            birth: HashMap::new(),
            last_plan: Vec::new(),
            plan_tx: Some(plan_tx),
            plan_rx,
            plan_pending: false,
            planner: Some(planner),
            expanded: Vec::new(),
            expanded_dirty: true,
            summary: SkySummary {
                resident_tiles: 0,
                fallback_tiles: 0,
                resident_stars: 0,
                fallback_stars: 0,
                cache_bytes: 0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                model_only: false,
            },
        })
    }

    /// Current plan order (mirrors the scheduler this sky was opened with).
    pub fn order(&self) -> u8 {
        self.order
    }

    /// True when no manifest was found (procedural sky only).
    pub fn is_model_only(&self) -> bool {
        self.manifest.is_none()
    }

    /// Advance one frame: post the view to the planner worker,
    /// consume completed plans, drain loader completions, issue requests
    /// within budget. Never blocks (all channels polled, decode happened
    /// off-thread).
    pub fn update(&mut self, view: &SkyView) {
        self.frame = self.frame.wrapping_add(1);
        // Post the latest view whenever the planner is free (its
        // throughput self-limits); coalescing happens worker-side.
        if !self.plan_pending
            && let Some(tx) = &self.plan_tx
            && tx.send(*view).is_ok()
        {
            self.plan_pending = true;
        }
        // Consume completed plans (latest wins; identical plans skip
        // the rebuild).
        while let Ok(plan) = self.plan_rx.try_recv() {
            self.plan_pending = false;
            if plan.ranges == self.last_plan {
                continue;
            }
            let ranked: Vec<(TileKey, u8)> = self
                .cache
                .resident()
                .into_iter()
                .map(|(key, _)| {
                    let priority = plan
                        .ranges
                        .iter()
                        .find(|planned| planned.range.contains(&key.pixel))
                        .map(|planned| planned.priority)
                        .unwrap_or(0);
                    (key, priority)
                })
                .collect();
            self.cache.note_priorities(&ranked);
            self.issue_requests(&plan.ranges);
            self.last_plan = plan.ranges;
            self.expanded_dirty = true;
        }
        self.drain_completions();
    }

    /// Issue loader requests for planned-but-missing tiles, up to
    /// [`MAX_REQUESTS_PER_UPDATE`]. Model-only skies and remembered
    /// failures issue nothing. The walk is scan-capped (plans hold
    /// millions of pixels; ranges arrive priority-first, so the cap
    /// keeps the highest-priority head).
    fn issue_requests(&mut self, ranges: &[game_engine::catalog::scheduler::PlannedRange]) {
        if self.manifest.is_none() {
            return;
        }
        const SCAN_CAP: usize = 65_536;
        let order = self.order;
        let mut issued = 0usize;
        let mut scanned = 0usize;
        'ranges: for planned in ranges {
            for pixel in planned.range.clone() {
                scanned += 1;
                if scanned > SCAN_CAP {
                    break 'ranges;
                }
                let key = TileKey { order, pixel };
                if self.cache.contains(&key) || self.failed.contains(&key) {
                    continue;
                }
                if issued >= MAX_REQUESTS_PER_UPDATE {
                    break 'ranges;
                }
                let path = game_engine::catalog::scheduler::tile_path(
                    &self.tile_root,
                    key.order,
                    key.pixel,
                );
                if self.loader.request(LoadRequest {
                    key,
                    path,
                    generation: 0,
                }) {
                    issued += 1;
                }
            }
        }
    }

    /// Drain finished fetches into the cache + registry + metrics.
    fn drain_completions(&mut self) {
        for result in self.loader.poll() {
            self.stats.record(result.elapsed_ms);
            match result.outcome {
                Ok(tile) => {
                    let key = TileKey {
                        order: tile.header.order,
                        pixel: tile.header.pixel,
                    };
                    match self.cache.insert(key, tile.clone()) {
                        Ok(()) => {
                            self.registry.index_tile(key, &tile);
                            self.birth.insert(key, Instant::now());
                            self.expanded_dirty = true;
                        }
                        Err(full) => {
                            tracing::warn!("catalog cache full ({full}): tile dropped");
                        }
                    }
                }
                Err(error) => {
                    tracing::debug!("catalog tile {} failed: {error}", result.key);
                    self.failed.insert(result.key);
                }
            }
        }
    }

    /// Expanded backdrop points (camera-independent: shell around the
    /// camera origin of the relative frame) plus whether the set
    /// changed since the last call (the binary re-uploads only then).
    pub fn points(&mut self, camera_world: glam::DVec3) -> (&[StarPoint], bool) {
        if self.expanded_dirty {
            self.rebuild(camera_world);
            self.expanded_dirty = false;
            let changed = true;
            return (&self.expanded, changed);
        }
        (&self.expanded, false)
    }

    fn rebuild(&mut self, camera_world: glam::DVec3) {
        let order = self.order;
        let now = Instant::now();
        let started = Instant::now();
        // Deterministic buffer order: sorted resident keys first, then
        // fallback units in plan order.
        let mut resident: Vec<TileKey> = self
            .cache
            .resident()
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        resident.sort();
        // Missing units: per-tile fallback, or coarser aggregates when
        // the missing set exceeds the unit cap (wide frustum at order
        // 12 holds millions of tiles — aggregates keep full coverage
        // instead of rendering a center patch over blank edges).
        let missing = self.missing_units(order);
        let units = resident.len() + missing.len();
        // Fair point budget per unit (at least one): even coverage, no
        // blank patches, deterministic stride within each unit.
        let budget = (MAX_POINTS_PER_FRAME / units.max(1)).max(1);
        let mut points = Vec::new();
        let mut resident_tiles = 0usize;
        let mut resident_stars = 0usize;
        for key in resident {
            let age_ms = self
                .birth
                .get(&key)
                .map(|born| now.duration_since(*born).as_secs_f64() * 1000.0)
                .unwrap_or(f64::INFINITY);
            if let Some(tile) = self.cache.get(&key) {
                resident_tiles += 1;
                let count = tile.stars.len();
                let stride = count.max(1).div_ceil(budget).max(1);
                let alpha = crossfade_alpha(age_ms);
                for star in tile.stars.iter().step_by(stride) {
                    if let Some(point) = game_engine::render::stars::expand_catalog_record(
                        star,
                        camera_world,
                        SKY_SHELL_RADIUS,
                        alpha,
                    ) {
                        points.push(point);
                        resident_stars += 1;
                    }
                }
            }
        }
        let mut fallback_tiles = 0usize;
        for (pixel, depth) in missing {
            let meta = self.meta_for(depth, pixel);
            let count = meta.count as usize;
            let stride = count.max(1).div_ceil(budget).max(1) as u64;
            let mut slot = 0u64;
            let mut drew = 0usize;
            while slot < count as u64 {
                if let Some(star) =
                    fallback_star(self.master_seed, depth, pixel, slot as u32, &meta)
                    && let Some(point) =
                        expand_fallback_star(&star, camera_world, SKY_SHELL_RADIUS, 1.0)
                {
                    points.push(point);
                    drew += 1;
                }
                slot += stride;
            }
            if drew > 0 {
                fallback_tiles += 1;
            }
        }
        points.truncate(MAX_POINTS_PER_FRAME);
        let fallback_stars = points.len().saturating_sub(resident_stars);
        let rebuild_ms = started.elapsed().as_secs_f64() * 1000.0;
        if rebuild_ms > 50.0 {
            // Visibility into motion-rebuild cost (wide-frustum model
            // skies expand ~500k lattice points): debug builds only.
            tracing::debug!(rebuild_ms, points = points.len(), "sky rebuild over budget");
        }
        self.expanded = points;
        let (p50, p95) = self.stats.percentiles().unwrap_or((0.0, 0.0));
        self.summary = SkySummary {
            resident_tiles,
            fallback_tiles,
            resident_stars,
            fallback_stars,
            cache_bytes: self.cache.memory_bytes(),
            p50_ms: p50,
            p95_ms: p95,
            model_only: self.manifest.is_none(),
        };
    }

    /// Planned-but-missing fallback units `(pixel, depth)`: plan
    /// ranges minus residents (sweep, no pixel enumeration), decomposed
    /// into aligned blocks, then aggregated by parent while over
    /// [`MAX_FALLBACK_UNITS`]. Wide frustums at order 12 hold millions
    /// of tiles — this path never enumerates them; coverage stays full
    /// instead of degrading to a center patch over blank edges.
    fn missing_units(&self, order: u8) -> Vec<(u64, u8)> {
        let split = self
            .last_plan
            .partition_point(|planned| planned.priority == PRIORITY_FRUSTUM);
        let (tight, travel) = self.last_plan.split_at(split);
        let mut residents: Vec<u64> = self
            .cache
            .resident()
            .into_iter()
            .map(|(key, _)| key.pixel)
            .collect();
        residents.sort();
        residents.dedup();
        let mut units: Vec<(u64, u8)> = Vec::new();
        for group in [tight, travel] {
            let group_ranges: Vec<std::ops::Range<u64>> =
                group.iter().map(|planned| planned.range.clone()).collect();
            for range in subtract_sorted_points(&group_ranges, &residents) {
                units.extend(decompose_range(range.start, range.end, order));
            }
        }
        while units.len() > MAX_FALLBACK_UNITS {
            units.sort_by_key(|(pixel, depth)| (*depth, pixel >> 2));
            let mut next: Vec<(u64, u8)> = Vec::new();
            let mut index = 0usize;
            while index < units.len() {
                let (pixel, depth) = units[index];
                if depth == 0 {
                    next.push((pixel, depth));
                    index += 1;
                    continue;
                }
                let parent = pixel >> 2;
                let mut end = index + 1;
                while end < units.len() && units[end].1 == depth && units[end].0 >> 2 == parent {
                    end += 1;
                }
                next.push((parent, depth - 1));
                index = end;
            }
            if next.len() >= units.len() {
                if next.len() <= 1 {
                    break;
                }
                // Stalled (sparse singletons): merge consecutive pairs
                // under the first key, dropping the second's placement.
                // Approximate, deterministic, and halves the count —
                // termination is structural.
                let mut paired = Vec::with_capacity(next.len() / 2 + 1);
                let mut iter = next.into_iter();
                while let Some(first) = iter.next() {
                    let _ = iter.next();
                    paired.push(first);
                }
                next = paired;
            }
            units = next;
        }
        units
    }

    /// Metadata for one tile: manifest entry or the procedural model.
    fn meta_for(&self, order: u8, pixel: u64) -> TileMeta {
        self.manifest
            .as_ref()
            .and_then(|index| index.get(order, pixel))
            .unwrap_or_else(|| model_meta(self.master_seed, order, pixel))
    }

    /// Latest overlay summary (valid after [`points`](Self::points)).
    pub fn summary(&self) -> SkySummary {
        self.summary
    }

    /// True once the planner worker has delivered a plan.
    pub fn has_plan(&self) -> bool {
        !self.last_plan.is_empty()
    }

    /// GPU-free self-check for CI: model-only sky, synthetic orbiting
    /// views, asserts the loop invariants, returns the report line.
    /// Panics loudly on regression (headless contract).
    pub fn headless_check() -> String {
        use game_engine::catalog::scheduler::SkyView;
        use glam::DVec3;
        use glam::dcamera::rh::proj::directx::perspective;
        use glam::dcamera::rh::view::look_at_mat4;

        let root = std::env::temp_dir().join("game_sky_headless_none");
        let _ = std::fs::remove_dir_all(&root);
        let mut sky = CatalogSky::open(root, SKY_ORDER, 99).expect("sky opens model-only");
        assert!(sky.is_model_only());
        let view_at = |yaw: f64| {
            let forward = DVec3::new(yaw.cos(), 0.2, yaw.sin()).normalize();
            let eye = -forward * 5.0;
            let view = look_at_mat4(eye, DVec3::ZERO, DVec3::Y);
            let proj = perspective(60.0_f64.to_radians(), 16.0 / 9.0, 0.05, 1000.0);
            (
                forward,
                SkyView {
                    view_proj: proj * view,
                    cam_forward_world: forward,
                    fov_y_rad: 60.0_f64.to_radians(),
                    aspect: 16.0 / 9.0,
                    world_to_equatorial: game_engine::render::stars::WORLD_TO_EQUATORIAL,
                    velocity_world: DVec3::ZERO,
                },
            )
        };
        // Planning is async: wait for the first plan (bounded —
        // ~0.3 s normally) before asserting frame content.
        for _ in 0..600 {
            let (_, view) = view_at(0.0);
            sky.update(&view);
            if sky.has_plan() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(sky.has_plan(), "planner worker delivered no plan");
        for frame in 0..90u64 {
            let yaw = frame as f64 * 0.05;
            let (_, view) = view_at(yaw);
            sky.update(&view);
            let (points, _) = sky.points(DVec3::new(-5.0, 0.0, 0.0));
            assert!(!points.is_empty(), "sky never blank (frame {frame})");
        }
        let summary = sky.summary();
        assert!(summary.fallback_tiles > 0, "model fallback served");
        assert_eq!(summary.resident_tiles, 0, "model-only has no catalog");
        format!(
            "sky order={} fallback_tiles={} fallback_stars={} model_only={}",
            SKY_ORDER, summary.fallback_tiles, summary.fallback_stars, summary.model_only
        )
    }
}
