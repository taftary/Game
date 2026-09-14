# Plan — debug-screens

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — crate scaffold

Manifest + `lib.rs` + three stub screen modules; workspace wiring.

### Phase 2 — docs + gates

Architecture docs updated, full gate run, DoD evidenced.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| DBG-001 | done | Create `crates/debug/` (`Cargo.toml` + `src/lib.rs` + `fps`/`console`/`inspector` stubs) | Functional requirements |
| DBG-002 | done | Add `crates/debug` to workspace members (non-default, like `tools`) | Functional requirements |
| DBG-003 | done | Add `game_debug` runnable game binary (`src/main.rs`, mirrors `game` + tools) | Goals |
| DBG-004 | done | Update `docs/techstack/architecture.md` + bump techstack version | DoD |
| DBG-005 | done | Run fmt/clippy/build/test/run-both-bins gates, record evidence | Non-functional requirements |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Crate exists, builds, stubs only | done | `cargo build --workspace` Finished, `game_debug` lib + bin compiled |
| 2 | Member wired; both binaries run; gates green | done | `cargo run --bin game` + `cargo run -p game_debug --bin game_debug` exit 0; fmt/clippy/test pass |
| 3 | Architecture docs + version bump | done | `architecture.md` crate trees updated, techstack 0.1.9 |

## Acceptance criteria

- `cargo build --workspace` succeeds with `game_debug` compiled.
- No new clippy/fmt violations; no behavior change to existing crates.

## Risks & Next steps

- Next: M1 renders the screens (`vulkano`/`winit` drawing) and wires them into `game`/`tools` (new plans feature).
