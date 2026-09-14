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
  <feature-name>/          # kebab-case, unique per idea
    notion.md              # needs, Status + Definition of Done (defines)
    plan.md                # organized features + todo list (checks)
    update-YYYY-MM-DD-HHMM/
      notion.md
      plan.md
    issue-YYYY-MM-DD-HHMM-<slug>/
      specs.md             # intake, day one
      report.md            # added after investigation
      plan.md              # added after investigation
```

## Rules

### 1. Feature folder

- Name: kebab-case only, `^[a-z0-9]+(-[a-z0-9]+)*$`, e.g. `voxel-terrain/`. Must be unique; never reuse a name. `_template/` is reserved (underscore prefix can never collide).
- Copy `_template/notion.md` → `<feature>/notion.md`, fill it first. Copy `_template/plan.md` → `<feature>/plan.md` only after notion is written. Never the reverse.
- Valid feature = both files present.

### 2. Status (Definition of Done tracking)

Single vocabulary everywhere (feature `notion.md`, update `notion.md`, issue `specs.md`):

`draft → planned → in-progress → in-review → done`, plus `on-hold`, `cancelled`.

- `done` only when every DoD criterion is checked in the corresponding `plan.md`.
- `on-hold` / `cancelled` require a one-line reason next to the status.

### 3. Definition of Done

- `notion.md` **defines**: `## Definition of Done` = bullet list of verifiable criteria.
- `plan.md` **checks**: `## DoD verification` table maps each criterion → status + evidence.

### 4. Updates (after done)

- Allowed only when parent feature `Status: done`.
- Folder `update-YYYY-MM-DD-HHMM/` directly under the feature. Timestamp is **UTC**, 24h (e.g. `update-2026-09-14-1430`). Collision → append `-02`, `-03`.
- Self-contained: own `notion.md` (from `update-notion.md`) + own `plan.md` (from `update-plan.md`). Todo IDs namespaced per update: `UPD-YYYYMMDD-001`, …
- Never edit parent files in place. Parent stays `done`; cross-link parent ↔ update.

### 5. Issues (inside a feature)

- Folder `issue-YYYY-MM-DD-HHMM-<slug>/` directly under the feature. Timestamp **UTC**, 24h; `<slug>` is a kebab-case symptom (e.g. `issue-2026-09-14-1430-crash-on-load`).
- Lifecycle files:
  1. `specs.md` (from `issue-specs.md`) — intake, required day one. Holds `Status` (same vocabulary).
  2. `report.md` (from `issue-report.md`) — added after investigation. `Status` moves to `in-review`.
  3. `plan.md` (from `issue-plan.md`) — added after investigation, fix tasks with IDs `ISS-YYYYMMDD-001`, … Only then implement.
- Never rewrites parent feature/update files — cross-links only. Issue `done` requires `report.md` + `plan.md` present and all fix todos checked.
