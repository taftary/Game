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
| FIX-001 | in-progress | Invert rank->quantile in descriptor.rs:189 | Functional req 1 |
| FIX-002 | pending | Add monotonic pin test in web/mod.rs | Functional req 2 |
| FIX-003 | pending | Run existing tests, confirm statistical gates pass | Functional req 3 |
| FIX-004 | pending | Re-roll committed hash vector | Goal 3 |
| FIX-005 | pending | Bump UNIVERSE_VERSION 2->3 | Functional req 4 |
| FIX-006 | pending | Re-run full test suite after version bump | DoD 6 |
| FIX-007 | pending | Verify home node + spawn location | Goal 4 |
| FIX-008 | pending | Visual re-look in debug build | Goal 5 |
| FIX-009 | pending | Update spec #1 Stage C claim | DoD 8 |
| FIX-010 | pending | Update ADR-023 mass-assignment wording if needed | DoD 8 |

## Role sign-off

Breakdown approved by: ARCHITECT _(pending)_ . Todos approved by:
TECHLEAD _(pending)_ . UX acceptance rows: _(pending -- visual
hierarchy change)_. DoD verified by: ANALYST _(pending)_. Security
reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Code fix in descriptor.rs | pending | | |
| 2 | Monotonic pin test added | pending | | |
| 3 | Committed hash vector re-rolled | pending | | |
| 4 | Home node verified | pending | | |
| 5 | Statistical gates pass | pending | | |
| 6 | UNIVERSE_VERSION = 3 | pending | | |
| 7 | Visual re-look confirmed | pending | | |
| 8 | Spec #1 updated | pending | | |

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
