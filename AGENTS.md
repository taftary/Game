# AGENTS.md - Instructions for AI Coding Assistants

This file is the single source of truth for AI assistants working on this
repository. Read it fully before making changes. If you change anything this
file documents, update this file to match.

## Project overview

Game is a Rust workspace (procedural colony universe). Engine direction and
milestones live in [`docs/`](docs/README.md) (`techstack/`, `game/`,
`milestones/`, `risks/`, `decisions/`); per-feature work
lives in [`plans/`](plans/README.md). This file is the entry point for
day-to-day agent commands and workflow pointers — details stay where they
belong, linked below.

## Commands

Local gates live in [`docs/techstack/quality.md`](docs/techstack/quality.md) — run them
before committing. Test policy and perf budgets: same file.

## Feature lifecycle (plans/)

Canonical spec: [`plans/README.md`](plans/README.md) — read it before creating
or changing any feature and follow it verbatim.

## Docs maintenance

Every feature or change must update the docs it affects and verify them:

- `docs/` — update the part your change touches (`techstack/`, `game/`,
  `milestones/`, `risks/`, `decisions/`); bump the Version line in
  `docs/techstack/README.md`.
- `plans/<feature>/` — keep `notion.md` / `plan.md` (and update/issue files)
  in sync with what actually landed, including DoD evidence.
- Verify: every link you touched resolves, no section contradicts another,
  and anything this file documents still matches — otherwise update this file too.


