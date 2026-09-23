# Plan — cosmic-rebase-async

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — new `cosmic_rebase.rs`
(worker, job/result types, pure functions), `main.rs` (split of
`refresh_cosmic`, poll + swap in `tick_cosmic`, telemetry),
`cosmic_demo.rs` (`rebased()` applied at swap). No engine change.
**Boundary rule (recorded in `architecture.md`):** the worker owns
`Arc<WebDescriptor>` / `Arc<WebField>` and produces plain `Vec`s;
it never touches `vulkano` objects; the main thread owns every
upload and swap. **Invariant rows:** camera recenter per frame
unchanged; origin applied atomically with the buffer swap; bloom /
march targets untouched; no fence wait from the frame loop.

### Phase 1 — Split the rebuild (`main.rs`)

`refresh_cosmic_seed()` (volume + HDR + inspector + demo) vs
`rebuild_demo_buffers(origin)` (glow + splats, still synchronous).
Rebase calls only the latter. Pointer-identity test on veil image /
HDR chain across a rebase.

### Phase 2 — Worker (`cosmic_rebase.rs`)

`RebaseWorker` on the `TileLoader` shape: `spawn(Arc<desc>,
Arc<field>, tier, mode)`, `request(origin, gen)`, `poll() ->
Option<Result>`, latest-wins drain, `Drop` joins. Pure build
functions shared with the synchronous path (bit-identity test).

### Phase 3 — Frame integration (`main.rs`, `cosmic_demo.rs`)

`tick_cosmic`: on `needs_rebase()` → `request` (once per generation);
each frame `poll` → upload both → swap both → `rebased()` → Console
event. Stale-generation drop test. Headless timed traverse.

### Phase 4 — Measurement + docs + gates

UHD 620 traverse numbers; UX-1 session; `architecture.md`,
`rendering.md`, `quality.md`; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CRA-001 | pending | Split `refresh_cosmic` → `refresh_cosmic_seed` + `rebuild_demo_buffers(origin)`; rebase path calls only the latter; reseed / load / swapchain call the former | FR1, Goals §2 |
| CRA-002 | pending | Test: veil image + HDR chain `Arc::ptr_eq` and inspector buffers unchanged across a rebase | DoD 2 |
| CRA-003 | pending | `cosmic_rebase.rs`: `RebaseJob` / `RebaseResult`, `RebaseWorker` (`spawn`, `request`, `poll`, latest-wins drain, `Drop` joins) | FR2, NFR2, NFR3 |
| CRA-004 | pending | Bit-identity test (worker vs synchronous build); latest-wins test; drop-joins test | FR7, DoD 4 |
| CRA-005 | pending | `tick_cosmic`: request once per generation; per-frame `poll`; upload + swap both buffers in one frame; `rebased()` applied at swap; stale generations dropped | FR3, FR4, Goals §1 |
| CRA-006 | pending | Fence-wait pin: `wait(None)` unreachable from `tick_cosmic` / `draw_main` outside load plan + swapchain recreate (grep pin + stubbed-worker timing test ≤ 2 ms) | FR5, DoD 3 |
| CRA-007 | pending | Telemetry: Console `rebase: queued gen N` / `swapped gen N after F frames`; headless timed traverse (≥ 500 Mpc, ≥ 10 rebases) prints max frame ms + count | FR6, DoD 1 |
| CRA-008 | pending | Measure on the reference UHD 620 at High; if upload+swap > 8 ms, split over two frames (recorded); record numbers here | NFR1, DoD 1 |
| CRA-009 | pending | UX-1 hands-on: max-speed hub-to-hub run, nominal seed; Console evidence recorded | DoD 5 |
| CRA-010 | pending | Docs: `architecture.md` worker rule, `rendering.md` rebase paragraph, `quality.md` rebase row, techstack bump; link check | DoD 6 |
| CRA-011 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — worker boundary rule
stated; origin applied atomically with the swap so ADR-013 precision
model is untouched; no Vulkan object crosses a thread) · Todos
approved by: TECHLEAD (2026-09-23 — risk-first: the split alone
(CRA-001/002) removes the fence wait and the two largest redundant
costs before any threading; worker bit-identity pinned before frame
integration; measurement is a todo with a recorded cut) · UX
acceptance rows: UX-1 below · DoD verified by: ANALYST _(pending)_ ·
Security reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Timed traverse: max frame ≤ 33 ms on UHD 620; numbers recorded | pending | | |
| 2 | Rebase rebuilds only demo glow + splats | pending | | |
| 3 | No fence wait reachable from the frame loop | pending | | |
| 4 | Bit-identity, latest-wins, drop-joins tests green | pending | | |
| 5 | UX-1 hands-on recorded | pending | | |
| 6 | Docs + links | pending | | |
| 7 | Gates + audit + review + one commit | pending | | |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Worker never holds a `vulkano` object; every upload on the main
  thread [T: type-level — job/result carry only `Vec` + `DVec3` +
  `u64`].
- A-2. `upload_origin` and both buffers change in the same frame [T:
  CRA-005 test].
- A-3. Camera `recenter(eye, origin)` per frame unchanged; marker /
  picking paths untouched [T: existing tests green].
- A-4. No engine / `game` / `tools` diff.

UX acceptance rows:

- UX-1. Cruising at max speed from the home node to the first Tier-A
  hub and back on the nominal seed: no perceptible hitch; Console
  shows every rebase queued and swapped within ≤ 6 frames.

## Risks & Next steps

- R-1 (upload cost on the main thread): 17 MB `Buffer::from_iter`
  may exceed the per-frame budget on the iGPU; cut recorded in
  CRA-008 (two-frame split). Disappears when `cosmic-gpu-tracers`
  retires the splat buffer.
- R-2 (stale visual during the wait): the ship may travel a few Mpc
  past the rebase distance before the swap; f32 error at ≤ 60 Mpc
  from origin ≈ 4 kpc — sub-pixel at every demo framing (ADR-013
  numbers).
- Next: `cosmic-gpu-tracers` deletes the splat closure from the job
  (todo there); reseed on the worker with a real progress feed is a
  follow-up (not scheduled).
