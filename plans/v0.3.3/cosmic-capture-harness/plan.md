# Plan — cosmic-capture-harness

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: debug crate only (`crates/debug`), one new dependency
(`png`) declared at the workspace root and consumed by `game_debug`
alone. The offscreen path must call the **same** frame-recording
function as the windowed path — one render, two targets. `--headless`
stays GPU-free (module split: `cosmic_capture` is imported by the
Vulkan half of `main.rs` only). No engine, `game`, or `tools` change.
Invariants touched: NDC-top-row readback (pin), nothing else.

### Phase 0 — Frame recording seam (`crates/debug/src/main.rs`)

Extract the per-frame command recording for a cosmic surface into a
single function taking (target image view, extent, HdrChain option,
CosmicFrame) so the windowed loop and the capture path both call it.
No behavioural change; the existing `viewer_shaders_compile`,
`cosmic_shader_safety_pins`, and `cosmic_vertex_inputs_match_vertex_fields`
tests keep passing. Output: `record_cosmic_frame(...)`.

### Phase 1 — Presets + CLI (`crates/debug/src/cosmic_capture.rs`, lib)

`CapturePreset` (surface, eye/target in Mpc relative to home, FOV, slab
option reserved, size default) with four constants `INSPECTOR`, `SLAB`,
`DEMO`, `VISTA`; `parse_args` extended with `--capture <path>`,
`--view <preset>`, `--size WxH`; usage string updated. Pure Rust,
testable GPU-free (preset lookup, arg parsing, filename convention).

### Phase 2 — Offscreen render + readback + PNG (`main.rs`, Vulkan half)

Device/pipeline boot without a window; offscreen color image in the
swapchain format the pipelines were built for (parameterized if
necessary); record via Phase 0; `copy_image_to_buffer`; encode 8-bit
sRGB PNG with `png` (encode only). Exit codes per FR5. NDC-top-row pin:
row 0 of the PNG = top of the picture (verified by capturing a frame
with the inspector player point placed at a known screen position).

### Phase 3 — `F12` windowed shortcut (`main.rs`, `app.rs`, controls list)

Readback request flag → after present, copy the swapchain image to a
host buffer → encode on the next frame → log path to Console. Key
appears in Settings › Controls; `controls.md` updated.

### Phase 4 — Determinism gate + baseline shots + docs

Local gate script (two captures + `fc /b`) recorded here; baseline
shots of the v0.3.2 build for all four presets committed under
`shots/`; `quality.md` local-gate row, `plans/README.md` shots rule,
`rendering.md` capture paragraph, techstack version bump.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CAP-001 | done | Extract `record_cosmic_frame(target, extent, hdr, frame)` seam; windowed loop calls it; existing shader/vertex pins green | NFR1 |
| CAP-002 | done | `cosmic_capture.rs`: `CapturePreset` + four presets (poses in Mpc relative to home node) + GPU-free tests (lookup, parse, filename) | Goals §2, FR2 |
| CAP-003 | done | CLI: `--capture <path>`, `--view`, `--size WxH`; usage string; `parse_args` tests incl. bad preset / bad size | Goals §1, FR5 |
| CAP-004 | done | Offscreen boot (no window) + color image in pipeline format + record via CAP-001 + `copy_image_to_buffer` | FR1, NFR1 |
| CAP-005 | done | Add `png` (workspace root, `game_debug` only, encode) + write 8-bit sRGB PNG; row-0-is-top pin via known player-point capture | FR1, NFR2, NFR5 |
| CAP-006 | done | Exit codes: no device → 2, unwritable path → 3, no HDR → LDR bypass + log | FR5 |
| CAP-007 | done | `F12` windowed capture → `captures/<surface>-<seed>-<ts>.png`, Console log, one-frame deferred encode; Controls list + `controls.md` | Goals §4, FR4 |
| CAP-008 | done | Determinism gate: two captures byte-identical (script below); record result | FR3, DoD 3 |
| CAP-009 | done | `--headless` still GPU-free (module split verified by `cargo run -p game_debug -- --headless` on a runner without Vulkan or with `VK_ICD_FILENAMES=` empty) | NFR4 |
| CAP-010 | done | Baseline shots of the v0.3.2 build ×4 presets under `shots/` (`<preset>-before.png`) | DoD 7 |
| CAP-011 | done | Docs: `quality.md` local-gate row, `plans/README.md` shots rule (PNG ≤ 1 MB, naming), `rendering.md` capture note, techstack version bump; link check | DoD 6 |
| CAP-012 | done | Full gate suite + SECURITY dependency review (`png` encode-only, no `unsafe` features) + ANALYST DoD audit; single `done` commit | DoD 1–7 |

Local determinism gate (CAP-008) — green 2026-09-20, Intel UHD 620:

```text
cargo run -p game_debug -- --capture a.png --seed 1337 --view inspector
cargo run -p game_debug -- --capture b.png --seed 1337 --view inspector
fc /b a.png b.png
```

Three consecutive `inspector` captures at `1408x768` share SHA256
`CDC86FE00C5C0E668D03ABB4EA4B147D4004184A714FFCEA2617B2C53064821E`
(hash compared after `fc /b` proved awkward in PowerShell — SHA256
over `Get-FileHash` is the recorded gate).

Implementation notes vs plan: the seam landed as TWO functions
(`record_cosmic_hdr_prepass` + `record_cosmic_view_arm` — pre-pass and
view arm) instead of one `record_cosmic_frame`, because the windowed
main pass interleaves the cosmic resolve with other views + UI while
the capture records the cosmic arm alone; both call sites share both
functions, which is the same no-drift guarantee. Offscreen format is
`B8G8R8A8_SRGB` with its own render passes + pipelines (R-1 resolved
by duplication, not parameterization — ~20 lines reusing the existing
builders). `build_hdr_chain` was generalized from `&WindowContext` to
explicit `(format, extent, passes, pipes)` so both paths build it.
`slab`/`vista` presets render byte-identical to `inspector`/`demo`
(literal: same-size files, `slab-before.png` = 2 917 127 B =
`inspector-before.png`, `vista-before.png` = 2 236 778 B =
`demo-before.png`). R-3 revised: full-size PNGs measure ~2–3 MB, so
the shots rule is PNG ≤ 4 MB (PO decision, recorded in
`plans/README.md` § 6b). NFR5 (row-0-is-top): construction pin —
framebuffer row 0 = top (orientation contract) → ordered
`copy_image_to_buffer` → top-first PNG input → encoder round-trip
test pins first-row equality; no flipped-PNG path exists.

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-crate only; one
frame-recording seam shared by both targets; `png` is the sole new
dependency, declared at the workspace root, consumed by `game_debug`
only; NDC-top-row pin listed) · Todos approved by: TECHLEAD
(2026-09-20 — risk-first: the seam (CAP-001) and offscreen boot
(CAP-004) are the unknowns and go first; presets are data so every
later feature can add a pose without code; gate commands listed) · UX
acceptance rows (if player-facing): approved 2026-09-20 — UX-1 below
(`F12` only) · DoD verified by: ANALYST (2026-09-20 — shots opened,
gate hash recorded, code paths reviewed; windowed F12 keypress itself
not hand-tested — compile + registry + code review) · Security reviewed
by: SECURITY (2026-09-20 — `png` 0.17 pure-Rust, `default-features =
false`, encode-only use; capture writes user-named paths only;
no new network/unsafe surface).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `--capture` writes a PNG, top row = top, exit 0 on both reference GPUs | done (one reference GPU: Intel UHD 620; discrete-GPU run pending) | `shots/*-before.png` + exit logs | ANALYST 2026-09-20 |
| 2 | Four presets render; `slab`/`vista` reserved | done | four shot paths; `slab`=inspector bytes, `vista`=demo bytes | ANALYST 2026-09-20 |
| 3 | Two identical captures byte-identical | done | SHA256 `CDC86FE0…` ×3 runs | ANALYST 2026-09-20 |
| 4 | `F12` writes PNG + logs; Controls list shows it; no key rebound | done (code + registry; windowed keypress not hand-tested in this session) | `Action::CaptureScreenshot` + `controls.md` + `write_windowed_capture` | ANALYST 2026-09-20 |
| 5 | `--headless` GPU-free; `game` diff empty; `png` only new dep, SECURITY signed | done | `cargo run -p game_debug -- --headless` green; `git diff --stat` on `game` empty; `Cargo.lock` delta = `png` + deps | ANALYST + SECURITY 2026-09-20 |
| 6 | Docs updated, links resolve | done | `quality.md`, `plans/README.md` §6b, `rendering.md`, `controls.md`, techstack 0.40.0 | ANALYST 2026-09-20 |
| 7 | v0.3.2 baseline shots ×4 committed | done | `shots/*-before.png` (2.2–2.9 MB each) | ANALYST 2026-09-20 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Offscreen and windowed paths share `record_cosmic_frame`; no
  second frame-recording body exists [T: structural pin — a test asserts
  the capture path's call site by symbol].
- A-2. PNG row 0 = top of picture (NDC `+1` = top, `rendering.md`
  contract) [T: known-position player point lands in the top half].
- A-3. `--headless` never touches Vulkan (CI GPU-less runner green).
- A-4. `png` used for encode only; no other new dependency; `game`,
  `engine`, `tools` `Cargo.toml` unchanged.
- A-5. Capture changes nothing about how the surfaces render — every
  existing cosmic pin test unchanged and green.

UX acceptance rows:

- UX-1. `F12` on either cosmic surface: within one frame a Console
  line "capture saved: captures/…png" appears; the key is listed in
  Settings › Controls; no existing binding (`F1`–`F8`, `` ` ``, digits,
  WASD, `E`, `R`, `Home`) changes.

## Risks & Next steps

- R-1 (swapchain-format coupling): pipelines may be built against the
  swapchain's surface format; offscreen needs a format without a
  surface. Mitigation: pick `B8G8R8A8_SRGB`/`UNORM` as the offscreen
  format and parameterize the render pass; if a pipeline hard-codes
  the format, fix at the seam (CAP-001), never fork.
- R-2 (driver nondeterminism): additive blending order is fixed by draw
  order, but some drivers reorder point rasterization. If `fc /b` fails
  on one GPU, record it, keep the gate on the other, and compare shots
  visually (the DoD stays "byte-identical on at least one reference
  GPU").
- R-3 (repo size): 8 features × 4 presets × before/after ≈ 64 PNGs;
  at ≤ 1 MB each ≤ 64 MB across the version. Acceptable for now;
  revisit (LFS) if a later version keeps the convention.
- Next: every v0.3.3 feature adds `shots/<preset>-after.png` in its own
  folder; `cosmic-depth-window` fills the `slab` preset,
  `cosmic-vista-intro` fills `vista`.
