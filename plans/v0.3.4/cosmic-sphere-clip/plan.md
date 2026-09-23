# Plan — cosmic-sphere-clip

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/engine/src/universe/web/` only
(`descriptor.rs` acceptance, `mod.rs` version + pins,
`classify.rs` untouched — the cut sits where positions exist, in
`assemble()` before `accept_peaks`). Hashed path: **invariant rows**
— `r² ≤ R²` in `f64` (no `sqrt`, no float branch beyond the
comparison); node order stays densest-first; `generate_cosmic_web`
and `generate_cosmic_web_with_field` stay byte-equal to each other;
`UNIVERSE_VERSION` bump is the only allowed hash change in the
version (ADR-026 §1). Debug-crate tests in Phase 3 are read-only
consumers.

### Phase 1 — Cut + version (`descriptor.rs`, `mod.rs`)

Radial rejection of refined peak positions before greedy
acceptance; `UNIVERSE_VERSION = 4`; re-record every pinned vector
in one commit-local step; new `all_nodes_inside_sphere` pin over the
nominal seed set.

### Phase 2 — Calibration (`mod.rs` tests)

Run the band tests; if a band fails, retune `target_node_count`
(only) and record before/after numbers here. Home-node band and
`masses_decrease_with_density_rank` must hold without retune.

### Phase 3 — Downstream pins (`crates/debug`)

Radius tests on `hub_impostors`, `hub_members` (`≤ R + r_vir`),
`link_segments` endpoints; capture `slab-after.png` +
`inspector-after.png`; ANALYST corner check.

### Phase 4 — Docs + gates

`descriptor.rs` comments, visual-architecture report table row,
`docs/game/universe.md`, techstack bump; gate suite; audit; review;
single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CSC-001 | done | Radial cut of refined peak positions in `assemble()` before `accept_peaks` (`r² ≤ R²`, `f64`, no `sqrt`) | FR1, NFR1 |
| CSC-002 | done | `UNIVERSE_VERSION` 3 → 4; re-record `committed_web_vectors_pin_stage0` (5_587_830_784_546_330_281) + stage-1 pin (`galaxy_hash` 9_988_333_191_762_962_555; `system_hash` unchanged — version-free); equality pin between the two entry points still green | FR2, DoD 2 |
| CSC-003 | done | New test `all_nodes_inside_sphere` over the nominal seed set (+ the doc-test params) | FR4, DoD 1 |
| CSC-004 | done | Calibration bands on the bounded descriptor; `target_node_count` kept at 6 000 (no retune — all bands green); numbers recorded below | FR3, DoD 3 |
| CSC-005 | done | Debug-crate radius tests: `hub_impostors`, `hub_members` (`≤ R + r_vir`), `link_segments` | FR4, DoD 1 |
| CSC-006 | done | Shots `slab-after.png` + `inspector-after.png` (seed 1337); capture byte-identical (`B411DFC4…` twice on Intel UHD 620); ANALYST corner check below | DoD 4 |
| CSC-007 | done | Docs: `descriptor.rs` comments, visual-architecture report parameter row, `docs/game/universe.md`, techstack 0.48.0; link check | FR5, DoD 5 |
| CSC-008 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |
| CSC-009 | done | PO-directed scope extension (2026-09-23): dive-exact vista hub shortlist — the bounded descriptor moved the nominal dive to 35.3°/s vs the 30°/s bound; `interior_window` + focal composition + dive-scored pick (`vista_pose(web, radius, chase)`); nominal dive 28.4°/s, hub exits s=0.460, focal NDC (0.096, 0.000) | DoD 6 (gates-green enabler) |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — cut placed at the only
point where refined positions exist; hashed-path rules listed; version
bump is the explicit migration path per ADR-004/ADR-026; CSC-009
reviewed 2026-09-23 — pose stays a pure function of seed-derived
inputs, no new GPU resource, hashed paths untouched) · Todos
approved by: TECHLEAD (2026-09-23 — risk-first: cut + bump + pin
(CSC-001..003) before any calibration judgement; retune limited to one
constant so the diff stays auditable; CSC-009 approved 2026-09-23 —
knob sweep (dock 60/120/200, eye 180/220/260) exhausted first, hub
choice proven dominant by measurement, dive-exact shortlist is the
minimal honest fix) · UX acceptance rows (if
player-facing): n-a (no new interaction; dive bound unchanged) · DoD verified by: ANALYST (2026-09-23 — per-row audit below) · Security
reviewed by: SECURITY (2026-09-23 — no new input surface, dependency,
or `unsafe`; see review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | All nodes ≤ radius; hub/member/link radius tests | done (nominal seeds 1234/1337/11: max node r = 249.99/249.97/249.98 Mpc; small-box pins green) | `all_nodes_inside_sphere`, `nominal_hubs_inside_sphere`, `nominal_links_inside_sphere` | ANALYST 2026-09-23 |
| 2 | `UNIVERSE_VERSION = 4`; pins re-recorded; determinism suite green | done (`committed_web_vectors_pin_stage0` = 5_587_830_784_546_330_281; stage-1 galaxy pin = 9_988_333_191_762_962_555, system pin unchanged; `with_field_matches_plain_entry_point`, replay-identical, ulp pins green) | workspace `cargo test` green | ANALYST 2026-09-23 |
| 3 | Calibration bands green; retune recorded | done (no retune: nodes 4976/4876/4914 ∈ [3000, 8000]; void_frac 0.629 ∈ [0.60, 0.90]; home mass 9.5–9.7e12 ∈ band; `statistical_gates`, `void_bands`, `masses_decrease` green; before → after: nodes 6000 → ~4900, max_r 436 → 250 Mpc, links ~22.6k → ~18k) | `csc_numbers` release probe (before via stash, after on tree) + band tests | ANALYST 2026-09-23 |
| 4 | No hub sprite outside the limb on `slab` / `inspector` | done (nodes ≤ R by construction + radius tests; both captures byte-identical twice on the final tree, Intel UHD 620, 1408×768: inspector `B411DFC4…`, slab `23DDAA63…`; 1.4 MB + 0.9 MB ≤ 4 MB) | `shots/slab-after.png`, `shots/inspector-after.png` | ANALYST 2026-09-23 |
| 5 | Docs + links | done (`descriptor.rs` cut comment + `WebNode` bound doc, `params.rs` cut doc, report table row, `universe.md` sphere sentence, techstack 0.48.0; `cargo doc` shows no new warnings) | diff + doc build | ANALYST 2026-09-23 |
| 6 | Gates + audit + review + one commit | done (fmt/clippy/build/workspace-tests (40+265+37+310+3+5)/doc-tests (6+85)/`game`/headless/tools-smoke/android+ios guards green; CSC-009 dive 28.4°/s ≤ 30 nominal; this audit + SECURITY note) | gate logs 2026-09-23 | ANALYST + SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Hashed path stays integer-decision + `sqrt`-free for the cut
  (`r² ≤ R²`) [T: code review + determinism suite].
- A-2. `generate_cosmic_web(seed, p) == generate_cosmic_web_with_field(seed, p, _).0`
  still pinned [T].
- A-3. Node order remains densest-first; `masses_decrease_with_density_rank`
  green without change [T].
- A-4. No other hashed constant changes in this feature except
  `target_node_count` under CSC-004, and only with recorded numbers.

## Risks & Next steps

- R-1 (band drift): halving the candidate volume can shift the void
  fraction / filament-length bands. Mitigation: CSC-004 retunes one
  constant with numbers; if a band needs a second constant, stop and
  return to PO (scope). OUTCOME 2026-09-23: no retune — before → after
  (release probe, nominal): seed 1234 nodes 6000 → 4976, max_r 435.77
  → 249.99, links 22666 → 18337; seed 1337 nodes 6000 → 4876, max_r
  428.85 → 249.97; seed 11 nodes 6000 → 4914, max_r 424.27 → 249.98.
  Void fraction unchanged 0.629 (stage A/B untouched); home mass
  5.5–6.4e12 → 9.5–9.7e12 (still in band — deeper in-sphere peaks
  admitted, as predicted).
- R-2 (home node): the home band search (`1e12–1e13 M☉`) may find
  fewer candidates; the existing assert must pass on every nominal
  seed — if not, this is a PO question, not a silent widening.
  OUTCOME: passes on 1234/1337/11 (masses above).
- R-3 (vista dive, FOUND during implementation): the bounded
  descriptor moved the nominal home–hub geometry and the real dive
  (nearest Tier-A opening → Chase) whipped 35.3°/s vs the 30°/s bound
  (was 26.5°/s; hub still exited late at s=0.388). Knob sweep
  (dock 60/120/200, eye 180/220/260) proved the structure — not a
  constant — was wrong: dive rate is hub-dominated (5–58°/s across
  Tier-A on seed 1337). PO decision 2026-09-23: fix the pose now
  (CSC-009) instead of a separate feature — `interior_window` +
  focal composition + dive-exact shortlist
  (`vista_pose(web, radius, chase)`; composition primary, bound as
  filter, gentlest-dive fallback). OUTCOME: nominal pick rank 1,
  dive 28.4°/s, hub exits s=0.460, focal NDC (0.096, 0.000).
- Next: `cosmic-rebase-async` (engine-free) starts on the bounded
  descriptor; every later shot in the version is taken after this
  commit. `cosmic-vista-reframe` keeps its presets / shots /
  readings / UX todos (CVR-005..009) plus the 16:9 shrink fallback;
  its CVR-004 dive-bound core is satisfied by CSC-009 (cross-link).

## ANALYST audit note (2026-09-23)

- Re-ran: full workspace suite green on this tree (game 40, debug-lib
  265 incl. `real_dive_turns_slowly_on_nominal_seed` 28.4°/s, debug-bin
  37, engine 310 incl. `all_nodes_inside_sphere` + re-recorded pins,
  game_tests 3, tools 5); doc-tests 6 + 85; `game`, headless
  (`vista=done`), tools smoke, android + ios guards.
- Calibration numbers above reproduced from the release probe;
  before-numbers via `git stash` on the same binary.
- Captures reproduced byte-identical twice (inspector SHA256
  `B411DFC4…`); shots ≤ 4 MB (`inspector-after` 1.4 MB,
  `slab-after` 0.9 MB).
- No findings; no issues filed.

## SECURITY review note (2026-09-23)

- No new input surface (no CLI flag, file I/O, or parsing touched);
  no dependency change (`Cargo.lock` untouched); no new `unsafe`
  (grep clean); save path follows the existing `UNIVERSE_VERSION`
  regeneration contract (no stored web content).
- Verdict: pass, no blockers.
