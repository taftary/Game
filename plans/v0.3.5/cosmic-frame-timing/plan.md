# Plan — cosmic-frame-timing

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` (lib + bin) + workspace `Cargo.toml`
only. New lib module `frame_timing.rs` is GPU-free (slot enum, tick→ms
math, ring buffers, widget-row formatting — unit-tested, headless
safe). New bin state `FrameTimer` owns one `QueryPool` (`Timestamp`);
`unsafe` is limited to `reset_query_pool` + `write_timestamp` with the
reset-before-write invariant at the call site. CAP-001 seam functions
(`record_cosmic_hdr_prepass`, `record_veil_march`,
`record_cosmic_view_arm`, all called from the windowed loop AND the
offscreen `--capture` path) gain an optional timer param — the timer
writes land *between* render passes, never inside one, so the
write-once bloom description and all pipeline layouts are untouched.
**Invariant rows:** bloom write-once holds (timestamps don't write
images); capture byte-identity at High holds (timestamps don't alter
draws); `--headless` never loads the Vulkan loader (pool lives behind
the windowed/capture GPU paths only); no `engine` / `game` / `tools`
diff.

### Phase 1 — Lib math + build hygiene (CFT-001, CFT-006)

`frame_timing.rs` + dev-profile opt-levels. Risk-first: the pool
geometry (slot count × frames-in-flight) is fixed here and every later
todo depends on it.

### Phase 2 — Windowed wiring (CFT-002, CFT-003, CFT-005)

Pool create (boot, guarded by `timestamp_compute_and_graphics` +
period), per-frame reset/write/read in `draw_main`, CPU `Instant`
spans. Readback gated on widget-visible-or-capture (FR5).

### Phase 3 — Surfaces (CFT-004, CFT-005 capture line)

FPS-tab rows + `cosmic_timing=` capture log line (offscreen path
reads its own pool after its fence wait).

### Phase 4 — Baseline + docs + gates (CFT-007, CFT-008)

UHD 620 numbers, `quality.md` / `rendering.md` / `architecture.md`
notes, techstack bump, full gates, audit, review.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CFT-001 | done | `crates/debug/src/frame_timing.rs`: `GpuSlot` (Prepass/Bloom/March/Main) + `CpuPhase` (UiBuild/CosmicFrame/Record) enums, `FrameTiming` rings (avg+max, `n/a` state), `ms_from_ticks(ticks, period_ns)`; unit tests (tick math, ring cap, `n/a` before first sample) | FR1, FR2, NFR2 |
| CFT-002 | done | Bin pool + windowed writes: `QueryPool::new(Timestamp, 2 frame slots × 5 fenceposts)` at boot (guarded, `None` when unsupported); `reset_query_pool` + 5 `write_timestamp(TopOfPipe start/BottomOfPipe ends)` per frame in `draw_main` (prepass in/out via the CAP-001 seam param, march in/out, main in/out); previous-frame `get_results` (no wait) feeds `FrameTiming` | FR1, FR2, FR5, NFR4 |
| CFT-003 | done | CPU spans: `Instant` around UI build, `cosmic_frame`, command recording in `draw_main`; feed the same `FrameTiming` | Goals §2 |
| CFT-004 | done | `build_fps_tab`: "GPU passes" + "CPU phases" rows (avg + max, 20/34 ms colours, `n/a` when unsupported) | FR3 |
| CFT-005 | done | `--capture` logs `cosmic_timing=` (GPU ms from its own waited pool read + CPU phases); readback gate: `get_results` only when widget visible or capturing | FR4, FR5 |
| CFT-006 | done | `[profile.dev.package.game_debug]` + `[profile.dev.package.game_engine]` `opt-level = 1`; `quality.md` note (budgets measured with `--release`) | FR6 |
| CFT-007 | done | Baseline table (UHD 620, seed 1337, 1280×768... dev profile; release blocked by known `issue-2026-09-19-1958-release-fill-pipeline-panic`) + `K=1/2` prediction runs for F2 | Goals §6 |
| CFT-008 | done | Docs (`rendering.md` frame-timing §, `architecture.md` module row, techstack bump) + full gates + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — bin+lib only, no
engine/game/tools diff; seam-param approach keeps one recording
function for both targets; invariants listed above) · Todos approved
by: TECHLEAD (2026-09-23 — lib math + pool geometry first (CFT-001),
wiring before surfaces, baseline before docs; budgets checked — pool
is one 10-query object, writes ~µs, readback gated; gate commands per
`quality.md`) · UX acceptance rows: n-a (dev-only overlay) · DoD
verified by: ANALYST (2026-09-23 — per-row audit below) · Security reviewed by: SECURITY
(2026-09-23 — pass, no findings; review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Widget GPU ms per pass | done | Windowed rows fed by `timer_feed` into `FrameTiming` rings shown in `build_fps_tab`; the offscreen path shares the identical stamp writes (same seam) and printed `prepass:125.79 bloom:5.79 march:2.56 main:0.85` on the inspector run (see DoD 6). Widget-row rendering reviewed (no interactive windowed session driven in this automation — limitation L-1; rows use the existing `text_row` path with the same rings the capture feeds). | DEV 2026-09-23; ANALYST 2026-09-23 (pass with L-1) |
| 2 | Widget CPU phase ms | done | `CpuPhase::{UiBuild,CosmicFrame,Record}` `Instant` spans pushed every `draw_main` unconditionally; same rings/rows as DoD 1. Capture line reports `cpu_record:1.05–5.13` (dev). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 3 | `--capture` `cosmic_timing=` line | done | Inspector: `cosmic_timing=prepass:125.79 bloom:5.79 march:2.56 main:0.85 total:134.99 cpu_record:5.13 (ms)`; demo, slab, k1/k2 runs below. Tests: lib `frame_timing` 8/8 green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass — reproduced 6 captures) |
| 4 | Zero readback when widget hidden | done | `get_results` is inside `if self.debug.widget_visible` (windowed) — code-gated; writes always run (~µs). Offscreen path reads after its own fence only. | DEV 2026-09-23 (code review); ANALYST 2026-09-23 (pass) |
| 5 | Dev opt-levels + `--release` note | done | `Cargo.toml` `[profile.dev.package.game_debug]` + `[profile.dev.package.game_engine]` `opt-level = 1`; `quality.md` budgets-measured-with-`--release` note. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 6 | Baseline table recorded | done (dev; release blocked — see R-3) | UHD 620 (device `IntegratedGpu`, period 83.33 ns), seed 1337, 1408×768, High, dev profile. Inspector k8: prepass 125.79 / bloom 5.79 / march 2.56 / main 0.85 (run 2: 139.63 / 1.89 / 1.93 / 0.67 — iGPU clock variance). Demo k8: 190.16 / 1.94 / 3.15 / 0.67. Slab k8: 202.35 / 2.02 / 3.27 / 0.65. Inspector k2: 30.24 / 5.66 / 2.58 / 1.93. Inspector k1: 58.27 / 2.49 / 2.56 / 0.87 then 18.58 / 8.82 / 2.56 / 0.86 (variance — rolling avg/max exists for this). Verdict: prepass owns ~95%; k scales it ~4–7×. Release column blocked by pre-existing `issue-2026-09-19-1958-release-fill-pipeline-panic` (draft, v0.3.2 — release binary panics in `build_fill_pipeline` on this driver before any frame; reproduced 2026-09-23 on this tree, untouched by this feature). | DEV 2026-09-23; ANALYST 2026-09-23 (pass with R-3 follow-up) |
| 7 | Gates + audit + review + one commit | done | `fmt --check`, `clippy --workspace --all-targets --all-features -D warnings`, `build --workspace`, `test --workspace --all-targets`, `test --doc`, `game` run, `game_debug --headless`, `game_tools --headless --tier low`, mobile guards — all green (see gate log below). Captures byte-identical ×2 (`A5AA81…`). ANALYST + SECURITY signed below; single `done` commit on branch `v0.3.5`. | DEV; ANALYST 2026-09-23; SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Bloom write-once description unchanged (`assert_write_once`
  green) [T: existing pin tests].
- A-2. Capture byte-identity at High unchanged (timestamps add no
  draws, no state) [T: capture gate ×2].
- A-3. `--headless` loads no Vulkan symbols [T: existing headless run].
- A-4. `unsafe` limited to query reset/write with documented
  reset-before-write [R: SECURITY review].

## Risks & Next steps

- R-1 (no timestamp support on some device): guarded `None` path,
  widget `n/a`, zero behavior change. OUTCOME 2026-09-23: guard
  unit-tested (`unsupported_ignores_gpu_but_keeps_cpu`); live-proven
  the other way (UHD 620 exposes timestamps, period 83.33 ns).
- R-2 (pool sizing vs frames-in-flight): app chains `previous_frame_end`
  (≤ 1 frame in flight + acquire); 2 frame slots give margin.
  OUTCOME 2026-09-23: 6 consecutive captures + full test suite green,
  no validation error; windowed readback runs one frame late by design.
- R-3 (release baseline blocked): `--release` binary panics in
  `build_fill_pipeline` on UHD 620 before any frame — pre-existing
  `plans/v0.3.2/cosmic-scale-player/issue-2026-09-19-1958-release-fill-pipeline-panic`
  (draft), reproduced on this tree 2026-09-23, untouched by this
  feature (no shader/vertex/pipeline change in the diff). DoD 6 ships
  dev-profile numbers; the release column is that issue's DoD, not
  this feature's. PO acknowledged 2026-09-23.
- L-1 (no interactive windowed session in this automation): the FPS
  widget rows are review-verified, not screenshot-verified. The
  operator's `F6` check on their box closes this (rows read the same
  rings the capture path demonstrably feeds).
- Next: F2 `cosmic-device-tier` consumes the baseline (CFT-007);
  culling/bricks/fill follow-ups consume the per-pass split.

## ANALYST audit note (2026-09-23)

- Re-ran: `frame_timing` lib tests 8/8 (in the 294-test lib suite),
  bin suite 45/45, 6 `--capture` runs (inspector ×2 byte-identical
  `A5AA81…`, demo, slab, k1 ×2, k2 ×1) all printing `cosmic_timing=`.
- DoD 1: code-reviewed (L-1 recorded); DoD 4: code-gated, no test —
  accepted (a GPU test is not unit-representable; the gate is three
  lines).
- E2E: n-a (debug-only tooling, no player journey, no save format).
- Verdict: all rows pass with L-1 + R-3 noted. No findings filed.

## SECURITY review note (2026-09-23)

- New input surfaces: none (no CLI flag, no text field, no console
  command; `cosmic_timing=` is stdout-only).
- New dependencies: none. New `unsafe`: 2 sites (`reset_query_pool`,
  `write_timestamp`), both with reset-before-write + same-command-buffer
  invariants documented at the call site; pool sized 10, indices
  `base + 0..5` provably in range (`base` ∈ {0, 5}).
- Save/migration: untouched. Verdict: pass, no findings.
