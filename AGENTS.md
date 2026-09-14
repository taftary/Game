# AGENTS.md - Instructions for AI Coding Assistants

This file is the single source of truth for AI assistants working on this
repository. Read it fully before making changes. If you change anything this
file documents, update this file to match.

## Project overview

Game is a freshly cleaned Rust workspace scaffold. The project is a blank slate to grow from.

## Commands

```text
cargo build --workspace
cargo run --bin game
cargo test --workspace --all-targets
cargo test --doc --workspace
cargo check --example viewer --features gpu
```

## Feature lifecycle (plans/)

Read [`plans/README.md`](plans/README.md) before creating or changing any
feature. Agents must follow this cycle:

- Feature folder: `plans/<feature-name>/`, kebab-case only
  (`^[a-z0-9]+(-[a-z0-9]+)*$`), unique. Copy empty templates from
  `plans/_template/`; never invent file names.
- Order: `notion.md` first (needs + `Status` + `Definition of Done`), then
  `plan.md` (organized features + todo list + DoD verification). Implement
  from `plan.md` only.
- Status vocabulary everywhere (`notion.md`, update `notion.md`, issue
  `specs.md`): `draft -> planned -> in-progress -> in-review -> done`, plus
  `on-hold` / `cancelled` (require a reason). `done` only when every DoD
  criterion is checked with evidence.
- Updates after done: `update-YYYY-MM-DD-HHMM/` (UTC) under the feature, own
  `notion.md` + `plan.md`, todos `UPD-YYYYMMDD-001`, ... Never edit the parent
  in place; cross-link.
- Issues: `issue-YYYY-MM-DD-HHMM-<slug>/` (UTC) under the feature,
  `specs.md` (intake, holds `Status`) -> `report.md` (investigation) +
  `plan.md` (fix, todos `ISS-YYYYMMDD-001`, ...). Never rewrite parents.


