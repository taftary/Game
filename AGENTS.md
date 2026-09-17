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

## Roles

Lifecycle roles live in [`docs/roles/`](docs/roles/) (PO, UX, ARCHITECT,
TECHLEAD, DEV, ANALYST, SECURITY); gates in [`plans/README.md`](plans/README.md)
§ *Role gates*. Before acting as a role, read its profile in `docs/roles/`
and fill the `Roles` / sign-off rows in the `plans/_template/` file you touch.

## Rendering invariants (read before touching cameras/projection/picking)

Binding contract, detailed in
[`docs/techstack/rendering.md`](docs/techstack/rendering.md) § *Camera &
screen-space conventions* — three shipped bugs came from these rules
existing nowhere (`issue-2026-09-14-2113`, `issue-2026-09-15-1144`,
`issue-2026-09-16-0851`):

- NDC `+1` = **top** row (y-down pixels); the UI ortho and flat MVP
  already prove it.
- `OrbitCamera::projection_matrix` is glam `directx::perspective`
  (RH, Z ∈ [0, 1], **no Y-flip**) — never `vulkan::perspective`.
- Un-flipped projection ⇒ `FrontFace::CounterClockwise` +
  `CullMode::Back`; flip projection or front-face, never both.
- The player world frame is true ENU (`east × north == up`); headings
  are compass bearings; turn input steers toward the avatar's own right.
- Picking uses `ndc = (2u−1, 1−2v)` on the sphere viewport; the
  player marker uses only `world_to_pixels`.
- Follow opens south of the player looking north; FirstPerson hides
  the marker by design.

## Docs maintenance

Every feature or change must update the docs it affects and verify them:

- `docs/` — update the part your change touches (`techstack/`, `game/`,
  `milestones/`, `risks/`, `decisions/`); bump the Version line in
  `docs/techstack/README.md`.
- `plans/<version>/<feature>/` — keep `notion.md` / `plan.md` (and update/issue files)
  in sync with what actually landed, including DoD evidence.
- Verify: every link you touched resolves, no section contradicts another,
  and anything this file documents still matches — otherwise update this file too.


