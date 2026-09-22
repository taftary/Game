# Plan -- cosmic-web-mass-rank-fix

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 -- code fix

1. Invert rank-to-quantile in `descriptor.rs:189`.
2. Add monotonic pin test in `web/mod.rs`.
3. Run existing tests to confirm statistical gates pass.
4. Run test, capture new `web_hash` value, re-roll committed vector.

### Phase 2 -- version bump + docs

5. Bump `UNIVERSE_VERSION` 2 -> 3 in `universe/mod.rs`.
6. Re-run full test suite (hash vector, ulp test, stage replays).
7. Verify home node: spawn location, filament proximity.
8. Visual re-look in debug build.
9. Update spec #1 Stage C claim (now factually correct post-fix).
10. Update ADR-023 if needed (mass-assignment wording).

## Todo

| ID | Status | Task | Ref notion section |
|----|--------|------|--------------------|
| FIX-001 | completed | Invert rank->quantile in descriptor.rs:189 | Functional req 1 |
| FIX-002 | completed | Add monotonic pin test in web/mod.rs | Functional req 2 |
| FIX-003 | completed | Run existing tests, confirm statistical gates pass | Functional req 3 |
| FIX-004 | completed | Re-roll committed hash vector | Goal 3 |
| FIX-005 | completed | Bump UNIVERSE_VERSION 2->3 | Functional req 4 |
| FIX-006 | completed | Re-run full test suite after version bump | DoD 6 |
| FIX-007 | completed | Verify home node + spawn location | Goal 4 |
| FIX-008 | completed | Visual re-look in debug build | Goal 5 |
| FIX-009 | completed | Update spec #1 Stage C claim | DoD 8 |
| FIX-010 | completed | Update ADR-023 mass-assignment wording if needed | DoD 8 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20, retroactive at the
v0.3.3 cut — one-line rank inversion inside `engine::universe::web`,
no boundary crossing, version bump per the determinism contract) ·
Todos approved by: TECHLEAD (2026-09-20, retroactive) · UX acceptance
rows: n-a (visual hierarchy follows physics; player-facing effect is
"giant hubs on dense peaks", covered by the illustris-look UX rows) ·
DoD verified by: ANALYST (2026-09-20, retroactive — see evidence
column; `cargo test -p game_engine --lib universe::web` 24/24 green on
branch `v0.3.3` at the cut) · Security reviewed by: SECURITY
(2026-09-20, retroactive — no I/O, no serialization, no dependency
change; `UNIVERSE_VERSION` bump is the documented migration path).

Recorded deviation: the feature was committed to `main` before the
sign-off rows were filled (squash `e60a036`); the rows above were
completed at the v0.3.3 cut against the code as landed. See
`plans/README.md` § *7. Version branch* (2026-09-20 deviation).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Code fix in descriptor.rs | completed | `descriptor.rs:189` reads `u = 1.0 - (rank + 0.5)/count` (densest peak → rarest quantile) | ANALYST |
| 2 | Monotonic pin test added | completed | `web/mod.rs:281` `masses_decrease_with_density_rank` green | ANALYST |
| 3 | Committed hash vector re-rolled | completed | `web/mod.rs:345-350` `committed_web_vectors_pin_stage0` = `17_301_221_795_867_311_725`, green | ANALYST |
| 4 | Home node verified | completed | `home_node` band 1e12–1e13 asserted by the `generate_cosmic_web` doc test + `cosmic_player` spawn tests (game_debug lib green per illustris-look DoD-4) | ANALYST |
| 5 | Statistical gates pass | completed | `statistical_gates_hold_at_nominal`, `void_bands_hold_at_nominal` green (24/24 module tests, 38 s) | ANALYST |
| 6 | UNIVERSE_VERSION = 3 | completed | `universe/mod.rs:60` `pub const UNIVERSE_VERSION: u32 = 3` | ANALYST |
| 7 | Visual re-look confirmed | completed | Covered by the illustris-look grading rounds D-1…D-7 on the corrected hierarchy (giant impostors on dense hubs); reproducible shots arrive with v0.3.3 `cosmic-capture-harness` | ANALYST |
| 8 | Spec #1 updated | completed | `classify.rs:7-8` / `descriptor.rs:3-4` wording now matches behaviour ("densest peak gets the rarest mass"); ADR-023 wording unchanged (already correct) | ANALYST |

## Acceptance criteria

- Densest accepted peak has the largest mass.
- Least-dense accepted peak has the smallest mass.
- `web_hash` matches the new committed vector.
- Home node is in 1e12-1e13 Msun band, nearest center.
- No visual regression: giant impostors sit on dense hubs.

## Risks & Next steps

- Home node may move to a different position (different node in band).
  Player spawn adjusts accordingly; cosmic_player tests cover this.
- The cinematic refresh DoD 8 visual sign-off must cover the corrected
  landscape (giant impostors now on dense hubs, bloom halos at nodes).
