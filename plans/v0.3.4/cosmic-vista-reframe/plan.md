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
| CVR-001 | pending | `interior_window(R, c, W, H, T, margin) -> Option<(d, fov)>` + 16:9 fallback; tests (nominal fit, `None`, fallback) | FR1, Goals §1 |
| CVR-002 | pending | `vista_hub(&web)` (`rank_score − λ·|c|/R`, Tier A, footprint fit, fallback chain); tests: determinism, Tier A, `|c| ≤ 0.4·R` | FR2, Goals §2 |
| CVR-003 | pending | `vista_pose` from FR1/FR2 with focal offset `(0.55, 0.5)`; projection test on the nominal seed | FR3, DoD 2 |
| CVR-004 | pending | Dive re-dock from the new eye; existing bound / continuity / headless `Done == Chase` tests green; `VISTA_DOCK_MPC` retune recorded if needed | FR4, Goals §3, DoD 3 |
| CVR-005 | pending | `CapturePreset::VISTA` / `::SLAB` take the FR3 pose; capture determinism pin | FR5 |
| CVR-006 | pending | Shots `vista-after.png`, `slab-after.png`, `vista-after-hint.png`, `vista-after-dive.png`; corner limb scans recorded | DoD 1 |
| CVR-007 | pending | Six-reading table (report §2) for `vista-after.png` in a `docs/reports/` addendum; ≥ 5 met | Goals §5, DoD 2 |
| CVR-008 | pending | UX-1 / UX-2 hands-on session recorded | UX rows |
| CVR-009 | pending | Docs: `rendering.md` interior-window note, `journey.md` boot sentence, techstack bump; link check | DoD 5 |
| CVR-010 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit; milestones rows `done`; merge `v0.3.4 → main` | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — pure composition
functions on the existing camera contract; no new resource; every
invariant row inherited from `cosmic-vista-intro`) · Todos approved
by: TECHLEAD (2026-09-23 — math + tests before pose; dive bounds
re-run before any shot; the reading table is a todo with a numeric
threshold; version close is the last row) · UX acceptance rows: UX-1,
UX-2 below · DoD verified by: ANALYST _(pending)_ · Security reviewed
by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | No limb on `vista` / `slab` (corner scans) | pending | | |
| 2 | Focal hub position; six readings ≥ 5 met | pending | | |
| 3 | Dive bounds + continuity + headless pin | pending | | |
| 4 | Tests listed | pending | | |
| 5 | Docs + links | pending | | |
| 6 | Gates + audit + review + one commit + merge | pending | | |

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
  (recorded, tested on the seed set).
- R-2 (dive length): a central hub can be far from home, lengthening
  the dive or raising angular velocity; `VISTA_DOCK_MPC` and the 8 s
  dive are the two knobs, bound test is the gate.
- Next (post-v0.3.4): anisotropic splats; LOD / culling hardening
  from this version's measured costs; "seen" flag persistence.
