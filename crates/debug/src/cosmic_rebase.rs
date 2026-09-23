//! Cosmic rebase worker (v0.3.4 `cosmic-rebase-async`, ADR-026 §3):
//! non-blocking origin rebase for the Game Demo glow buffer.
//!
//! `cosmic-gpu-tracers` CGT-010 shrank the job to glow only: tracers
//! never need a rebase rebuild (origin rides a push constant), so the
//! worker rebuilds the hub/member glow list and nothing else.
//!
//! Window- and GPU-free: the worker owns `Arc` clones of the immutable
//! inputs (`WebDescriptor` / `WebField`) and produces plain `Vec`s; it
//! never touches `vulkano` objects — the main thread owns every upload
//! and swap (boundary rule recorded in
//! `docs/techstack/architecture.md`). Shape follows the house pattern
//! (`engine::catalog::io::TileLoader`, `debug::sky` planner):
//! `std::thread` + `mpsc`, generation-tagged requests, latest-wins
//! coalescing, `try_recv` per frame, join on drop. No async runtime.

use super::cosmic_veil::{VeilMode, veil_sprites};
use game_engine::universe::{WebDescriptor, WebField};
use glam::DVec3;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

/// One origin-relative glow point: `(pos_mpc, color, misc)` — the same
/// layout as `cosmic_hubs::{hub_impostors, hub_members}` and
/// `cosmic_veil::veil_sprites` outputs. The binary maps these to
/// vertices with one shared helper, so the worker and synchronous
/// paths produce identical bytes by construction.
pub type GlowPoint = ([f32; 3], [f32; 3], [f32; 3]);

/// Build the demo glow list (shared synchronous path + worker): veil
/// sprites (sprites mode only, stride-2 subset per the Low budget
/// precedent) + hub members + hub impostors, in draw order (veil,
/// then members, then impostors so cores top the scatter). Pure
/// function of its inputs — the FR7 bit-identity pin.
pub fn build_demo_glow(
    web: &WebDescriptor,
    field: &WebField,
    seed: u64,
    origin: DVec3,
    veil_mode: VeilMode,
) -> Vec<GlowPoint> {
    let mut out: Vec<GlowPoint> = Vec::new();
    if matches!(veil_mode, VeilMode::Sprites) {
        out.extend(veil_sprites(field, origin).iter().step_by(2).cloned());
    }
    out.extend(super::cosmic_hubs::hub_members(web, seed, origin));
    out.extend(super::cosmic_hubs::hub_impostors(web, origin));
    out
}

/// Rebase request: rebuild the demo glow buffer relative to `origin`.
#[derive(Clone, Copy, Debug)]
pub struct RebaseJob {
    /// Buffer upload origin the result is relative to (ship position
    /// at request time — the swap moves the render origin to it).
    pub origin: DVec3,
    /// Frame-thread generation; only the matching result swaps.
    pub generation: u64,
}

/// Rebase result: ready-to-upload CPU data for one generation. Carries
/// only `Vec` + `DVec3` + `u64` — never a `vulkano` object (A-1).
#[derive(Clone, Debug)]
pub struct RebaseResult {
    /// Echo of the request generation (stale generations drop).
    pub generation: u64,
    /// Origin the vectors are relative to (swap target).
    pub origin: DVec3,
    /// Glow points in draw order (see [`build_demo_glow`]).
    pub glow: Vec<GlowPoint>,
}

/// Outstanding-request bookkeeping (frame-thread side): at most one
/// request is ever in flight — a newer crossing waits for the swap,
/// then re-requests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RebasePending {
    /// Generation of the in-flight request.
    pub generation: u64,
    /// Origin the in-flight request builds for.
    pub origin: DVec3,
    /// Frame counter value at request time (telemetry denominator).
    pub request_frame: u64,
}

/// Rebase worker: one background thread rebuilding the demo glow buffer
/// off-frame. Latest-wins: a newer request supersedes everything still
/// queued — the worker drains its inbox before computing.
pub struct RebaseWorker {
    tx: Option<mpsc::Sender<RebaseJob>>,
    rx: mpsc::Receiver<RebaseResult>,
    handle: Option<JoinHandle<()>>,
}

impl RebaseWorker {
    /// Spawn the worker over `Arc` clones of the seed's immutable
    /// inputs. Fails cleanly when the thread cannot spawn (the shell
    /// falls back to the synchronous demo rebuild).
    pub fn try_spawn(
        web: Arc<WebDescriptor>,
        field: Arc<WebField>,
        seed: u64,
        veil_mode: VeilMode,
    ) -> Result<Self, String> {
        let (job_tx, job_rx) = mpsc::channel::<RebaseJob>();
        let (res_tx, res_rx) = mpsc::channel::<RebaseResult>();
        let handle = std::thread::Builder::new()
            .name("cosmic-rebase".to_string())
            .spawn(move || rebase_loop(&job_rx, &res_tx, &web, &field, seed, veil_mode))
            .map_err(|error| format!("cosmic rebase thread failed to spawn: {error}"))?;
        Ok(Self {
            tx: Some(job_tx),
            rx: res_rx,
            handle: Some(handle),
        })
    }

    /// Queue a rebuild at `origin` for `generation`. Fire-and-forget:
    /// a send failure (worker gone) is ignored — the frame thread
    /// keeps drawing the old buffers and retries next crossing.
    pub fn request(&self, origin: DVec3, generation: u64) {
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.send(RebaseJob { origin, generation });
        }
    }

    /// Drain completed results without blocking, returning the newest.
    /// The caller swaps only when it matches the pending generation.
    pub fn poll(&self) -> Option<RebaseResult> {
        let mut latest = None;
        while let Ok(result) = self.rx.try_recv() {
            latest = Some(result);
        }
        latest
    }
}

impl Drop for RebaseWorker {
    fn drop(&mut self) {
        // Disconnect first so the worker sees EOF, then join (never
        // detach: a rebuild thread must not outlive the shell
        // silently — the `TileLoader` / planner precedent).
        self.tx = None;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Worker loop: blocking `recv`, latest-wins drain, compute, send.
/// Pure builds only — no Vulkan object is ever touched here.
fn rebase_loop(
    jobs: &mpsc::Receiver<RebaseJob>,
    results: &mpsc::Sender<RebaseResult>,
    web: &WebDescriptor,
    field: &WebField,
    seed: u64,
    veil_mode: VeilMode,
) {
    while let Ok(mut job) = jobs.recv() {
        // Coalesce: a newer request supersedes everything pending.
        while let Ok(newer) = jobs.try_recv() {
            job = newer;
        }
        let result = RebaseResult {
            generation: job.generation,
            origin: job.origin,
            glow: build_demo_glow(web, field, seed, job.origin, veil_mode),
        };
        if results.send(result).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{WebLink, WebNode};

    /// Small deterministic fixture: 10 nodes (top-1 % hub + members)
    /// and a 2³ grid with one max-quant NODE cell so the veil-sprite
    /// branch is covered.
    fn small_fixture() -> (WebDescriptor, WebField) {
        let nodes = (0..10)
            .map(|i| WebNode {
                node_index: i,
                position_mpc: [i as f64 * 10.0, 0.0, 0.0],
                mass_msun: 1.0e15 - f64::from(i),
                virial_radius_mpc: 2.0,
            })
            .collect::<Vec<_>>();
        let web = WebDescriptor::new(7, nodes, Vec::<WebLink>::new(), Vec::new(), 0, 0.0);
        // Grid byte 0: NODE class (2) at max quant — overdensity 64,
        // above the 0.5 veil floor; rest void.
        let mut grid = vec![0u8; 8];
        grid[0] = (2 << 6) | 63;
        let field = WebField {
            displacement: vec![[0, 0, 0]; 8],
            grid,
            grid_cells: 2,
            cell_size_mpc: 4.0,
            mean_density: 1.0,
            origin_mpc: [-4.0, -4.0, -4.0],
            sphere_radius_mpc: 1.0e9,
        };
        (web, field)
    }

    /// Block until `generation` arrives (or fail after the budget).
    fn await_generation(worker: &RebaseWorker, generation: u64) -> RebaseResult {
        for _ in 0..1000 {
            if let Some(result) = worker.poll() {
                assert!(
                    result.generation <= generation,
                    "worker must never run ahead of the newest request"
                );
                if result.generation == generation {
                    return result;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("rebase generation {generation} never completed");
    }

    #[test]
    fn worker_result_matches_synchronous_build() {
        // FR7 bit-identity: one rebase via the worker vs the shared
        // synchronous function — identical glow bytes.
        let (web, field) = small_fixture();
        let origin = DVec3::new(100.0, -50.0, 25.0);
        let worker = RebaseWorker::try_spawn(
            Arc::new(web.clone()),
            Arc::new(field.clone()),
            7,
            VeilMode::Sprites,
        )
        .expect("worker spawns");
        worker.request(origin, 1);
        let result = await_generation(&worker, 1);
        assert_eq!(result.origin, origin);
        assert_eq!(
            result.glow,
            build_demo_glow(&web, &field, 7, origin, VeilMode::Sprites)
        );
        assert!(!result.glow.is_empty(), "fixture must emit glow");
        // March mode drops the veil sprites but keeps hubs: same hubs,
        // fewer points.
        let march_glow = build_demo_glow(&web, &field, 7, origin, VeilMode::March { steps: 48 });
        assert!(!march_glow.is_empty());
        assert!(march_glow.len() < result.glow.len());
    }

    #[test]
    fn latest_request_wins() {
        // Five rapid requests: the newest generation MUST complete
        // (intermediate ones may coalesce away — assert nothing about
        // them), and afterwards the inbox is empty (one result per
        // poll drain, never a stale backlog).
        let (web, field) = small_fixture();
        let worker = RebaseWorker::try_spawn(Arc::new(web), Arc::new(field), 7, VeilMode::Sprites)
            .expect("worker spawns");
        for generation in 1..=5 {
            worker.request(DVec3::new(generation as f64, 0.0, 0.0), generation);
        }
        let result = await_generation(&worker, 5);
        assert_eq!(result.origin, DVec3::new(5.0, 0.0, 0.0));
        // Settle: the worker is back on blocking recv, so no further
        // result can be in flight — the next poll is empty.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            worker.poll().is_none(),
            "no stale result may linger after the newest"
        );
    }

    #[test]
    fn poll_never_blocks() {
        // Frame-loop analog (CRA-006): `poll` is `try_recv`-only — an
        // idle worker answers instantly, never stalls the frame.
        let (web, field) = small_fixture();
        let worker = RebaseWorker::try_spawn(Arc::new(web), Arc::new(field), 7, VeilMode::Sprites)
            .expect("worker spawns");
        let started = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(worker.poll().is_none());
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "1000 idle polls must be instant"
        );
    }

    #[test]
    fn drop_joins_worker_thread() {
        // NFR3: dropping the worker joins the thread — afterwards no
        // thread holds the `Arc` inputs anymore (deterministic: `join`
        // returns only after thread end, so the count MUST be 1 with
        // no retry).
        let (web, field) = small_fixture();
        let web = Arc::new(web);
        let field = Arc::new(field);
        let worker =
            RebaseWorker::try_spawn(Arc::clone(&web), Arc::clone(&field), 7, VeilMode::Sprites)
                .expect("worker spawns");
        assert_eq!(Arc::strong_count(&web), 2);
        // One round trip proves the thread runs before we drop it.
        worker.request(DVec3::ZERO, 1);
        assert_eq!(await_generation(&worker, 1).generation, 1);
        drop(worker);
        assert_eq!(
            Arc::strong_count(&web),
            1,
            "drop must join the worker thread"
        );
        assert_eq!(Arc::strong_count(&field), 1);
    }

    #[test]
    fn builds_are_translation_consistent() {
        // The shared builds shift exactly with the origin (rebase-safe
        // by construction: only the `pos` translation changes).
        let (web, field) = small_fixture();
        let a = build_demo_glow(&web, &field, 7, DVec3::ZERO, VeilMode::Sprites);
        let shift = DVec3::new(5.0, -3.0, 2.0);
        let b = build_demo_glow(&web, &field, 7, shift, VeilMode::Sprites);
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(b.iter()) {
            for axis in 0..3 {
                let moved = p.0[axis] - q.0[axis];
                assert!((moved - shift.to_array()[axis] as f32).abs() < 1e-4);
            }
            assert_eq!(p.1, q.1);
            assert_eq!(p.2, q.2);
        }
    }
}
