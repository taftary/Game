# Notion — cosmic-rebase-async

## Status

`in-progress` (PO sign-off 2026-09-23; UX consulted; ARCHITECT consulted —
threading precedent, ADR-026 §3; DEV implementation started 2026-09-23)

## Context

In the Game Demo the application **freezes** for a visible moment
every time the ship has travelled ≈ 50 Mpc, then resumes. Root cause
(investigation 2026-09-23): `CosmicDemoState::tick` returns `true`
once `|position − upload_origin| > REBASE_DISTANCE_MPC` (50,
`cosmic_camera.rs`); `ViewerApp::tick_cosmic` (`main.rs`) then calls
`refresh_cosmic()` **synchronously inside `RedrawRequested`**, before
`draw_main`. `refresh_cosmic` rebuilds every cosmic GPU resource:

| Rebuilt on rebase | Depends on origin? | Cost |
|---|---|---|
| 128³ R8 veil volume (`upload_veil_volume`) | no | new image + staging + **blocking fence wait** |
| Full HDR / bloom / march chain (`build_hdr_chain`) | no | ≈ 2·levels + 3 images, framebuffers, ≈ 10 descriptor sets |
| Demo glow (`upload_cosmic_glow(origin)`) | yes | `hub_members` + `hub_impostors`, ≈ 50k points |
| Demo splats (`upload_cosmic_splats(origin)`) | yes | `splat_records` over ≈ 1.1M tracers (≈ 30M `HashMap` probes) + 17 MB `Buffer::from_iter` |
| Inspector glow at origin `ZERO` | no | duplicate of the above |
| Inspector splats at origin `ZERO` | no | duplicate of the above — doubles the largest cost |

The code's own comments (`main.rs` "rebuilt on reseed (never on
rebase)", "the tab never rebases") describe the intended behaviour;
the implementation does not match them. The march push already
handles the origin per frame with no rebuild (`march_push_for`).

The workspace has a house pattern for off-frame work
(`engine::catalog::io::TileLoader`: `std::thread` workers + `mpsc`,
`try_recv` in the frame, join on drop; `debug::sky` planner thread
with latest-wins coalescing), and the cosmic path uses none of it.

## Problem & Needs

- **Player:** travelling is the core loop of the cosmic demo; a stall
  every 50 Mpc breaks the sense of motion and makes speed feel
  arbitrary. The application must never pause because of where the
  ship is.
- **Developer:** the rebase path must do only what a rebase needs;
  origin-independent resources must not churn (VRAM, driver objects,
  fence stalls).
- **Determinism / safety:** whatever moves off the frame must produce
  bit-identical vertex data to the synchronous path and must never
  touch Vulkan objects from a worker.

## Goals

1. **No frame stall on rebase.** Crossing the rebase distance never
   causes a frame longer than the budget; the rebuilt buffers appear
   a few frames later without any visual snap (the old buffer,
   drawn with the old origin, stays valid until the swap because the
   camera is recentred against the *upload* origin, not the ship).
2. **Rebase rebuilds only origin-dependent resources** (glow +
   splat buffers of the demo). Veil volume, HDR chain and inspector
   buffers rebuild on reseed and swapchain recreate only, matching
   the existing comments.
3. **CPU build off-thread.** A `CosmicRebaseJob` worker owns `Arc`
   clones of `WebDescriptor` / `WebField`, receives
   `(origin, generation)`, and returns ready-to-upload vertex `Vec`s.
   Latest-wins: a newer request supersedes an in-flight result. The
   frame thread does `try_recv`, `Buffer::from_iter`, and swaps the
   `Subbuffer`; the previous buffer is released by the frame-future
   refcount as today.
4. **No fence wait in the frame loop.** The veil-volume upload's
   `wait(None)` moves to the reseed / load path only, where the
   staged `LoadPlan` already shows progress.
5. **Measured.** A headless-timed traverse (scripted thrust across
   ≥ 500 Mpc, ≥ 10 rebases) reports the max frame time attributable
   to rebase; the number is recorded in the DoD.

## Non-goals

- No change to `REBASE_DISTANCE_MPC`, the camera precision model
  (ADR-013), or when a rebase is *decided*.
- No async runtime (`tokio`, `rayon`, …); `std::thread` + `mpsc` per
  ADR-026 §3.
- No change to what the buffers contain — `cosmic-gpu-tracers` later
  removes the splat part of the job (todo there); this feature keeps
  the job generic so that removal is a deletion.
- No engine change.

## Users / Stakeholders

- Player: continuous travel.
- Developer: rebase cost visible in the `cosmic_layout=` / Console
  line (`rebase: queued gen N`, `rebase: swapped gen N in M frames`).
- UX consulted: yes — acceptance is "no perceptible hitch while
  cruising at max speed through a hub-to-hub run".

## Roles

Author: PO. UX consulted (required if player-facing): yes — UX-1 below
is the felt criterion. ARCHITECT consulted (required if cross-module):
yes — first worker thread in `crates/debug`; boundary rule recorded in
`architecture.md` (worker never holds Vulkan objects; main thread owns
every upload).

## Functional requirements

- FR1 (split): `refresh_cosmic()` is split into `refresh_cosmic_seed()`
  (volume + HDR chain + inspector buffers + demo buffers; reseed /
  load / swapchain paths) and `request_rebase(origin)` (demo glow +
  splats only, via the worker).
- FR2 (job): `CosmicRebaseJob { origin: DVec3, generation: u64 }` →
  `CosmicRebaseResult { generation, glow: Vec<MapVertex>, splats:
  Vec<SplatVertex> }`; worker built on the `TileLoader` shape
  (`try_spawn`, `mpsc`, `poll()` non-blocking, `Drop` joins). Latest
  request wins: the worker drains its inbox before computing.
- FR3 (swap): on `poll()` returning the current generation, the frame
  thread uploads and swaps both buffers **in the same frame**
  (never one without the other); stale generations are dropped.
- FR4 (origin semantics): `rebased()` (moving `upload_origin` and the
  camera render origin) is applied **at swap time**, not at request
  time, so the frames in between draw the old buffers with the old
  origin — no snap.
- FR5 (fence): no `then_signal_fence_and_flush().wait(None)` is
  reachable from `tick_cosmic` / `draw_main` outside the staged load
  plan and swapchain recreate.
- FR6 (telemetry): Console events `rebase: queued gen N` / `rebase:
  swapped gen N after F frames`; headless `--timed-traverse` (or the
  existing headless step loop with a scripted thrust) prints max
  frame ms and rebase count.
- FR7 (bit-identity): a test builds one rebase via the worker and via
  the synchronous functions and asserts identical vertex bytes.

## Non-functional requirements

- NFR1 (frame budget): no frame > 33 ms attributable to rebase on the
  reference Intel UHD 620 at High; upload + swap of the two buffers
  (≈ 17 MB + ≈ 1 MB) is the only main-thread cost — if it exceeds
  8 ms, split the upload over two frames (recorded).
- NFR2 (memory): at most one in-flight result held (≈ 20 MB) plus the
  live buffers; no queue growth (latest-wins).
- NFR3 (shutdown): worker joins on `Drop`; no detached thread survives
  the window.
- NFR4 (invariants): projection, picking, marker, bloom write-once
  untouched; camera `recenter(eye, origin)` per frame unchanged.

## Definition of Done

1. Headless timed traverse (≥ 500 Mpc, ≥ 10 rebases): max frame ms
   and rebase count printed; max ≤ 33 ms on the reference iGPU;
   numbers recorded in `plan.md`.
2. Rebase path rebuilds only demo glow + splats (code inspection +
   test: veil image / HDR chain `Arc` pointers unchanged across a
   rebase; inspector buffers unchanged).
3. No fence wait reachable from the frame loop (grep pin on the two
   call sites + test that `tick_cosmic` never blocks > 2 ms with a
   stubbed worker).
4. Bit-identity test (FR7) green; latest-wins test (two requests,
   one result) green; `Drop` joins (test).
5. UX-1 hands-on: cruising at max speed hub-to-hub on the nominal
   seed shows no perceptible hitch (recorded in the plan with the
   Console line evidence).
6. Docs: `architecture.md` worker rule, `rendering.md` rebase
   paragraph, `quality.md` rebase budget row, techstack bump; links
   resolve.
7. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Second feature of the version; independent of `cosmic-sphere-clip`
  (engine-free) but shot after it.
- Assumes `Buffer::from_iter` of ≈ 17 MB stays well under 8 ms on the
  reference machine (measured at boot today inside the staged loader).
- `cosmic-gpu-tracers` removes the splat part of the job later; keep
  glow and splat build as two independent closures inside the job.

## Open questions

- Should the worker also serve the reseed path (progress bar today is
  synchronous but staged)? Out of scope here; recorded as a follow-up
  in `Risks & Next steps`.
