# Notion — build-foundations

## Status

`done` (all DoD criteria checked — see `plan.md` DoD verification)

## Context

Project review (2026-09-14) found the repo is a well-organized,
docs-first scaffold, but the two stated top priorities — mobile and
performance — have zero infrastructure: no release/dev profile tuning,
no pinned toolchain, no allocator/logging decisions, no mobile
compile-guard, no CI, and open build-vs-use decisions (ECS, vulkano
fallback, device floor). Decisions locked with the maintainer:

- Mobile deploy + hardening stays M6; CI guards mobile *compilation*
  from now; device floor decided on paper before M1.
- Minimal CI check matrix (no releases).
- ECS: adopt `hecs`; scheduling stays custom.
- `vulkano` kept; measurable kill-switch written now.

## Problem & Needs

- Release builds have no size/speed tuning (`LTO`, `codegen-units`,
  `panic = "abort"`, `strip`); dev builds of heavy deps (`vulkano`,
  `naga`) run unoptimized. Cold-start (<5 s) and install-size budgets
  in `docs/techstack/quality.md` are unenforceable.
- Toolchain is unpinned ("Rust 1.85+") while reproducibility is
  claimed; MSRV is unverified.
- `hecs`/`tracing` choices are made but not declared or wired, so CI
  cannot cover them.
- "Phones first-class, not ports" is unverifiable: nothing checks that
  the workspace compiles for Android/iOS targets.
- "No CI" means cross-platform breakage (Linux/Win/mac) is found late
  and manually.
- ADR-001 has no vulkano fallback trigger; ADR-007/008/009 don't exist
  as files.

## Goals

- `[profile.release]` + `[profile.dev.package."*"]` tuning in the
  workspace `Cargo.toml`; `rust-toolchain.toml` pinning stable +
  fmt/clippy components.
- `hecs` + `tracing` (+ `tracing-subscriber` declared) in workspace
  deps, wired into `game_engine` so CI covers them.
- `.github/workflows/ci.yml`: host matrix (Win/Linux/mac) running the
  canonical gates, mobile compile-guard
  (`aarch64-linux-android`, `aarch64-apple-ios`), MSRV 1.85 check.
- ADR-001 amended with vulkano kill-switch; ADR-007 drafted (device
  floor proposal, devices finalized by M6); ADR-008 (`hecs`),
  ADR-009 (allocator default + `tracing`) written.
- Docs in sync per `AGENTS.md` (`quality.md` CI note, `stack.md` rows,
  `architecture.md` ground truth, milestones M1 line, README setup
  note), techstack version 0.2.0.

## Non-goals

- No mobile deployment, devices, or touch work (stays M6).
- No renderer/sim/logic code; deps are declared and wired, not used.
- No release publishing, signing, or store metadata.
- No `fat` LTO (deferred to pre-ship); `thin` LTO for now.

## Users / Stakeholders

- Developers / contributors (only). No player-facing change.

## Functional requirements

- Workspace `Cargo.toml` has `[profile.release]` (`lto = "thin"`,
  `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`) and
  `[profile.dev.package."*"]` (`opt-level = 1`).
- `rust-toolchain.toml` exists (`channel = "stable"`, components
  `rustfmt`, `clippy`).
- Workspace deps include `hecs`, `tracing`, `tracing-subscriber`;
  `game_engine` depends on `hecs` + `tracing`.
- `.github/workflows/ci.yml` exists: gates matrix, mobile
  compile-guard, MSRV job.
- `docs/decisions/ADR-001.md`, `ADR-007.md`, `ADR-008.md`,
  `ADR-009.md` exist; `docs/decisions/README.md` links them with
  statuses.

## Non-functional requirements

- `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo build --workspace`,
  `cargo test --workspace --all-targets`, `cargo test --doc
  --workspace`, both bins run — all pass locally.
- `cargo check --workspace --target aarch64-linux-android` and
  `--target aarch64-apple-ios` pass locally (targets installed
  ad-hoc; not forced on contributors).
- Follows repo naming, edition/version workspace inheritance, and the
  docs-maintenance rules in `AGENTS.md`.

## Definition of Done

- [ ] Release/dev profiles + toolchain pin land; workspace builds green.
- [ ] `hecs`/`tracing` declared and wired; clippy/test clean.
- [ ] CI workflow present and green (matrix + mobile guard + MSRV).
- [ ] Four ADRs written; decisions log links them with statuses.
- [ ] Docs updated (`quality.md`, `stack.md`, `architecture.md`,
  milestones, README, techstack 0.2.0); all touched links resolve.
- [ ] This `plan.md` records gate + CI evidence per criterion.

## Constraints & Assumptions

- Mobile targets are CI-and-opt-in-local only; contributors are not
  required to install them (keep `rust-toolchain.toml` minimal).
- MSRV 1.87 must hold with the committed `Cargo.lock`; if a locked dep
  needs newer rustc, document the true MSRV instead of downgrading.
- CI mirrors the canonical gate list in `docs/techstack/quality.md`;
  it never becomes a second source of truth.

## Open questions

- None for the scaffold. Allocator default (`mimalloc`) is validated
  in M1; reference devices are finalized by M6 per ADR-007.
