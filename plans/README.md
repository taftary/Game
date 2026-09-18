# Plans lifecycle

Single source of truth for how `plans/` works: `notion.md` → `plan.md` → implement → done → update / issue.

## Structure

```text
plans/
  README.md                # this file
  _template/               # empty templates, copy from here (never a feature)
    notion.md
    plan.md
    update-notion.md
    update-plan.md
    issue-specs.md
    issue-report.md
    issue-plan.md
  <version>/               # v<MAJOR>.<MINOR>.<PATCH> release folder (grouping only, never a feature)
    <feature-name>/        # kebab-case, unique per idea
      notion.md            # needs, Status + Definition of Done (defines)
      plan.md              # organized features + todo list (checks)
      update-YYYY-MM-DD-HHMM/
        notion.md
        plan.md
      issue-YYYY-MM-DD-HHMM-<slug>/
        specs.md           # intake, day one
        report.md          # added after investigation
        plan.md            # added after investigation
```

Current version folders (contents + per-version scope:
[`../docs/milestones/`](../docs/milestones/)):

- `v0.0.1/` — Foundations (done + in-review features)
- `v0.1.0/` — Navigation core (navigation-engine spec §2, §3, §5, §6)
- `v0.2.0/` — Scale rendering & visuals (spec §4, §9)
- `v0.3.0/` — Player-facing layer (spec §10 + debug screens)
- `v0.3.1/` — Debug shell unification (ADR-022: `unified-debug-view`)
- `v0.3.2/` — Cosmic-scale player (main game notion: `cosmic-scale-player`)

## Role gates

Profiles: [`../docs/roles/`](../docs/roles/) — PO, UX, ARCHITECT, TECHLEAD,
DEV, ANALYST, SECURITY. Agents read the matching profile before acting as
that role. Gates are documented (agents self-enforce); nothing here is scripted.

| Step | Required sign-off |
|---|---|
| `notion.md` `draft → planned` | PO (author); UX consulted if player-facing |
| `plan.md` → implementation | ARCHITECT (breakdown) + TECHLEAD (todos) |
| todos `in-progress → in-review` | DEV (gates green + evidence) → ANALYST (audit) |
| `in-review → done` | ANALYST (DoD verified) + SECURITY (review) |
| update `notion.md` / `plan.md` | same as feature, scoped to the delta |
| issue `specs.md` → investigate | PO (priority) |
| issue `report.md` → fix plan | TECHLEAD (+ ARCHITECT if invariant touch) |
| issue fix → `done` | ANALYST + SECURITY |

DEV never self-approves DoD; ANALYST never implements; SECURITY `blocker`
stops `done` until re-reviewed.

## Rules

### 1. Version folder

- Name: `^v\d+\.\d+\.\d+$` (e.g. `v0.1.0/`), matching a release in
  `docs/milestones/`. Grouping only — a version folder is never a feature
  and holds no `notion.md` of its own.
- Every feature lives in exactly one version folder. Moving a feature to
  another version = moving the whole folder; every doc link to it must
  follow (AGENTS.md docs-maintenance rule).

### 2. Feature folder

- Name: kebab-case only, `^[a-z0-9]+(-[a-z0-9]+)*$`, e.g. `voxel-terrain/`. Must be unique across **all** versions; never reuse a name. `_template/` is reserved (underscore prefix can never collide).
- Copy `_template/notion.md` → `<version>/<feature>/notion.md`, fill it first. Copy `_template/plan.md` → `<version>/<feature>/plan.md` only after notion is written. Never the reverse.
- Valid feature = both files present.

### 3. Status (Definition of Done tracking)

Single vocabulary everywhere (feature `notion.md`, update `notion.md`, issue `specs.md`):

`draft → planned → in-progress → in-review → done`, plus `on-hold`, `cancelled`.

- `done` only when every DoD criterion is checked in the corresponding `plan.md`.
- `on-hold` / `cancelled` require a one-line reason next to the status.

### 4. Definition of Done

- `notion.md` **defines**: `## Definition of Done` = bullet list of verifiable criteria.
- `plan.md` **checks**: `## DoD verification` table maps each criterion → status + evidence.

### 5. Updates (after done)

- Allowed only when parent feature `Status: done`.
- Folder `update-YYYY-MM-DD-HHMM/` directly under the feature. Timestamp is **UTC**, 24h (e.g. `update-2026-09-14-1430`). Collision → append `-02`, `-03`.
- Self-contained: own `notion.md` (from `update-notion.md`) + own `plan.md` (from `update-plan.md`). Todo IDs namespaced per update: `UPD-YYYYMMDD-001`, …
- Never edit parent files in place. Parent stays `done`; cross-link parent ↔ update.

### 6. Issues (inside a feature)

- Folder `issue-YYYY-MM-DD-HHMM-<slug>/` directly under the feature. Timestamp **UTC**, 24h; `<slug>` is a kebab-case symptom (e.g. `issue-2026-09-14-1430-crash-on-load`).
- Lifecycle files:
  1. `specs.md` (from `issue-specs.md`) — intake, required day one. Holds `Status` (same vocabulary).
  2. `report.md` (from `issue-report.md`) — added after investigation. `Status` moves to `in-review`.
  3. `plan.md` (from `issue-plan.md`) — added after investigation, fix tasks with IDs `ISS-YYYYMMDD-001`, … Only then implement.
- Never rewrites parent feature/update files — cross-links only. Issue `done` requires `report.md` + `plan.md` present and all fix todos checked.

### 7. Version branch

Each version folder `vX.Y.Z` gets a same-named git branch, cut from
`main` when version work starts.

- The full lifecycle (notion → plan → implement → updates/issues) runs
  on the version branch.
- **Each feature = exactly one commit**, made only when the feature
  reaches `done` (all gates green, ANALYST + SECURITY signed). Notion,
  plan, code, and DoD evidence accumulate uncommitted in the working
  tree until then.
- Docs-only commits (`docs:` — workflow rules, PO decisions, ADRs) are
  allowed as separate commits on the branch.
- The version closes by merging the branch back into `main` when every
  feature in the version folder is `done`. No release tags on the
  branch name (the branch occupies it).

Recorded deviation (PO decision 2026-09-17): v0.2.0 feature work lands
directly on `main` — the `v0.2.0` branch holds only a duplicate of the
log-depth commit (`b842bb7` ≈ `d72a545`) and is dropped; every other
rule here (one commit per `done` feature, docs-only commits separate)
still applies. The branch rule resumes for the next version.
