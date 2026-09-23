# Plan — cosmic-void-contrast

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_veil.rs` (snippet +
CPU mirror + constants), `main.rs` (`SPLAT_VERT` v2, sprite path,
`MARCH_FRAG`, HDR clear colour). No new resource. **Invariant rows:**
fragment arithmetic-only; write-once pin untouched; identity pin
across three shaders is the single source (same mechanism as the
ramp).

### Phase 1 — Snippet + mirror (`cosmic_veil.rs`)

`COSMIC_TRANSFER_GLSL`, constants, `transfer()` CPU mirror; band
tests; identity pin scaffold.

### Phase 2 — Wiring (`main.rs`)

Splat vertex weight, sprite weight, march accumulation, rim in all
three, clear colour; `emissive_scale` retired; safety pins extended.

### Phase 3 — Grading + scans

Shots ×3 presets; void / ridge / sheet / limb scans; constants graded
and recorded; UX rows.

### Phase 4 — Docs + gates

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CVC-001 | pending | `COSMIC_TRANSFER_GLSL` (floor, band curve, rim; `smoothstep`/`mix`/FMA only) + constants + CPU `transfer()` mirror; band tests (below floor → 0, rim → 0 at `R`, monotone in band) | FR1, FR2, FR3 |
| CVC-002 | pending | Paste into `SPLAT_VERT` v2 / sprite path / `MARCH_FRAG`; identity pin ×3; `emissive_scale` retired | FR1, FR5, FR6 |
| CVC-003 | pending | HDR clear = deep indigo constant; resolve adds no backdrop term; pin | FR4 |
| CVC-004 | pending | Rim weight on every cosmic emission; `inspector` radial limb scan (no > 20 % step within 10 px) | FR6, DoD 2 |
| CVC-005 | pending | Grade constants from `slab`: void ≤ 1.15× backdrop, ridge/floor ≥ 6, sheet/floor ≥ 1.5; record numbers | Goals §3–4, DoD 1 |
| CVC-006 | pending | `demo` inside a filament: hot fraction ≤ 50 %, dark cells around the ship; UX-1/UX-2 recorded | DoD 3 |
| CVC-007 | pending | Shader safety pins extended (transfer literals; no `exp`/`pow`) | NFR2, DoD 4 |
| CVC-008 | pending | Docs: `rendering.md` grading paragraph, `quality.md` constants row, techstack bump; link check | DoD 5 |
| CVC-009 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — shader-only; one
snippet, three consumers, one pin; no resource or pass change) ·
Todos approved by: TECHLEAD (2026-09-23 — mirror + bands before
wiring; grading is a todo with numeric targets, not a loop) · UX
acceptance rows: UX-1, UX-2 below · DoD verified by: ANALYST
_(pending)_ · Security reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `slab` scans: void ≤ 1.15×, ridge ≥ 6×, sheet ≥ 1.5× | pending | | |
| 2 | `inspector` no limb step | pending | | |
| 3 | `demo` hot fraction ≤ 50 %, dark cells | pending | | |
| 4 | Tests + pins | pending | | |
| 5 | Docs + links | pending | | |
| 6 | Gates + audit + review + one commit | pending | | |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Fragment shaders stay `fract`-hash / arithmetic only [T: pin].
- A-2. No new attachment / pass; write-once pin unchanged [T].
- A-3. One transfer snippet across splat / sprite / march [T: identity
  pin ×3].

UX acceptance rows:

- UX-1. At `slab`, voids read as dark cells bounded by threads and
  faint walls — "soap-bubble compartments" (report §6).
- UX-2. Inside a filament in the demo, the surroundings are a
  translucent thread with visible dark space beyond it, never a wash.

## Risks & Next steps

- R-1 (auto-exposure chase): a darker field may pull exposure up and
  re-brighten voids; clamp the cosmic exposure range (one constant,
  recorded).
- R-2 (walls vanish): a zero floor can kill the faint sheets; the
  `sheet / floor ≥ 1.5` target guards it — if it fails, raise the
  sheet class weight in the veil, not the floor.
- Next: `cosmic-hub-compact-cores` sets hub inputs on top of this
  grading; `cosmic-vista-reframe` takes the headline shot.
