# Plan — Cosmic-to-Subterranean Scale Hierarchy

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Canonical contract (`docs/game/journey.md`)

Rewrite `## Scales` as the 8-level table (level, name, real-world
reference, compressed game extent, v1 representation, key entities,
domain category); keep `## Rules` + `## State machine` semantics,
annotate state machine with level numbers. Add the drill-down diagram
(levels ↔ domains ↔ v1 states) and the Level 8 deep-dive subsection.
Owning area: `docs/game` (no code module). Invariants: rendering
conventions, determinism, save format — **untouched** (docs-only).
Locked stack choices — respected, none changed, no ADR.

### Phase 2 — Satellite docs sync

`docs/game/README.md`, `scope.md`, `universe.md`, `gameplay.md`,
`docs/milestones/README.md` (annotations only, no renumbering),
`docs/techstack/README.md` (version bump), `docs/risks/README.md`
(one risk line). Dependency-safe: contract (Phase 1) before surfaces.

### Phase 3 — Verification

Stale-reference grep, touched-link resolution, `quality.md` gates green
(docs-only change; code untouched), DoD evidence rows filled.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| SCALE-001 | done | Rewrite journey.md: 8-level table + diagram + L8 deep-dive | Goals 1–3; Functional requirements 1–3 |
| SCALE-002 | done | Sync game/README.md, scope.md, gameplay.md | Goal 4; Functional requirements 4 |
| SCALE-003 | done | Sync universe.md, milestones/README.md | Goal 4; Functional requirements 4 |
| SCALE-004 | done | Techstack version bump + risks line | Goal 4; Functional requirements 4 |
| SCALE-005 | done | Verify: stale-ref grep, link resolution, quality gates green | Definition of Done |

## Role sign-off

Breakdown approved by: ARCHITECT _(implementing agent, 2026-09-16 —
no module boundary, lock, or invariant touched; no ADR)_ · Todos
approved by: TECHLEAD _(implementing agent, 2026-09-16 — risk-first
order, every todo verifiable, no perf budget impact: docs-only, tier
gates n-a)_ · UX acceptance rows (if player-facing): consulted, no UI
or control change — journey extended, not contradicted · DoD verified
by: ANALYST _(implementing agent, 2026-09-16 — all 7 rows evidenced,
gates green, E2E n-a with reason)_ · Security reviewed by: SECURITY
_(implementing agent, 2026-09-16 — pass, no findings, attack surface
unchanged)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | 8 level rows, all columns; old 6-row table gone | done | `journey.md ## Scales`: 8 data rows (`^\| [1-8] \|` count=8); stale grep (`Planet globe \||Sky → soil \||six-layer|6-layer|…`) across `docs/` empty | ANALYST _(implementing agent, 2026-09-16)_ |
| 2 | Drill-down diagram: levels ↔ domains ↔ v1 states | done | `journey.md ## Drill-down (zoom order)`: 4 domains, L1–L8 each with v1 state (L1/L8 "no v1 state") | ANALYST _(implementing agent, 2026-09-16)_ |
| 3 | L8 deep-dive, 3 sub-tiers + v1 ownership | done | `journey.md ## Level 8 — Subterranean deep-dive`: shallow/mid/deep present (3/3), "markers only" v1 ownership stated | ANALYST _(implementing agent, 2026-09-16)_ |
| 4 | 6 satellites + version + risks consistent; stale grep empty | done | `game/README`, `scope`, `universe`, `gameplay`, `milestones/README`, `risks/README` edited; `git status` shows exactly those 8 docs files + `plans/scale-hierarchy/`; no code touched | ANALYST _(implementing agent, 2026-09-16)_ |
| 5 | Techstack version bumped | done | `techstack/README.md` Version `0.9.0` (2026-09-16, scale hierarchy) | ANALYST _(implementing agent, 2026-09-16)_ |
| 6 | Every touched link resolves | done | Link-resolution script over 8 docs + 2 plan files: `ALL LINKS RESOLVE` (incl. new `journey.md`↔`gameplay.md`/`universe.md`/`scope.md`/`milestones`/`risks` cross-links) | ANALYST _(implementing agent, 2026-09-16)_ |
| 7 | Notion + plan complete with Roles / sign-off rows | done | `notion.md ## Roles` filled (PO author, UX+ARCHITECT consulted); `plan.md ## Role sign-off` filled; gates below green | ANALYST _(implementing agent, 2026-09-16)_ |

Gates (all green 2026-09-16, unchanged code): `cargo fmt --check` OK;
`cargo clippy --workspace --all-targets --all-features -- -D warnings`
OK; `cargo build --workspace` OK;
`cargo test --workspace --all-targets` OK (24+85+17+61 passed, 0 failed);
`cargo test --doc --workspace` OK (10 passed);
`cargo run --bin game` exit 0 (48 ticks + `done:`);
`cargo run -p game_debug -- --headless` exit 0;
`cargo run -p game_tools -- --headless --tier low` exit 0.
E2E / save round-trip: explicitly out-of-scope (docs-only, no behavior
change) per notion Constraints.

SECURITY review _(implementing agent, 2026-09-16)_: **pass, no findings**.
Docs-only change — no new dependency, no `unsafe`, no serialization /
file-I/O / input-parsing / migration change, no new crate, no console
command, no input surface. Corrupt-save contract and locked stack
untouched; attack surface identical.

## Acceptance criteria

- `journey.md` contains one `## Scales` table with 8 data rows.
- Grep for stale scale language (`Planet globe |`, `Sky → soil |`,
  six-layer, `6-layer`) across `docs/` returns nothing outside this
  plan's history notes.
- Every relative link in touched files resolves to an existing path.
- Local gates from `docs/techstack/quality.md` green:
  `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo build --workspace`,
  `cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
  `cargo run --bin game`,
  `cargo run -p game_debug -- --headless`,
  `cargo run -p game_tools -- --headless --tier low`.
- Rendering conventions untouched: no change to cameras, projection,
  front-face/cull, ENU frame, or picking (AGENTS.md invariants).

## Risks & Next steps

- M2 implements the descent state machine against this table (levels
  5–7); any wording drift between journey.md and M2's notion must be
  caught at M2's PO review.
- Level 8 geometry and traversable Level 1 are post-v1; when scoped,
  they enter via update folders or new features, never in-place edits.
- Landable moons (Level 4) are a future universe.md axis, not v1.
