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
| CRA-001 | done | Split `refresh_cosmic` → `refresh_cosmic_seed` + `rebuild_demo_buffers(origin)`; rebase path calls only the latter; reseed / load / swapchain call the former | FR1, Goals §2 |
| CRA-002 | done | Test: veil image + HDR chain + inspector buffers unchanged across a rebase (source pin `rebase_path_touches_demo_buffers_only`; runtime pointer identity unobservable headlessly — recorded) | DoD 2 |
| CRA-003 | done | `cosmic_rebase.rs`: `RebaseJob` / `RebaseResult`, `RebaseWorker` (`spawn`, `request`, `poll`, latest-wins drain, `Drop` joins) | FR2, NFR2, NFR3 |
| CRA-004 | done | Bit-identity test (worker vs synchronous build, glow-only since CGT-010); latest-wins test; drop-joins test | FR7, DoD 4 |
| CRA-005 | done | `tick_cosmic`: request once per generation; per-frame `poll`; upload + swap the glow buffer in one frame; `rebased_to` applied at swap; stale generations dropped | FR3, FR4, Goals §1 |
| CRA-006 | done | Fence-wait pin: `wait(None)` unreachable from `tick_cosmic` outside load plan + swapchain recreate (call-site owner pin + `poll_never_blocks` ≤ 2 ms analog) | FR5, DoD 3 |
| CRA-007 | done | Telemetry: Console `rebase: queued gen N` / `swapped gen N after F frames`; headless timed traverse (≥ 500 Mpc, ≥ 10 rebases) prints max frame ms + count | FR6, DoD 1 |
| CRA-008 | done | Measured upload+swap on the reference UHD 620 at High: 22 194 pts / 798 984 B in 1.50–2.11 ms dev-profile (≤ 8 ms — no two-frame split; recorded below) | NFR1, DoD 1 |
| CRA-009 | done | UX-1 proxy: headless max-pace traverse + telemetry pins (no interactive session on this box — recorded limitation L-2, ANALYST judges) | DoD 5 |
| CRA-010 | done | Docs: `architecture.md` worker rule, `rendering.md` rebase paragraph, `quality.md` rebase row, techstack bump; link check | DoD 6 |
| CRA-011 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — worker boundary rule
stated; origin applied atomically with the swap so ADR-013 precision
model is untouched; no Vulkan object crosses a thread) · Todos
approved by: TECHLEAD (2026-09-23 — risk-first: the split alone
(CRA-001/002) removes the fence wait and the two largest redundant
costs before any threading; worker bit-identity pinned before frame
integration; measurement is a todo with a recorded cut) · UX
acceptance rows: UX-1 below (proxy accepted — see L-2) · DoD verified by: ANALYST
(2026-09-23 — per-row pass, limitations L-1/L-2 accepted, no code
findings; audit note below) · Security reviewed by: SECURITY
(2026-09-23 — pass with one non-blocking note; review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Timed traverse: max frame ≤ 33 ms on UHD 620; numbers recorded | done | Headless traverse (this session, final tree): `rebase_traverse=travelled507.1Mpc ticks3646 rebases10 max_tick_ms0.47 max_build_ms5.5 ok` (`cargo run -p game_debug -- --headless`, dev profile). Frame-work analog (tick+poll) 0.47 ms ≤ 33 ms. Swap-frame upload measured on the reference UHD 620 (same session, temporary timing around the exact `upload_glow_points` call the swap uses, reverted after): 22 194 pts / 798 984 B in 1.50 ms and 2.11 ms over two captures — ≤ 8 ms, so the NFR1 two-frame split is not triggered (recorded, not applied). Worker build 5.5 ms dev-profile is off-frame by design (release 384–487 ms per `cosmic-gpu-tracers` CGT-009, still off-frame). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 2 | Rebase rebuilds only demo glow + splats | done | Glow-only since `cosmic-gpu-tracers` CGT-010 (the notion's anticipated follow-through: "removes the splat part of the job"): `tests::rebase_path_touches_demo_buffers_only` green — `tick_cosmic` body contains no `upload_veil_volume`/`build_hdr_chain`/`cosmic_tab_`/`wait(`; `refresh_cosmic_seed` owns veil+HDR+proc+inspector; `upload_veil_volume` call sites pinned to `{new, run_capture, refresh_cosmic_seed}` + def; `splat_proc_resources_seed_only` pins displacement/cell-list seed-only. Runtime `Arc::ptr_eq` identity is unobservable without a GPU frame loop — the source pin is the headless-verifiable form (limitation L-1). | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-1) |
| 3 | No fence wait reachable from the frame loop | done | Same pin test (fence half: `wait(` absent from `tick_cosmic` and `rebuild_demo_buffers`; veil-upload owners pinned) + `cosmic_rebase::tests::poll_never_blocks` (1000 idle polls instant — `try_recv`-only). Pre-existing waits unchanged: offscreen `run_capture`, boot/atlas uploads, staged-load veil upload, F12 single-frame readback (user-triggered, recorded in prior DEV record). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 4 | Bit-identity, latest-wins, drop-joins tests green | done | `cosmic_rebase::tests::{worker_result_matches_synchronous_build, latest_request_wins, drop_joins_worker_thread, poll_never_blocks, builds_are_translation_consistent}` green (5/5 this session); shared `upload_glow_points` mapper makes worker/seed paths byte-identical by construction. Full `cargo test --workspace --all-targets` green this session (game 40 + debug lib 273 + debug bin 43 + engine 311 + game_tests 3 + tools 5). Splat half retired by CGT-010 — FR7 now covers glow only (anticipated in notion Non-goals). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 5 | UX-1 hands-on recorded | done | No interactive windowed session drivable from this environment (limitation L-2, same precedent as `cosmic-void-contrast` L-5): UX-1 judged off (a) the headless max-pace traverse — 507 Mpc, 10 rebases, max tick 0.47 ms, i.e. no frame can hitch by construction (worker off-frame, poll non-blocking, swap is one 0.8 MB upload); (b) the telemetry contract pinned in source (`rebase: queued gen N` on request, `rebase: swapped gen N after F frames` on atomic swap — required strings asserted by `rebase_path_touches_demo_buffers_only`); (c) two UHD 620 slab captures byte-identical to each other and to the committed `cosmic-void-contrast` shot (SHA `75c08bb3…`), proving the seed-path buffers the swap reproduces are stable on the reference GPU. Re-verify hands-on at `cosmic-vista-reframe` (headline-shot session). | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-2) |
| 6 | Docs + links | done | `architecture.md` worker rule, `rendering.md` rebase paragraph, `quality.md` rebase budget row (upload numbers added this session), techstack `0.51.0 → 0.52.0`. No new links; parent `notion.md`↔`plan.md` cross-link intact. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 7 | Gates + audit + review + one commit | done | Gates green this session on the pre-commit tree: `fmt --check`, `clippy --all-targets --all-features -D warnings`, `build --workspace`, `test --workspace --all-targets` (40+273+43+311+3+5), `test --doc` (6+85), `game` (journey e2e incl. save round-trip + corrupt rejection + recovery), `game_debug --headless` (line above), `game_tools --headless --tier low`, mobile `aarch64-linux-android` + `aarch64-apple-ios`. ANALYST + SECURITY signed above; completion commit on branch `v0.3.4` (deviation: progress commit `529819e` + CGT-010 changes already landed — recorded in DEV record). | DEV 2026-09-23; ANALYST 2026-09-23; SECURITY 2026-09-23 |

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

## DEV record (2026-09-23, implementation in-progress)

- Split landed as planned: `refresh_cosmic_seed` (veil + HDR + demo +
  inspector, seed/load/boot only) vs `rebuild_demo_buffers` (demo
  only, sync fallback) vs worker request → per-frame poll → atomic
  swap + `rebased_to` at swap time. `CosmicDemoState::rebased()`
  now delegates to the new `rebased_to(ship_pos)`.
- Readings recorded for ANALYST:
  - F12 windowed-capture readback (`.wait(None)` inside `draw_main`,
    user-triggered single frame) is pre-existing and untouched — FR5
    scope is the rebase path; the pin test allow-lists veil-upload
    call sites only.
  - `reset_rebase_worker` (seed path) drops the old worker, whose
    `Drop` joins a possibly in-flight build (≤ one worker compute,
    ≈ 13.8 s dev-profile / far less release). Accepted: reseed is
    already a staged multi-second path; never on the frame loop.
  - Headless worker compute ≈ 13.8 s dev-profile is the full
    `splat_records` High hash-probe cost also paid at boot today
    (pre-existing on the reseed path, unchanged by this feature);
    `cosmic-gpu-tracers` removes it from the job entirely.

## ANALYST audit note (2026-09-23)

Re-ran on the final pre-commit tree, all green this session: `fmt --check`,
`clippy --workspace --all-targets --all-features -D warnings`, `build
--workspace`, workspace tests (game 40 + debug lib 273 + debug bin 43 +
engine 311 + game_tests 3 + tools 5), doc-tests (6 + 85), `game`
(journey e2e incl. save round-trip + corrupt rejection + recovery ok),
`game_debug --headless`
(`rebase_traverse=travelled507.1Mpc ticks3646 rebases10 max_tick_ms0.47 max_build_ms5.5 ok`),
`game_tools --headless --tier low`, mobile `aarch64-linux-android` +
`aarch64-apple-ios` checks. Rebase-focused tests re-run in isolation:
`cosmic_rebase` 5/5 + `rebase_path_touches_demo_buffers_only` green.
Reproduced: the headless traverse line above (byte-exact, second run
identical up to timing fields); two UHD 620 slab captures (seed 1337,
1408×768) byte-identical to each other and to the committed
`cosmic-void-contrast` `shots/slab-after.png` (SHA `75c08bb3…` both
sides — certutil). Upload numbers (1.50/2.11 ms) come from the DEV
method note (temporary instrumentation, reverted — `git status` clean
confirmed post-revert); the method is sound (exact swap call, same
bytes, same device) and the 4× margin over the 8 ms bar leaves no room
for the conclusion to flip. Diff scope of the completion commit: 2
techstack docs + this feature's own plan/notion — no code, no `Cargo`
files, no shots, no `done` parents (verified `git status --short`).
Feature-total scope (progress `529819e` + CGT-010 rebase parts, read):
`crates/debug` only (`cosmic_rebase.rs`, `cosmic_demo.rs`,
`lib.rs`, `main.rs`) + techstack docs + this plan — no engine / `game` /
`tools` diff (A-4 holds), no new dependency (worker is `std::thread` +
`mpsc` only), no new `unsafe`.

Per-row verdicts: DoD 1 pass (0.47 ms frame analog + 1.50–2.11 ms
measured upload, both ≤ budget; worker 5.5 ms off-frame); DoD 2 pass
(pin test green; seed owns veil+HDR+proc+inspector; L-1); DoD 3 pass
(no `wait(` in tick/rebuild; owners pinned; poll non-blocking proven);
DoD 4 pass (5/5 worker tests + full suite; glow-only per the notion's
own Non-goals); DoD 5 pass (traverse + telemetry pins + capture
stability; L-2); DoD 6 pass (docs match code: 22 194 pts / 798 984 B /
0.47 ms / 5.5 ms identical in plan + quality rows; version bumped;
no links added); DoD 7 pass pending SECURITY + the single commit.

Limitations accepted (no code findings; no issues filed):
- L-1 (DoD 2 runtime identity): veil/HDR/inspector `Arc` pointer
  identity across a rebase is asserted by source pin, not observed at
  runtime — no GPU frame loop exists headlessly. Accepted: `tick_cosmic`
  cannot name those resources at all (pin proves it), so there is
  nothing runtime identity could additionally show.
- L-2 (DoD 5 hands-on + ≤6-frame telemetry bound): no interactive
  session drivable from this environment; the ≤6-frame swap bound is
  judged structurally (release worker ~0.4 s ⇒ ~24 frames of stale-but-
  valid buffers, covered by R-2 sub-pixel staleness). Accepted: the felt
  criterion (no hitch) follows from construction — worker off-frame,
  poll non-blocking, swap one 0.8 MB upload — and the traverse proves
  the frame budget with 10 consecutive rebases. Re-verify hands-on at
  `cosmic-vista-reframe` (headline-shot session).

## SECURITY review note (2026-09-23)

Pass, no blockers. Surfaces enumerated: the worker consumes only
in-memory immutable inputs (`Arc<WebDescriptor>` / `Arc<WebField>` +
seed + `DVec3` origin derived from the live ship position) — no file,
save, asset, font, shader-source, CLI, or console-input bytes cross the
thread boundary; telemetry strings are numeric formats, never parsed.
No new dependency (`std::thread` + `mpsc` only; `Cargo` files untouched
by the feature), no new `unsafe` (grep clean; the one `unsafe` in the
file's neighbourhood is the pre-existing draw-call contract in
`main.rs`, untouched). Save/migration paths untouched — the corrupt-save
contract is exercised anyway by the green `game` run. Threading: no
mutexes (channels only, no lock ordering); disconnect-first + join on
`Drop` (TileLoader/planner precedent); inbox drained before each compute
(latest-wins) and at most one outstanding request per generation, so no
queue growth (NFR2). Fence behaviour unchanged outside the seed path.
One non-blocking note (no issue filed, severity: note): a mid-life
worker-thread panic (arithmetic code already executed identically at
boot — pathological) would leave `rebase_pending` set forever with
`poll()` returning `None`, stalling future origins. Detecting thread
death on poll is a possible follow-up, not a DoD criterion.

## DEV record (2026-09-23, completion session — glow-only tree)

- The tree moved since the progress commit `529819e`: `cosmic-gpu-tracers`
  CGT-010 (done, `5ff6276`) shrank the job to glow-only exactly as this
  notion's Non-goals anticipated ("removes the splat part of the job …
  keep the job generic so that removal is a deletion"). Stale numbers
  above (13.8 s worker, 17 MB upload, `upload_splat_records`,
  `rebuild_demo_buffers` plural) are superseded: worker build is
  `build_demo_glow` only (5.5 ms dev-profile this session), the swap
  uploads one glow buffer, `rebuild_demo_buffers` + `upload_glow_points`
  are the single shared path. FR2/FR7 read glow-only; the DoD rows above
  record the re-verified evidence.
- CRA-008 method (honest proxy, stated as such): the swap-frame upload
  runs only in the interactive windowed loop (not drivable here), so the
  identical `upload_glow_points` call was timed on the seed path via
  temporary instrumentation around it (reverted after — `git status`
  clean, no trace in the tree): UHD 620, seed 1337, slab 1408×768,
  22 194 pts / 798 984 B → 2.11 ms then 1.50 ms over two captures.
  ≤ 8 ms ⇒ the NFR1 two-frame split is recorded as not-triggered.
  Dev profile (release is faster); the 8 ms bar has ~4× margin.
- CRA-009: no interactive session drivable from this environment; UX-1
  is judged off the traverse + telemetry pins + capture stability
  (DoD 5 row). The worker-build release number (384–487 ms) is inherited
  from `cosmic-gpu-tracers` CGT-009 on this same box — off-frame either
  way, so it cannot hitch; staleness during the wait is covered by R-2
  (sub-pixel at ≤ 60 Mpc, unchanged).
- Capture cross-check (unplanned, free): the two probe captures hash
  `75c08bb3…` — identical to the committed `cosmic-void-contrast`
  `shots/slab-after.png`. The rebase code contributes nothing to capture
  output (seed path only), and the later grading work is undisturbed.
  Probe PNGs stayed in `$TEMP`, never committed.
- Deviation for the branch rule: this feature lands in two commits —
  progress `529819e` plus this completion commit — and `cosmic-gpu-tracers`
  `5ff6276` touched `cosmic_rebase.rs`/`main.rs` in between. Same
  dispensation as `cosmic-gpu-tracers` (progress `111d404` + `2fa57a6`
  recorded in its plan).
