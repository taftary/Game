# Notion — debug-screens

## Status

`done` (all DoD criteria checked — see `plan.md` DoD verification)

## Context

Developers have no in-app visibility into the running game: no FPS overlay, no
log console, no engine-state inspector. All diagnosis currently means reading
stdout or attaching external tools. The repo scaffold (`crates/engine`,
`crates/game`, `crates/tools`) has no home for developer screens.

## Problem & Needs

- Developers need debug screens to monitor, debug, and check the application
  while it runs (frame health, logs, engine state).
- The screens must live in their own crate under `crates/` so `game`/`tools`
  can consume them without polluting the engine or shipping them by accident.

## Goals

- New package `game_debug` at `crates/debug/`, wired into the workspace.
- Screen skeleton as a library: FPS/perf overlay, log console, engine-state
  inspector (stubs now, rendered in M1+).
- Own runnable game binary (`cargo run --bin game_debug`): `game` stays the
  clean release entry; the debug binary runs the same game with developer
  tools and extra displays on top.
- Workspace builds green with the new member; gates cover it.

## Non-goals

- No rendered UI yet (no `vulkano`/`winit` drawing code — M1+).
- No wiring into `game`/`tools` binaries yet (M1+).
- No new third-party dependencies beyond `game_engine`.

## Users / Stakeholders

- Developers (only). Never player-facing; strip or gate before any release.

## Functional requirements

- `crates/debug/` exists with `Cargo.toml` (package + lib `game_debug`) + `src/lib.rs`.
- `game_debug` binary (`src/main.rs`) runs the game with debug tools; `game` stays untouched release entry.
- Public modules: `fps`, `console`, `inspector`, each with a documented stub type.
- Workspace `Cargo.toml` lists `crates/debug` as a member.

## Non-functional requirements

- `cargo build --workspace`, `cargo test --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  and `cargo fmt --check` all pass.
- Follows repo naming (`game_*`) and edition/version workspace inheritance.

## Definition of Done

- [ ] `crates/debug/` package exists (lib screens + runnable game binary), builds, zero behavior.
- [ ] Both binaries run (`game`, `game_debug`); all gates green (evidence below).
- [ ] `docs/techstack/architecture.md` documents the new crate; techstack version bumped.

## Constraints & Assumptions

- Non-default workspace member (like `crates/tools`); `--workspace` gates still cover it.
- Depends only on `game_engine` for now.

## Open questions

- None for the scaffold. Screen rendering + `game`/`tools` integration are M1+ decisions.
