# Plan — cosmic-depth-window

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `map_camera.rs` (per-instance
FOV), `cosmic_web.rs` (inspector slab state + controls), `main.rs`
(shared fog/slab GLSL snippet, push blocks, key handling, dev-widget
slider), `cosmic_capture.rs` (fill `slab` preset). **Touches the
camera/projection contract** (`rendering.md` § Camera & screen-space
conventions) → invariant rows: un-flipped projection at any FOV;
picking and drawing share one matrix; `px_scale`/`pixels_per_unit`
derive from the instance FOV; other `MapOrbitCamera` consumers
unchanged. No engine change; `FOV_Y` stays the default.

### Phase 1 — Shared GLSL window snippet + push blocks (`main.rs`)

`COSMIC_WINDOW_GLSL` const (`fog_l`, `slab_center`, `slab_half` →
`vis`), concatenated into `GLOW_VERT`, `SMOKE_VERT`, `SPLAT_VERT` (and
later veil). Push blocks gain the three floats (`GlowPush` 76 → 88 B,
`SmokePush` 92 → 104 B, splat push accordingly — all < 128 B, pinned by
`cosmic_push_constants_fit_vulkan_floor`). Hub-impostor fog floor via
`kind`. Tests: snippet identical across shaders (pin), `vis` monotone,
fully fogged = 0.

### Phase 2 — Per-instance FOV (`map_camera.rs`)

`fov_y: f32` field (default `FOV_Y`), `set_fov_keep_framing(fov)`
adjusting distance by `tan(old/2)/tan(new/2)`; `projection_matrix`,
`px_scale`, `pixels_per_unit`, `project_to_screen` read `self.fov_y`.
Tests: framed width constant ±1 %; picking round-trip at 20° and 60°;
existing map-camera tests unchanged and green.

### Phase 3 — Inspector slab mode + controls (`cosmic_web.rs`, `main.rs`)

`SlabState { on, thickness, center }` on `CosmicWebInspector`; `S`,
`Shift+wheel`, `[`/`]`; readout in the dock; `cosmic_frame()` fills the
push constants per surface (`fog_l` demo 90 / map off-unless-slab).
Controls list + `controls.md`.

### Phase 4 — Demo fog + dev slider (`main.rs`, `app.rs`)

`COSMIC_DEMO_FOG_MPC = 90.0`; `F7` Inspector sub-tab slider `[30, 400]`;
Console log line on change.

### Phase 5 — `slab` preset + shots + overdraw relief + gates

Fill `CapturePreset::SLAB` (slab on, `T = 30`, `D` via home node, 20°,
≈ 400 Mpc framed); shots ×3; rerun the splat overdraw estimate at the
`slab` preset (before/after); docs; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CDW-001 | done | `COSMIC_WINDOW_GLSL` snippet + inclusion in every cosmic vertex shader; push blocks extended; `cosmic_push_constants_fit_vulkan_floor` green | Goals §1–2, FR1, FR2, NFR1 |
| CDW-002 | done | Tests: snippet identical across shaders (pin); `vis` monotone in depth; fully fogged vertex contributes 0; hub fog floor 0.25 present | FR1, FR7, DoD 4 |
| CDW-003 | done | `MapOrbitCamera.fov_y` + `set_fov_keep_framing`; all derived quantities read it; framed-width and picking round-trip tests at 20°/60°; existing camera tests untouched | Goals §3, FR4, NFR2, NFR4 |
| CDW-004 | done | Inspector `SlabState` + `S` / `Shift+wheel` / `[` `]` handling + dock readout; `cosmic_frame()` fills window push constants per surface | Goals §2, FR3 |
| CDW-005 | done | Demo fog default `90 Mpc` + `F8` slider `[30, 400]` + Console log | Goals §5, FR5 |
| CDW-006 | done | Controls list (Settings) + `controls.md`; assert no existing key rebound | FR3, DoD 3 |
| CDW-007 | done | Fill `CapturePreset::SLAB`; capture determinism gate at `slab` preset | Goals §4, FR6, NFR3 |
| CDW-008 | done | Shots `slab-after.png`, `inspector-after.png`, `demo-after.png`; void-interior luminance check (≤ 3× backdrop, ≥ 6 voids) | DoD 1–2 |
| CDW-009 | done | Splat overdraw estimate at `slab` preset before/after fog+slab; record relief factor | NFR5, DoD 5 |
| CDW-010 | done | Docs: `rendering.md` camera contract (per-instance FOV) + depth-window paragraph, `quality.md` note, techstack version bump; link check | DoD 6 |
| CDW-011 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Measurements (2026-09-20, Intel UHD 620, dev profile)

- `slab-after.png` (filled `slab` preset: 20°, T = 30 at home depth):
  dark polygonal voids across the frame (10+ distinct, interiors at
  backdrop), thin threads with beads, flat perspective — the
  target's §2/§6 reading. SHA `42224B1B…` across two runs
  (determinism gate green).
- `inspector-after.png` (60°, window off) is SHA-identical to the
  splat shot (`2F46F74E…`): window off = identity, framing unchanged.
- `demo-after.png`: far field fades to indigo, local body crisp and
  translucent, hub glows legible (UX-1).
- Slab relief: `slab_relief=keep30495 total255900 frac0.12 ok` — 12 %
  of Low splats survive the window, 8.3× fill relief (NFR5 ≥ 5× ✓).
- Push blocks: Glow 88 B, Smoke 104 B, Splat 112 B (pin green).

## Implementation notes vs plan

- The seam landed as `record_cosmic_hdr_prepass` +
  `record_cosmic_view_arm` (CAP), so window terms ride
  `CosmicFrame.fog_l/slab_center/slab_half` — no seam signature
  growth; capture fills the same fields (slab preset included).
- Snippet sharing is paste + `cosmic_window_snippet_shared` pin, not
  `concat!` composition (`concat!` cannot take consts) — the pin
  (byte-identity against the lib authority) is the guarantee the DoD
  checks.
- `SlabState::scroll` clamps absolute view depth; the windowed roam
  passes radius 2600 Mpc (full sphere depth) — recorded, not
  target-relative.
- Dev slider lives in the `F8` Inspector widget tab, not `F7`
  (F7 is Console; the notion's tab label was wrong) — readout +
  30–400 track + knob, Console-logged on release. Fog length itself
  lives on `CosmicDemoState.fog_l_mpc` (reseed resets to 90).
- `INSPECTOR_MAX_DISTANCE_MPC` 800 → 1600: the 20° rescale (×3.27)
  must not clamp at the default framing (430 → 1407).
- Slab keys are tab-gated (`S` is demo thrust, `Shift+wheel` is
  cruise pace on GameDemo); registry gains SlabToggle/Thinner/
  Thicker (34 static, 50 total).

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — camera contract touched
deliberately and pinned: one matrix for draw + pick at any FOV; the
snippet is a single source of truth for the window term; no engine
change) · Todos approved by: TECHLEAD (2026-09-20 — risk-first: the
FOV change (CDW-003) carries the invariant risk and has its own pins
before any UI; overdraw relief is measured, not assumed) · UX
acceptance rows: approved 2026-09-20 — UX-1…UX-3 below · DoD verified
by: ANALYST (2026-09-20 — shots judged: 10+ dark voids, demo goal
legible, controls listed; window-off identity by hash) · Security reviewed
by: SECURITY (2026-09-20 — no I/O, no new dependency; keys tab-gated,
no rebinding; shader arithmetic-only).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `slab-after.png` ≥ 6 dark voids ≤ 3× backdrop; inspector framing unchanged | done | 10+ voids at backdrop; `inspector-after.png` SHA = CTS shot | ANALYST 2026-09-20 |
| 2 | Demo far field fades; home hub stays visible | done | `demo-after.png` + hub glows legible (fog floor) | ANALYST 2026-09-20 |
| 3 | Controls work + listed; no rebinding | done | S/wheel/brackets + dock readout + `controls.md` + 3 registry actions; tab-gated | ANALYST 2026-09-20 |
| 4 | Tests listed green | done | snippet pin, fog/slab mirrors, FOV width + 20°/60° pick round-trip, existing camera tests | ANALYST 2026-09-20 |
| 5 | Overdraw relief recorded | done | `slab_relief` frac 0.12 → 8.3× | ANALYST 2026-09-20 |
| 6 | Docs + links | done | `rendering.md` contract + window paragraph, `controls.md`, techstack 0.42.0 | ANALYST 2026-09-20 |
| 7 | Gates + audit + review + one commit | done | gate log, commit | ANALYST + SECURITY 2026-09-20 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `projection_matrix` remains glam `directx::perspective` (RH,
  Z ∈ [0,1], no Y-flip) for every `fov_y` [T].
- A-2. Inspector picking uses the same matrix as the draw; round-trip
  `project → pick` within `COSMIC_PICK_RADIUS_PX` at 20° and 60° [T].
- A-3. Galaxy/System/Planet map cameras: default `fov_y = FOV_Y`,
  existing tests unchanged [T].
- A-4. One `COSMIC_WINDOW_GLSL` snippet, byte-identical in every cosmic
  vertex shader [T: pin].
- A-5. Fully fogged or out-of-slab vertex contributes exactly 0 to the
  additive chain (no floor leak) [T].
- A-6. Marker via `world_to_pixels`; bloom write-once; no engine diff.

UX acceptance rows:

- UX-1. Demo: from spawn, the home hub impostor is visible at any
  distance inside the sphere (fog floor), while filaments beyond
  ≈ 2 fog lengths fade to backdrop — goal legible in 3 s.
- UX-2. Inspector: `S` toggles a clearly different picture (dark voids,
  flat perspective); readout shows thickness + depth; `Shift+wheel`
  scrolls through the volume smoothly (no jump > `T/4` per notch).
- UX-3. Toggling slab off restores the exact previous framing (no
  zoom drift).

## Risks & Next steps

- R-1 (FOV coupling): other code may read `FOV_Y` directly for the
  inspector (e.g. `px_scale` in `cosmic_frame()`); grep + replace with
  the instance getter, pinned by the 20° picking test.
- R-2 (fog hides the goal): UX floor 0.25 for hubs; if still too dim at
  250 Mpc, raise the floor (constant) — never disable fog for hubs
  entirely (they would float over an empty field).
- R-3 (slab edge artifacts): a hard slab cut shows sprites half-in;
  5 Mpc smoothstep edge handles it; widen to 10 if visible in shots.
- Next: `cosmic-hub-hierarchy` (fewer, bigger hubs now that the field
  is windowed), `cosmic-vista-intro` (drives slab/fog over time).
