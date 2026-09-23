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
| CSC-001 | pending | Radial cut of refined peak positions in `assemble()` before `accept_peaks` (`r² ≤ R²`, `f64`, no `sqrt`) | FR1, NFR1 |
| CSC-002 | pending | `UNIVERSE_VERSION` 3 → 4; re-record `committed_web_vectors_pin_stage0` + every other pinned vector; equality pin between the two entry points still green | FR2, DoD 2 |
| CSC-003 | pending | New test `all_nodes_inside_sphere` over the nominal seed set (+ the doc-test params) | FR4, DoD 1 |
| CSC-004 | pending | Calibration bands on the bounded descriptor; retune `target_node_count` only if a band fails; record numbers in this plan | FR3, DoD 3 |
| CSC-005 | pending | Debug-crate radius tests: `hub_impostors`, `hub_members` (`≤ R + r_vir`), `link_segments` | FR4, DoD 1 |
| CSC-006 | pending | Shots `slab-after.png` + `inspector-after.png` (seed 1337); ANALYST corner-region pixel check | DoD 4 |
| CSC-007 | pending | Docs: `descriptor.rs` comments, visual-architecture report parameter row, `docs/game/universe.md`, techstack bump; link check | FR5, DoD 5 |
| CSC-008 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — cut placed at the only
point where refined positions exist; hashed-path rules listed; version
bump is the explicit migration path per ADR-004/ADR-026) · Todos
approved by: TECHLEAD (2026-09-23 — risk-first: cut + bump + pin
(CSC-001..003) before any calibration judgement; retune limited to one
constant so the diff stays auditable) · UX acceptance rows (if
player-facing): n-a · DoD verified by: ANALYST _(pending)_ · Security
reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | All nodes ≤ radius; hub/member/link radius tests | pending | | |
| 2 | `UNIVERSE_VERSION = 4`; pins re-recorded; determinism suite green | pending | | |
| 3 | Calibration bands green; retune recorded | pending | | |
| 4 | No hub sprite outside the limb on `slab` / `inspector` | pending | | |
| 5 | Docs + links | pending | | |
| 6 | Gates + audit + review + one commit | pending | | |

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
  return to PO (scope).
- R-2 (home node): the home band search (`1e12–1e13 M☉`) may find
  fewer candidates; the existing assert must pass on every nominal
  seed — if not, this is a PO question, not a silent widening.
- Next: `cosmic-rebase-async` (engine-free) starts on the bounded
  descriptor; every later shot in the version is taken after this
  commit.
