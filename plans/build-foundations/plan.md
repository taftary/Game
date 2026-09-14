# Plan — build-foundations

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — workspace build config

Release/dev profiles, toolchain pin, `hecs`/`tracing` deps declared
and wired into `game_engine`.

### Phase 2 — CI + decisions

Check matrix, mobile compile-guard, MSRV job; ADR-001/007/008/009.

### Phase 3 — docs + gates

Docs sync per `AGENTS.md`, full local gate run, mobile target checks,
DoD evidenced.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| BLD-001 | done | Add `[profile.release]` + `[profile.dev.package."*"]` to workspace `Cargo.toml` | Functional requirements |
| BLD-002 | done | Add `rust-toolchain.toml` (stable + rustfmt/clippy) | Functional requirements |
| BLD-003 | done | Declare `hecs`/`tracing`/`tracing-subscriber`; wire `hecs`+`tracing` into `game_engine` | Functional requirements |
| BLD-004 | done | Add `.github/workflows/ci.yml` (matrix + mobile guard + MSRV) | Functional requirements |
| BLD-005 | done | Write ADR-001/007/008/009; link from decisions log | Functional requirements |
| BLD-006 | done | Sync docs (`quality`, `stack`, `architecture`, milestones, README, 0.2.0) | Goals |
| BLD-007 | done | Run local gates + mobile target checks; record evidence | Non-functional requirements |
| BLD-008 | done | Enable `winit/android-native-activity` so the Android compile-guard passes (winit `default` omits both activity impls) | Constraints & Assumptions |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Profiles + toolchain pin; builds green | done | `cargo build --workspace` Finished (see §Evidence); `rust-toolchain.toml` present |
| 2 | `hecs`/`tracing` wired; clippy/test clean | done | `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo test --workspace --all-targets` pass |
| 3 | CI present and green | done | `.github/workflows/ci.yml`; every job's commands verified locally (see §Evidence); inaugural green run pending first push — unpushed by design, no commit/push without maintainer request |
| 4 | Four ADRs written; log links them | done | `docs/decisions/ADR-001.md`, `ADR-007.md`, `ADR-008.md`, `ADR-009.md`; README checkboxes updated |
| 5 | Docs updated; links resolve | done | `quality.md`, `stack.md`, `architecture.md`, milestones, README, techstack 0.2.0 |
| 6 | Evidence recorded here | done | This table + §Evidence |

## Acceptance criteria

- `cargo build --workspace` succeeds with tuned profiles.
- No new clippy/fmt violations; no behavior change to existing crates.
- Mobile target `cargo check`s pass; MSRV documented truthfully.

## Risks & Next steps

- Risk: a locked transitive dep may need rustc > 1.85 → then document
  the true MSRV instead (per Constraints), don't downgrade.
- Hit during implementation (now BLD-008): the Android guard failed on
  `android-activity` (`native-activity`/`game-activity` feature
  required) — winit `default` enables neither, so the workspace now
  enables `android-native-activity` explicitly. iOS check passed
  clean. `GameActivity` revisit at M6 by ADR if frame pacing needs it.
- Next: M1 renderer smoke consumes these foundations (profiles,
  `tracing`, harness); allocator default validated there.

## Evidence

- Local gates (2026-09-14, Windows, rustc 1.97.1): fmt/clippy/build/
  test/doc-test/both bins — all pass (full output in session log).
- Mobile guard local: `cargo check --workspace --target
  aarch64-linux-android` + `--target aarch64-apple-ios` — pass
  (after BLD-008; iOS passed on first try).
- MSRV: `cargo +1.85 check` failed — locked `naga 30.0.1` requires
  rustc 1.87. Per Constraints, documented the true floor (1.87) in
  `stack.md`/README/CI instead of downgrading; `cargo +1.87 check`
  passes clean.
- CI: <workflow run URL after first push — all job commands verified
  locally (gates on Windows; both mobile targets; MSRV 1.87)>.
