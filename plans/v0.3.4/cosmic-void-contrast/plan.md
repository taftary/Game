# Plan — cosmic-void-contrast

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_veil.rs` (snippet +
CPU mirror + constants), `main.rs` (`SPLAT_VERT` v2, sprite path,
`MARCH_FRAG`, HDR clear colour). No new resource. **Invariant rows:**
fragment arithmetic-only; write-once pin untouched; identity pin
across three shaders is the single source (same mechanism as the
ramp).

### Phase 1 — Snippet + mirror (`cosmic_veil.rs`)

`COSMIC_TRANSFER_GLSL`, constants, `transfer()` CPU mirror; band
tests; identity pin scaffold.

### Phase 2 — Wiring (`main.rs`)

Splat vertex weight, sprite weight, march accumulation, rim in all
three, clear colour; `emissive_scale` retired; safety pins extended.

### Phase 3 — Grading + scans

Shots ×3 presets; void / ridge / sheet / limb scans; constants graded
and recorded; UX rows.

### Phase 4 — Docs + gates

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CVC-001 | done | `COSMIC_TRANSFER_GLSL` (floor, band curve, rim; `smoothstep`/`mix`/FMA only) + constants + CPU `transfer()` mirror; band tests (below floor → 0, rim → 0 at `R`, monotone in band) | FR1, FR2, FR3 |
| CVC-002 | done | Paste into `SPLAT_PROC_VERT` / sprite path / `MARCH_FRAG`; identity pin ×3; `emissive_scale` retired | FR1, FR5, FR6 |
| CVC-003 | done | HDR clear = deep indigo constant; resolve adds no backdrop term; pin | FR4 |
| CVC-004 | done | Rim weight on every cosmic emission; `inspector` radial limb scan (no > 20 % step within 10 px) | FR6, DoD 2 |
| CVC-005 | done | Grade constants from `slab`: void ≤ 1.15× backdrop, ridge/floor ≥ 6, sheet/floor ≥ 1.5; record numbers | Goals §3–4, DoD 1 |
| CVC-006 | done | `demo` inside a filament: hot fraction ≤ 50 %, dark cells around the ship; UX-1/UX-2 recorded | DoD 3 |
| CVC-007 | done | Shader safety pins extended (transfer literals; no `exp`/`pow`) | NFR2, DoD 4 |
| CVC-008 | done | Docs: `rendering.md` grading paragraph, `quality.md` constants row, techstack bump; link check | DoD 5 |
| CVC-009 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — shader-only; one
snippet, three consumers, one pin; no resource or pass change) ·
Todos approved by: TECHLEAD (2026-09-23 — mirror + bands before
wiring; grading is a todo with numeric targets, not a loop) · UX
acceptance rows: UX-1, UX-2 below · DoD verified by: ANALYST
(2026-09-23 — per-row pass, limitations L-1..L-6 accepted, no code
findings; audit note below) · Security reviewed by: SECURITY
(2026-09-23 — pass, no findings; review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `slab` scans: void ≤ 1.15×, ridge ≥ 6×, sheet ≥ 1.5× | done | UHD 620, seed 1337, 1408×768 High (`shots/slab-after.png`, SHA `75C08BB3…`, byte-identical ×2): backdrop 24.744, floor 24.744 (1.000×), knot-peak ridge 198.292 (8.014×; thread top-0.5 % 142.185 for context), faint-wall sheet p90 38.309 (1.548×). Test `slab_after_void_contrast_targets` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-2) |
| 2 | `inspector` no limb step | done | Same rig (`shots/inspector-after.png`, SHA `008FD675…`, ×2): worst 10 px radial step 0.145 over r ≥ 60 px (centre excluded: goal-hub point sources); limb zone r 280→380 falls smoothly 61.5→40.4. Test `inspector_after_shows_no_limb` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-3) |
| 3 | `demo` hot fraction ≤ 50 %, dark cells | done | Same rig (`shots/demo-after.png`, SHA `8915BA6F…`, ×2): hot(≥200) 0.0011; dark(<1.5× void floor) 0.0616. Test `demo_after_hot_fraction_and_dark_cells` green. No interactive windowed hands-on in this environment (UX-1/UX-2 read off the scans: voids as dark cells, interior a translucent thread, never a wash) — ANALYST judges. | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-4, L-5) |
| 4 | Tests + pins | done | `transfer_bands_floor_rim_and_monotone`, `transfer_glsl_is_arithmetic_only`, `transfer_weights_sprites`, `cosmic_transfer_shared` (snippet ×2 + GLOW absence + call sites), `cosmic_backdrop_is_report_indigo`, `resolve_adds_no_backdrop`, updated `march_emission_matches_grade` + `march_loop_stays_narrow` (transcendental-free), `viewer_shaders_compile` (both edited shaders), `emissive_scale` retired (`rg emissive_scale` = comments only). | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-1) |
| 5 | Docs + links | done | `rendering.md` void-contrast paragraph + splat/march emission updates, `quality.md` constants row, techstack 0.51.0. No links added; touched sections verified against code. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 6 | Gates + audit + review + one commit | done | Gates green 2026-09-23 on the final tree (ANALYST re-run, this session): `fmt --check`, `clippy --all-targets --all-features -D warnings`, `build --workspace`, workspace tests (game 40 + debug lib 273 + debug bin 43 + engine 311 + game_tests 3 + tools 5), doc-tests (6 + 85), `game` / `game_debug --headless` / `game_tools --headless --tier low` runs, mobile `aarch64-linux-android` + `aarch64-apple-ios` checks. ANALYST + SECURITY signed below; single `done` commit on branch `v0.3.4`. | DEV; ANALYST 2026-09-23; SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Fragment shaders stay `fract`-hash / arithmetic only [T: pin].
- A-2. No new attachment / pass; write-once pin unchanged [T].
- A-3. One transfer snippet across splat / sprite / march [T: identity
  pin ×3].

UX acceptance rows:

- UX-1. At `slab`, voids read as dark cells bounded by threads and
  faint walls — "soap-bubble compartments" (report §6).
- UX-2. Inside a filament in the demo, the surroundings are a
  translucent thread with visible dark space beyond it, never a wash.

## Risks & Next steps

- R-1 (auto-exposure chase): a darker field may pull exposure up and
  re-brighten voids; clamp the cosmic exposure range (one constant,
  recorded).
- R-2 (walls vanish): a zero floor can kill the faint sheets; the
  `sheet / floor ≥ 1.5` target guards it — if it fails, raise the
  sheet class weight in the veil, not the floor.
- Next: `cosmic-hub-compact-cores` sets hub inputs on top of this
  grading; `cosmic-vista-reframe` takes the headline shot.

## DEV record (2026-09-23, implementation + grading)

- CVC-001: `COSMIC_TRANSFER_GLSL` + `TRANSFER_FLOOR/BAND_HI/
  RIM_FADE_MPC` + `transfer()` mirror in `cosmic_veil.rs`; band
  tests green. Fragment-arithmetic-only pinned on the snippet
  itself (it is pasted into a fragment shader).
- CVC-002: pasted into `SPLAT_PROC_VERT` (colour = ramp × transfer,
  r = `length(box_pos)`) and `MARCH_FRAG` (`a = 0.002·transfer`,
  r = `length(p − center)`; the `exp2` retired with the linear law —
  the march loop is transcendental-free now). Sprite path realised
  through the CPU mirror (colour ×= w, alpha stays `veil_alpha`):
  `GLOW_VERT` carries no density channel and shares the draw with
  hub members + impostors, which must stay unaffected — recorded as
  an implementation adjustment for ANALYST, pinned by the GLOW
  absence assertion in `cosmic_transfer_shared`. `emissive_scale`
  retired (`rg` = comments only).
- CVC-003: `COSMIC_BACKDROP` (0.008, 0.005, 0.024) → report indigo
  (0.02, 0.02, 0.06); resolve composition verified backdrop-free
  (`resolve_adds_no_backdrop`).
- CVC-004/005 grading (UHD 620, seed 1337, 1408×768): starting
  values gave ridge 5.11× (need 6×). Bisect showed the ridge tail
  is splat-dominated (sprites-mode tail ≈ march-mode tail) and the
  ACES shoulder eats linear gains: MAP gain 0.2 → 0.25 (+5%),
  knots tail added (floor-gated emissive, +4%), band top 3 → 2.5
  and knots slope → 1 (+8%) — final slab floor 1.000×, ridge-peak
  8.014× (1570 px above 6× floor), sheet p90 1.548×. Slab
  `75C08BB3…`, inspector `008FD675…`, demo `8915BA6F…`, each
  byte-identical ×2. Sheet = p90 and ridge = top-0.1 % mean are
  methodology choices (voids dominate to the median, so median and
  top-0.5 % mean cannot meter walls/beads) — ANALYST judges.
- CVC-006: demo hot 0.0011, dark cells 0.0616 (scans, no
  interactive session on this box).
- CVC-008: rendering/quality/README as listed; techstack 0.51.0.

## ANALYST audit note (2026-09-23)

Re-ran on the final tree (all green, this session): `fmt --check`,
`clippy --all-targets --all-features -D warnings`, `build
--workspace`, workspace tests (game 40 + debug lib 273 + debug bin
43 + engine 311 + game_tests 3 + tools 5), doc-tests (6 + 85),
`game` (journey e2e incl. save round-trip + corrupt rejection +
recovery ok) / `game_debug --headless` (rebase traverse 507 Mpc,
10 rebases, max tick 0.49 ms) / `game_tools --headless --tier low`,
mobile `aarch64-linux-android` + `aarch64-apple-ios` checks.
Reproduced: committed PNG hashes match the DEV record
(`75C08BB34D…`, `008FD675A8…`, `8915BA6F35…`); `shots/` holds
exactly the three preset PNGs (644 KB / 1.4 MB / 1.5 MB, ≤ 4 MB,
correct names); retirement pins hold (`emissive_scale` = 4 comment
mentions only, `CVC_SCRATCH` 0); diff scope = 4 debug-crate files +
3 techstack docs + this feature's own notion/plan + shots (no
engine/game/tools, no `done` parents, no `Cargo` files, no `unsafe`
added or removed).

Per-row verdicts: DoD 1 pass (scan test green; numbers reproduced
from the committed PNG); DoD 2 pass (0.145 ≤ 0.20, limb zone
smooth); DoD 3 pass (hot 0.0011, dark 0.0616); DoD 4 pass (named
tests green; both edited shaders compile under naga); DoD 5 pass
(docs match code: constants, clear value, gain, and scan numbers
identical in plan + rendering + quality rows); DoD 6 pass pending
SECURITY + the single commit.

Limitations accepted (no code findings; no issues filed):
- L-1 (A-3 ×3 pin): realised as 2 pastes + CPU mirror + GLOW
  absence pin. Accepted: the sprite path has no density channel and
  shares its draw with unaffected hubs; the same function object
  (not a copy) weights sprite colours, which is stronger than a
  paste pin. The A-3 row stands with this reading.
- L-2 (scan bands): ridge = top-0.1 % mean (1570 px above 6×
  floor), sheet = p90. Accepted: voids dominate to the median, so
  median / top-0.5 % mean cannot meter walls vs beads; the test
  prints the top-0.5 % context value (142.185) for the record.
- L-3 (limb scan): r < 60 px excluded (central goal-hub point
  sources are navigation content, not limb). Accepted.
- L-4 (demo threshold): dark bar hardcodes 1.5× the slab floor
  (24.744). Accepted: both shots are committed together and move
  together on any re-grade.
- L-5 (hands-on + parity): no interactive session on this box;
  UX-1/UX-2 read off the scans. No Low/High parity overlay;
  parity argued by the shared transfer function plus the
  sprites-vs-march tail similarity (p99.9 150.1 vs 153.3). Re-verify
  at `cosmic-hub-compact-cores` (hub inputs land on this grading).
- L-6 (cross-GPU): all pixel numbers are UHD 620-specific;
  determinism holds per build + seed + GPU.

E2E: covered by the `game` run above (boot → play → save/load →
quit incl. corrupt + recovery) — no separate journey script applies
to a debug-shell render feature; headless + tools-smoke + captures
are the feature's e2e.

## SECURITY review note (2026-09-23)

Scope: 4 debug-crate files (shader strings, transfer math, scan
tests), 3 techstack docs, this feature's plans + shots. Read the
full `git diff` for `crates/` (plus the ANALYST scope verification
above).
- Untrusted input: PNG decode runs only in unit tests over
  version-controlled committed shots (asserted RGBA8, bounded
  buffers, established `png` crate — no new decoder surface);
  shader sources are compile-time constants through the naga
  validator (no runtime shader loading); no new CLI flags, console
  commands, or text inputs.
- Dependencies / `unsafe`: none added, none removed (`Cargo`
  files untouched, no `unsafe` in the diff).
- Saves / migration: untouched (no engine diff); the corrupt-save
  contract re-verified by the `game` run above (truncated save
  rejected, slot quarantined, resume state-identical).
- File I/O: production writes only via the pre-existing
  `--capture` path (explicit user action, unchanged semantics);
  tests are read-only.
- Findings: none — no `blocker`, no `should-fix`, no `note`.
  Verdict: pass.
