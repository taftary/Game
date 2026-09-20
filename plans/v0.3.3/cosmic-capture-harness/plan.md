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
| CAP-001 | pending | Extract `record_cosmic_frame(target, extent, hdr, frame)` seam; windowed loop calls it; existing shader/vertex pins green | NFR1 |
| CAP-002 | pending | `cosmic_capture.rs`: `CapturePreset` + four presets (poses in Mpc relative to home node) + GPU-free tests (lookup, parse, filename) | Goals §2, FR2 |
| CAP-003 | pending | CLI: `--capture <path>`, `--view`, `--size WxH`; usage string; `parse_args` tests incl. bad preset / bad size | Goals §1, FR5 |
| CAP-004 | pending | Offscreen boot (no window) + color image in pipeline format + record via CAP-001 + `copy_image_to_buffer` | FR1, NFR1 |
| CAP-005 | pending | Add `png` (workspace root, `game_debug` only, encode) + write 8-bit sRGB PNG; row-0-is-top pin via known player-point capture | FR1, NFR2, NFR5 |
| CAP-006 | pending | Exit codes: no device → 2, unwritable path → 3, no HDR → LDR bypass + log | FR5 |
| CAP-007 | pending | `F12` windowed capture → `captures/<surface>-<seed>-<ts>.png`, Console log, one-frame deferred encode; Controls list + `controls.md` | Goals §4, FR4 |
| CAP-008 | pending | Determinism gate: two captures byte-identical (script below); record result | FR3, DoD 3 |
| CAP-009 | pending | `--headless` still GPU-free (module split verified by `cargo run -p game_debug -- --headless` on a runner without Vulkan or with `VK_ICD_FILENAMES=` empty) | NFR4 |
| CAP-010 | pending | Baseline shots of the v0.3.2 build ×4 presets under `shots/` (`<preset>-before.png`) | DoD 7 |
| CAP-011 | pending | Docs: `quality.md` local-gate row, `plans/README.md` shots rule (PNG ≤ 1 MB, naming), `rendering.md` capture note, techstack version bump; link check | DoD 6 |
| CAP-012 | pending | Full gate suite + SECURITY dependency review (`png` encode-only, no `unsafe` features) + ANALYST DoD audit; single `done` commit | DoD 1–7 |

Local determinism gate (CAP-008):

```text
cargo run -p game_debug -- --capture a.png --seed 1337 --view inspector
cargo run -p game_debug -- --capture b.png --seed 1337 --view inspector
fc /b a.png b.png
```

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-crate only; one
frame-recording seam shared by both targets; `png` is the sole new
dependency, declared at the workspace root, consumed by `game_debug`
only; NDC-top-row pin listed) · Todos approved by: TECHLEAD
(2026-09-20 — risk-first: the seam (CAP-001) and offscreen boot
(CAP-004) are the unknowns and go first; presets are data so every
later feature can add a pose without code; gate commands listed) · UX
acceptance rows (if player-facing): approved 2026-09-20 — UX-1 below
(`F12` only) · DoD verified by: ANALYST _(pending)_ · Security reviewed
by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `--capture` writes a PNG, top row = top, exit 0 on both reference GPUs | pending | shot path + exit code log | ANALYST _(pending)_ |
| 2 | Four presets render; `slab`/`vista` reserved | pending | four shot paths + preset doc comment | ANALYST _(pending)_ |
| 3 | Two identical captures byte-identical | pending | `fc /b` output | ANALYST _(pending)_ |
| 4 | `F12` writes PNG + logs; Controls list shows it; no key rebound | pending | Console log line + Controls screenshot | ANALYST _(pending)_ |
| 5 | `--headless` GPU-free; `game` diff empty; `png` only new dep, SECURITY signed | pending | CI run + `git diff --stat` + Cargo.lock delta | ANALYST + SECURITY _(pending)_ |
| 6 | Docs updated, links resolve | pending | file list + link check | ANALYST _(pending)_ |
| 7 | v0.3.2 baseline shots ×4 committed | pending | `shots/*-before.png` | ANALYST _(pending)_ |

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
