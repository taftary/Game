# Plan — debug-sphere-viewer

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — pure `game_debug` lib core (no GPU)

DSV-001–DSV-007: manifest deps, vendored font, `mesh.rs` (fan fill +
pentagon tint + deduped line list + stats), `params.rs` (subdivision
0–8 clamp + 10·4^N+2 live hint + >6 warning; radius parse finite >0;
inline hints — invalid input never reaches `HexSphere::generate`),
`text.rs` (fontdue ASCII atlas, CPU side), `ui.rs` (layout rects per
notion wireframe; nav button, label, text field, slider, checkbox,
button — hit-test/focus/edit logic), `app.rs` + `sphere_viewer.rs`
(Screen enum, F1–F4 + click nav, state preserved across switches, sync
regenerate with timing, toggles, placeholder screens). Unit tests for
every pure behavior. Zero `game_engine` changes; all new crates already
in workspace deps (no new third-party crates, no ADR).

### Phase 2 — binary: headless + windowed

DSV-008–DSV-010: `main.rs --headless` argv path (build N=6/R=1.0 via
lib, print `cells corners pentagons hash8 gen_ms`, exit 0, GPU-free);
windowed path (tracing install, winit 0.30 `ApplicationHandler`,
vulkano boot mirroring `game_tools`, D16 depth attachment, swapchain
recreation); three viewer-authored GLSL pipelines naga-compiled via
engine API (fill w/ pentagon-highlight flag, `LineList` wireframe —
avoids the `fill_mode_non_solid` device feature the 1.1 floor doesn't
request — UI textured quads, alpha blend, ortho); frame loop + input
routing (orbit drag/zoom ↔ widgets by hit region, F1–F4).

### Phase 3 — font asset, gates + docs

DSV-002/DSV-011–DSV-013: OFL font (Inter or DejaVu Sans) + LICENSE in
`assets/fonts/`, embedded via `include_bytes!`; `quality.md` + `ci.yml`
gate change to `cargo run -p game_debug -- --headless` (current bare-run
gate would hang on the windowed binary — mirrors the M1 tools pattern);
docs sync; full gates + mobile compile-guard; DoD evidence recorded.

Plan-time answers to notion Open questions (decided with requester
2026-09-14): regeneration is **synchronous** (N≤6 stays <1 s per budget;
N=7–8 freeze guarded by warning styling); **no primal-triangle toggle**
(dual-cell is the inspection target; primal topology isn't publicly
exposed); M1's `OrbitCamera` mapping is reused for consistency.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| DSV-001 | done | `crates/debug/Cargo.toml`: add workspace deps (vulkano, winit, glam, fontdue, tracing, tracing-subscriber) | Constraints & Assumptions |
| DSV-002 | done | Vendor OFL TTF + LICENSE → `assets/fonts/`; embed via `include_bytes!`; update fonts README | Non-functional requirements |
| DSV-003 | done | `mesh.rs`: fan fill (PlanetVertex, pentagon tint = mirrors engine tint rule), deduped ring-edge line list, stats; count/tint/dedup tests (N=0..=2 against formulas) | Goals, Functional requirements |
| DSV-004 | done | `params.rs`: subdivision/radius validation + cost hint + >6 warning; tests (reject `0`/`-1`/`NaN`/`inf`/`abc`) | Functional requirements |
| DSV-005 | done | `text.rs`: fontdue glyph atlas (CPU bitmap + UV map); tests | Non-functional requirements |
| DSV-006 | done | `ui.rs`: layout rects per notion wireframe + widget logic (hit-test, focus, edit, slider drag); tests | Functional requirements |
| DSV-007 | done | `app.rs` + `sphere_viewer.rs`: Screen nav (F1–F4/click), state preserved across switches, sync regenerate + stats refresh, placeholder screens; tests | Goals, Functional requirements |
| DSV-008 | done | `main.rs --headless`: hand-rolled argv, GPU-free N=6/R=1.0 build via lib, stats line, exit 0 | Non-functional requirements |
| DSV-009 | done | `main.rs` windowed: tracing install, winit 0.30 `ApplicationHandler`, vulkano boot mirroring `game_tools`, D16 depth attachment, swapchain recreation | Functional requirements |
| DSV-010 | done | Viewer GLSL (fill w/ highlight flag, line, UI quads) + 3 pipelines + frame loop + input routing (orbit ↔ widgets, F1–F4) | Functional requirements, Goals |
| DSV-011 | done | Gates: `quality.md` + `ci.yml` debug-gate line → `cargo run -p game_debug -- --headless` | Non-functional requirements |
| DSV-012 | done | Docs sync: `architecture.md` debug row, `rendering.md` viewer subsection, `assets.md` font line, techstack README 0.4.0; links resolve | Definition of Done |
| DSV-013 | done | Full gates + mobile compile-guard + local windowed run; DoD evidence recorded below | Non-functional requirements, Definition of Done |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Sphere Viewer renders the current `HexSphere` in `game_debug` with an orbit camera; wireframe + pentagon highlight toggles functional | done | Windowed run on Intel UHD 620 (logs + screenshots below): shaded dual-cell sphere, orbit drag/zoom via reused `OrbitCamera`, wireframe `LineList` overlay, pentagon tint via fill push-constant flag; `viewer_shaders_compile` covers all 6 viewer GLSL sources |
| 2 | Inputs panel edits subdivisions (clamped 0–8, live cell-count hint) and radius (validated `> 0`, finite); invalid input shows an inline hint and never panics | done | `params.rs` validation (19 tests: `0`/`-1`/`NaN`/`inf`/`abc` rejected with hints, 0–8 accepted); screenshot shows `→ 40,962 cells` hint; `regenerate_rejects_without_touching_mesh` proves invalid input never reaches `generate` |
| 3 | Regenerate rebuilds the mesh; stats (cells, corners, pentagons = 12, short hash, gen ms) refresh | done | `regenerate_applies_valid_edits` (N=4 → 2,562 cells) + `stats_hash_changes_with_params`; headless prints `cells=40962 corners=81920 pentagons=12 ... hash8=9f087a31`; screenshot STATS block matches |
| 4 | Nav menu switches between Sphere Viewer and the FPS/Console/Inspector placeholder screens; viewer state survives switching | done | `switching_preserves_viewer_state`, `fkey_routing`, `nav_order_matches_fkeys`, `only_viewer_is_functional` tests; `placeholder_ui_centers_title_and_body` test |
| 5 | `engine::hexsphere` and the release `game` binary untouched; both binaries run | done | `git status` shows no new engine/game modifications from this feature (engine entries pre-date it from M1; `crates/game/` untouched); `cargo run --bin game` + `cargo run -p game_debug -- --headless` exit 0 |
| 6 | `docs/techstack/architecture.md` debug-crate row updated, techstack version bumped; all `quality.md` gates green with evidence recorded in `plan.md` DoD verification | done | architecture.md debug rows, rendering.md viewer section, assets.md font line, milestones next-step line, techstack 0.4.0, `quality.md` + CI gate → `--headless`; this table + §Evidence |

## Acceptance criteria

- `cargo run -p game_debug -- --headless` exits 0 GPU-free and prints
  a stats line whose hash prefix matches the committed N=6, R=1.0
  engine mesh hash.
- Windowed run opens the Sphere Viewer: orbit drag + wheel zoom, live
  wireframe and pentagon-highlight toggles, subdivision/radius edit +
  Regenerate + stats refresh, F1–F4/click nav with viewer state intact,
  placeholder screens render centered title + "not implemented yet".
- Zero diff in `crates/engine` and `crates/game`; all `quality.md`
  gates green incl. mobile compile-guard; no new clippy/fmt violations.

## Risks & Next steps

- Risk: mini-UI scope creep (first custom-UI code in the workspace) →
  widgets strictly limited to the notion wireframe; no theming/styling
  system.
- Risk: N=7–8 sync regeneration freezes the dev tool for seconds
  (~655k cells at N=8) → accepted; warning styling is the guard. Escape
  hatch: a background-thread regeneration update if measured painful.
- Risk: mobile compile-guard breaks on new debug deps → vulkano/winit
  already proven via `game_tools`; `fontdue` is pure Rust. Guard runs in
  DSV-013.
- Next: M2 descent slice builds on the viewer loop; async regen and any
  extra viewer toggles land as `update-*/` folders per `plans/README.md`.

## Evidence

Local gates (2026-09-14, Windows, stable rustc 1.97.1):

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean; `cargo build --workspace`
  Finished; `cargo test --workspace --all-targets` — 80/80 pass
  (45 `game_debug` lib + 7 `game_debug` bin + 28 `game_engine`, all
  other targets empty); `cargo test --doc --workspace` pass (1/1
  engine doc test); `cargo run --bin game` + `cargo run -p game_debug
  -- --headless` exit 0.
- Headless viewer (dev profile, informational timings):
  `subdiv=6 radius=1 cells=40962 corners=81920 pentagons=12
  tris=245760 hash8=9f087a31 gen_ms=1163.3` — `hash8` is the 8-hex
  prefix of the engine committed hash `11459543604394007386`
  (`9F087A3164011B5A`), so the viewer mesh is bit-identical to the
  pinned `HexSphere`.
- Windowed viewer on Intel(R) UHD Graphics 620 (IntegratedGpu, api
  1.3.215 — device capability; instance capped at 1.1 per policy):
  booted, mesh generated with matching `hash8=9f087a31`, event loop
  ran 20 s clean (frames presenting, no panic/validation error),
  killed via timeout; screenshots confirm the notion wireframe (nav
  bar, 3D sphere + wireframe, INPUTS panel with hint, STATS block).
- Mobile guard: `cargo check --workspace --target
  aarch64-linux-android` + `--target aarch64-apple-ios` pass (targets
  pre-installed; new debug deps — vulkano/winit proven via tools,
  fontdue pure Rust — compile clean).
- Implementation notes: naga 30 rejects combined `sampler2D` globals
  (`NotImplemented("variable qualifier")`) — the UI fragment shader
  uses separate `texture2D` + `sampler` with explicit `set`/`binding`
  (naga's own test-suite pattern); the UI pipeline needs
  `Some(DepthStencilState::default())` (test off) because the subpass
  owns a depth attachment (VUID-06590, caught by validation layers);
  UI vertices upload per-frame via `Buffer::from_iter` (a persistent
  mapped buffer hits `AccessConflict(DeviceRead)` while the prior
  frame is in flight); `fontdue` returns empty bitmaps for spaces, so
  the atlas records them as advance-only blanks instead of `?`
  fallbacks (caught on screenshot).
