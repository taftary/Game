# Plan — cosmic-vista-reframe

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_demo.rs`
(`interior_window`, `vista_hub`, `vista_pose`), `cosmic_capture.rs`
(`VISTA` / `SLAB` poses), `cosmic_camera.rs` untouched (contract from
v0.3.3 suffices). Reads `WebDescriptor` only. **Invariant rows:**
un-flipped projection at 25°; marker via `world_to_pixels`; picking
gated during the vista; pose a pure function of `(seed)`; no new GPU
resource.

### Phase 1 — Composition math (`cosmic_demo.rs`)

`interior_window` + `vista_hub` + focal offset; tests (fit, `None`,
fallback, determinism, Tier A, footprint).

### Phase 2 — Pose + dive (`cosmic_demo.rs`)

`vista_pose` from Phase 1; re-dock the dive; run the existing bound /
continuity / headless tests; retune `VISTA_DOCK_MPC` if needed.

### Phase 3 — Presets + shots + readings

`VISTA` / `SLAB` presets; `vista-after.png`, `slab-after.png`, timed
hint / dive shots; corner scans; six-reading table; UX session.

### Phase 4 — Docs + gates + version close

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CVR-001 | done | `interior_window(R, c, W, H, T, margin) -> Option<(d, fov)>` + 16:9 fallback; tests (nominal fit, `None`, fallback) | FR1, Goals §1 |
| CVR-002 | done | `vista_hub(&web)` (`rank_score − λ·|c|/R`, Tier A, footprint fit, fallback chain); tests: determinism, Tier A, `|c| ≤ 0.4·R` | FR2, Goals §2 |
| CVR-003 | done | `vista_pose` from FR1/FR2 with focal offset `(0.55, 0.5)`; projection test on the nominal seed | FR3, DoD 2 |
| CVR-004 | done | Dive re-dock from the new eye; existing bound / continuity / headless `Done == Chase` tests green; `VISTA_DOCK_MPC` retune recorded if needed | FR4, Goals §3, DoD 3 |
| CVR-005 | done | `CapturePreset::VISTA` / `::SLAB` take the FR3 pose; capture determinism pin | FR5 |
| CVR-006 | done | Shots `vista-after.png`, `slab-after.png`, `vista-after-hint.png`, `vista-after-dive.png`; corner limb scans recorded | DoD 1 |
| CVR-007 | done | Six-reading table (report §2) for `vista-after.png` in a `docs/reports/` addendum; ≥ 5 met | Goals §5, DoD 2 |
| CVR-008 | done | UX-1 / UX-2 hands-on session recorded | UX rows |
| CVR-009 | done | Docs: `rendering.md` interior-window note, `journey.md` boot sentence, techstack bump; link check | DoD 5 |
| CVR-010 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit; milestones rows `done`; merge `v0.3.4 → main` | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — pure composition
functions on the existing camera contract; no new resource; every
invariant row inherited from `cosmic-vista-intro`) · Todos approved
by: TECHLEAD (2026-09-23 — math + tests before pose; dive bounds
re-run before any shot; the reading table is a todo with a numeric
threshold; version close is the last row) · UX acceptance rows: UX-1,
UX-2 below (judged 2026-09-23 off the reframed captures + pins,
limitation L-1) · DoD verified by: ANALYST (2026-09-23 — per-row audit
below) · Security reviewed by: SECURITY (2026-09-23 — pass, no
findings; review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | No limb on `vista` / `slab` (corner scans) | done | UHD 620, seed 1337, 1408×768 High, byte-identical ×2 (`vista-after.png` SHA `450450D9…` 1.52 MB, `slab-after.png` SHA `714E9837…` 0.60 MB): radial worst-10px-step past the hub complex vista `0.181` / slab `0.152` (both ≤ 0.20). Tests `vista_reframed_shows_no_limb`, `slab_reframed_shows_no_limb` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 2 | Focal hub position; six readings ≥ 5 met | done | Nominal hub rank 1/4876 Tier A, focal NDC `(0.096, 0.000)` → frame `(0.548, 0.500)` (test `nominal_pose_frames_tier_a_with_home_in_frame`); home in frame `(0.081, 0.048)`. Readings addendum `docs/reports/2026-09-23-vista-reframe-six-readings.md`: 5 met / 1 partial (depth cue partial — thin opening slice). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 3 | Dive bounds + continuity + headless pin | done | Real dive nominal 28.4°/s ≤ 30 (hub exits s = 0.460, late), `Done == Chase` 1e-6, headless `vista=done events=hold,dive,done`; timed `vista-after-hint.png` (t = 1.5, Hold-identical) + `vista-after-dive.png` (t = 4, moved into the web, no snap). `VISTA_DOCK_MPC` kept at 120 (no retune — bound holds). Tests `real_dive_turns_slowly_on_nominal_seed`, `vista_boots_hold_and_reaches_chase` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 4 | Tests listed | done | `interior_window_fits_nominal_and_rejects_rim` (d 383.4, rim `None`) + `interior_window_shrink_keeps_16_9_and_fits` (shrinks at |c| = 200, `None` at rim) + `composition_pose_targets_a_fitting_tier_a_hub` (deterministic, seam agrees) + `slab_capture_pose_matches_vista_pose` (eye/target 1e-3, 20°, 30 Mpc at hub depth, deterministic) + `set_eye_target_round_trips_off_axis_poses` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 5 | Docs + links | done | `rendering.md` interior-window paragraph (pose + shrink + slab-from-same-pose, 28.4°/s), `journey.md` boot sentence (interior window), report addendum (six readings), techstack 0.54.0, milestones v0.3.4 all `done`. Link check below. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 6 | Gates + audit + review + one commit + merge | done | Gates green 2026-09-23 on the final tree (this session — see ANALYST note): fmt, clippy, build, workspace tests, doc-tests, `game` / `game_debug --headless` / `game_tools --headless --tier low`, mobile android + ios checks, captures byte-identical ×2. ANALYST + SECURITY signed below; single `done` commit on branch `v0.3.4`; merge `v0.3.4 → main` closes the version. | DEV; ANALYST 2026-09-23; SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `directx::perspective` at 25° (RH, Z ∈ [0,1], no Y-flip) [T].
- A-2. Marker via `world_to_pixels` only [T: existing pin].
- A-3. `select_node_at` returns `None` while the vista is active [T].
- A-4. Pose is a pure function of `(seed)` [T: replay + capture gate].
- A-5. No engine / `game` / `tools` diff; no new GPU resource.

UX acceptance rows:

- UX-1. Boot: the first frame is a web that fills the screen edge to
  edge — a place, not an object; the hint appears after 1 s.
- UX-2. The focal hub sits slightly right of centre; the dive is one
  continuous motion into the web with the hub drifting out, never
  snapping.

## Risks & Next steps

- NOTE 2026-09-23 (cross-link from `cosmic-sphere-clip` CSC-009, PO
  decision): the bounded descriptor broke the 180 Mpc outside vista
  pose (nominal dive 35.3°/s vs the 30°/s bound), so the dive-safety
  core landed early in the sphere-clip commit — `interior_window` +
  focal composition + dive-exact hub shortlist
  (`vista_pose(web, radius, chase)`, nominal pick rank 1, dive
  28.4°/s, focal NDC (0.096, 0.000)). This feature keeps CVR-005
  (presets), CVR-006 (shots), CVR-007 (readings), CVR-008 (UX),
  CVR-009 (docs), CVR-010 (gates + version close), plus the FR1 16:9
  shrink fallback and any `VISTA_DOCK_MPC` re-tune the final
  materials require; CVR-001..003 stand (implemented) and CVR-004's
  bound runs green.
- R-1 (no fitting hub): a seed may lack a Tier A hub with `|c| ≤
  0.4·R`; the fallback chain widens to `0.6·R` then to Tier B
  (recorded, tested on the seed set). OUTCOME 2026-09-23: superseded
  in code by the FR1 16:9 shrink fallback (largest fitting 16:9
  footprint per hub, dive-exact shortlist on the shrunk poses) ahead
  of the legacy outside framing — the legacy chain stays for fully
  degenerate seeds only. Nominal seeds never leave the full window.
- R-2 (dive length): a central hub can be far from home, lengthening
  the dive or raising angular velocity; `VISTA_DOCK_MPC` and the 8 s
  dive are the two knobs, bound test is the gate. OUTCOME 2026-09-23:
  no retune — `VISTA_DOCK_MPC` stays 120, nominal dive 28.4°/s.
- Next (post-v0.3.4): anisotropic splats; LOD / culling hardening
  from this version's measured costs; "seen" flag persistence.

## UX session note (2026-09-23, static-shot review — limitation L-1)

- No interactive session drivable on this box (headless + offscreen
  captures only); UX-1/UX-2 judged off the reframed captures plus the
  projection / dive / continuity pins (the void-contrast L-4/L-5 and
  compact-cores L-5 precedent).
- UX-1 (place, not object): pass — `vista-after.png` fills the frame
  edge to edge with web on all four edges, no disc, no limb (scan
  0.181); the hint timing is unchanged (`vista_hint_alpha` pin).
- UX-2 (focal hub + continuous dive): pass — hub at the focal mark by
  projection pin (NDC `(0.096, 0.000)`); `vista-after-hint.png`
  (t = 1.5) is Hold-identical to t = 0; `vista-after-dive.png`
  (t = 4) has moved into the web with the hub drifted, no snap; the
  dive bound (28.4°/s) and `Done == Chase` pins hold.

## DEV record (2026-09-23, implementation + grading)

- CVR-001/FR1: `interior_window_shrink` in `cosmic_vista.rs` (full
  returned unchanged when it fits; else largest fitting exact-16:9
  `w` from `allowed = cross − margin − t/2`, verified through the
  exact `interior_window` check; `None` at the rim); wired as the
  pre-legacy fallback in `vista_pose` / `vista_pose_hub`
  (`pick_dive_safe_hub_shrunk` — same bound-then-composition rule on
  per-hub shrunk poses); `composition_pose_for_hub` takes `(w, h)`.
  Nominal path byte-identical (full-fit hubs exist → shrink never
  triggers). Test `interior_window_shrink_keeps_16_9_and_fits` green.
- CVR-005/FR5: `MapOrbitCamera::set_eye_target` (yaw/pitch/distance
  from the offset, zoom-band clamp, degenerate no-op) + test;
  `pose_inspector_for_slab_capture` in `main.rs` (slab = vista
  eye/target + 20° + 30 Mpc at the hub depth =
  `camera.distance()`); capture determinism test
  `slab_capture_pose_matches_vista_pose` green (eye/target 1e-3,
  deterministic re-pose). `SLAB_PRESET` / `VISTA_PRESET` doc comments
  note the FR3 pose.
- CVR-006: captures UHD 620, seed 1337, 1408×768 High:
  `vista-after.png` (`450450D9…`, 1.52 MB, ×2),
  `slab-after.png` (`714E9837…`, 0.60 MB, ×2),
  `vista-after-hint.png` (t = 1.5, Hold-identical to t = 0),
  `vista-after-dive.png` (t = 4, mid-dive, framing moved, no snap).
  Limb scans: vista `0.181`, slab `0.152` (tests
  `vista_reframed_shows_no_limb`, `slab_reframed_shows_no_limb`).
- CVR-009: rendering/journey/techstack/milestones as listed; techstack
  0.54.0. No engine / `game` / `tools` diff; no new GPU resource.

## ANALYST audit note (2026-09-23)

- Re-ran on the final tree (all green, this session): `fmt --check`,
  `clippy --all-targets --all-features -D warnings`, `build
  --workspace`, workspace tests (game 40 + debug lib 286 incl. the 2
  new limb scans + shrink + eye-target tests + nominal pose/dive pins
  with the numbers above + debug bin 45 incl. the slab-pose test +
  engine 311 incl. `all_nodes_inside_sphere` + re-recorded pins +
  game_tests 3 + tools 5), doc-tests (6 + 85), `game` (journey e2e
  incl. save round-trip + corrupt rejection + recovery ok) /
  `game_debug --headless` (`vista=done events=hold,dive,done`,
  traverse 507 Mpc / 10 rebases) / `game_tools --headless --tier low`,
  mobile `aarch64-linux-android` + `aarch64-apple-ios` checks,
  captures byte-identical ×2 (vista `450450D9…`, slab `714E9837…`).
- Reproduced: committed PNG hashes match the DEV record; `shots/`
  holds exactly the four preset PNGs (≤ 4 MB, correct names);
  shrink-filtered pose equals the full pose on the nominal seed
  (rank 1, NDC `(0.096, 0.000)`); slab camera carries the vista
  eye/target at 20° with the slice at the hub depth; diff scope =
  4 debug-crate files (`cosmic_vista.rs`, `map_camera.rs`,
  `cosmic_capture.rs` scans, `main.rs` slab seam + slab-pose test) +
  5 docs (`rendering.md`, `journey.md`, techstack README, milestones
  README, new readings report) + this feature's own notion/plan +
  shots (no engine/game/tools, no `done` parents, no `Cargo` files,
  no `unsafe` added or removed).
- Per-row verdicts: DoD 1 pass (0.181 / 0.152 ≤ 0.20, scan tests
  green); DoD 2 pass (focal frame `(0.548, 0.500)` inside
  `(0.55 ± 0.05, 0.5 ± 0.05)`, readings 5 met / 1 partial — the
  depth-cue partial is structural, recorded); DoD 3 pass (28.4 ≤ 30,
  hub exits s = 0.460 late, `Done == Chase`, headless pin, timed
  shots differ/progress as recorded); DoD 4 pass (named tests green);
  DoD 5 pass (docs match code; links checked below); DoD 6 pass
  pending SECURITY + the single commit + merge.

- Link check (touched docs): report addendum → visual-description §2
  + both shots resolve; rendering paragraph → ADR-026 + presets;
  journey boot sentence → rendering; techstack 0.54.0 → plan/shots;
  milestones F6 → notion/plan; notion → plan; plan → shots. No
  dangling links.

- Limitations accepted (no code findings; no issues filed):
  - L-1 (hands-on): no interactive session on this box — UX rows read
    off static captures + pins (precedent accepted). Re-verify
    hands-on post-merge if a windowed rig is available.

- E2E: covered by the `game` run above (boot → play → save/load →
  quit incl. corrupt + recovery) — no separate journey script applies
  to a debug-shell render feature; headless + tools-smoke + captures
  are the feature's e2e.

## SECURITY review note (2026-09-23)

- Scope: 4 debug-crate files (vista pose math + shrink fallback,
  orbit eye/target setter, capture slab seam + scan tests), 5 docs,
  this feature's plans + shots. Read the full `git diff` for
  `crates/` (plus the ANALYST scope verification above).
- Untrusted input: PNG decode runs only in unit tests over
  version-controlled committed shots (asserted RGBA8, bounded
  buffers, established `png` crate — no new decoder surface);
  no new CLI flag, env var, file I/O, parsing, or console command
  (the timed `VISTA_T` pre-roll path is pre-existing).
- Dependencies / `unsafe`: none added, none removed (`Cargo`
  files untouched, no `unsafe` in the diff).
- Saves / migration: untouched (no engine diff); the corrupt-save
  contract re-verified by the `game` run above.
- File I/O: production writes only via the pre-existing `--capture`
  path (explicit user action, unchanged semantics); tests are
  read-only.
- Findings: none — no `blocker`, no `should-fix`, no `note`.
  Verdict: pass.

