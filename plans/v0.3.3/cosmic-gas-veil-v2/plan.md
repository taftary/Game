# Plan — cosmic-gas-veil-v2

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only (`cosmic_web.rs` sprites,
`main.rs` 3D texture + march pass + composite, `cosmic_capture.rs`
mode tag in shot names). Reads `WebField.grid` — legal direction.
New GPU resources: one 3D image, one quarter-res HDR target, one
fullscreen pass, one pipeline — recorded in `quality.md`. The march
target enters the `bloom-mip-chain` pass description so the write-once
pin stays the single authority. The density ramp becomes a shared
GLSL snippet across splat / sprite / march shaders. **Invariant
rows:** write-once (pinned); ray reconstruction from the same
view-proj as draws (pinned); fragment loops texture + FMA only;
`engine` untouched.

### Phase 1 — Shared ramp + sprites (`main.rs`, `cosmic_web.rs`)

Extract `COSMIC_DENSITY_RAMP_GLSL` from `SPLAT_VERT`; `veil_sprites`
(cell centre + hash jitter, class diameters/tints, alpha clamp); tests
(count band, determinism, clamp, snippet identity). Wire Low: sprites
in the glow buffer (`kind 1`), old veil removed from it.

### Phase 2 — 3D texture + march pass (`main.rs`)

`R8_UNORM` 128³ upload per seed; `build_march_pipeline` (fullscreen
triangle, no blend, own quarter-res target); `MARCH_FRAG` (ray–sphere,
`N` steps, dither, ramp, fog/slab in-march, near-eye ramp); push block
< 128 B (pack: inv-VP 64 B + eye 16 + sphere 16 + params 32 = 128 — if
it does not fit, move constants to a uniform buffer, recorded).
`describe_bloom_chain` gains the march write + resolve read; resolve
shader adds `march · e`. Ray-reconstruction pin test.

### Phase 3 — Tiering + env override + parity (`main.rs`, tier plumbing)

`VeilMode::for_tier`; `GAME_DEBUG_COSMIC_VEIL`; sprites off when
marching; `cosmic_layout=` prints mode; Low vs High parity overlay
shots.

### Phase 4 — Retirement (`cosmic_web.rs`, `main.rs`, docs)

Delete smoke (`smoke_puffs`, `SmokePuff`, `SmokeVertex`, `SMOKE_*`,
pipeline, upload), `glow_point_cloud`, braid/spine/strand/fray helpers
and their tests; `rendering.md` paragraphs removed; `quality.md` rows
replaced.

### Phase 5 — Measurement + Intel check + shots + gates

Frame cost of the march on the reference iGPU + desktop; UHD 620
captures; shots ×3 ×2 modes; docs; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CGV-001 | pending | `COSMIC_DENSITY_RAMP_GLSL` shared snippet; splat shader uses it; identity pin across shaders | Goals §3, DoD 4 |
| CGV-002 | pending | `veil_sprites(field, origin)`: class diameters/tints, alpha clamp, hash jitter; tests (band `[100k, 200k]`, determinism, clamp) | Goals §1, FR1, NFR3 |
| CGV-003 | pending | Low wiring: sprites into the glow buffer (`kind 1`), old `glow_point_cloud` veil removed from uploads | Goals §1, FR5 |
| CGV-004 | pending | `R8_UNORM` 128³ upload per seed + sampler; rebase-safe origin/cell push values | FR2 |
| CGV-005 | pending | `build_march_pipeline` + quarter-res HDR target + `MARCH_FRAG` (ray–sphere, steps, dither, ramp, fog/slab, near-eye); push block fits or UBO (recorded) | Goals §2, FR3, FR7, NFR4 |
| CGV-006 | pending | `describe_bloom_chain` gains march write + resolve read; resolve adds `march·e`; write-once pin green | FR4, NFR2 |
| CGV-007 | pending | Ray-reconstruction pin: synthetic single-cell volume → blob within 4 px of the splat at the cell centre | NFR5 |
| CGV-008 | pending | `VeilMode::for_tier` (Sprites / March 32 / March 48) + `GAME_DEBUG_COSMIC_VEIL` + sprites-off-when-marching; layout line prints mode; tests | FR5, FR6 |
| CGV-009 | pending | Retire smoke + old veil + braid/spine/strand/fray helpers + tests; `rg` = 0 check | Goals §4, FR6, DoD 3 |
| CGV-010 | pending | March frame cost on reference iGPU + desktop (32/48 steps, 1080p); Low sprite count/MB; record vs `quality.md` | NFR1, DoD 5 |
| CGV-011 | pending | Intel UHD 620 High captures `slab` + `demo` clean; capture determinism gate both modes | NFR2, NFR3, DoD 5 |
| CGV-012 | pending | Shots ×3 presets ×2 modes; Low/High parity overlay at `slab`; near-eye wash check at `demo` | DoD 1–2, NFR6 |
| CGV-013 | pending | Docs: `rendering.md` veil section, `quality.md` row (replaces cue-raymarch row), techstack version bump, `architecture.md` note; link check | DoD 6 |
| CGV-014 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-only; one new
pass/pipeline/3D image recorded in budgets; march target governed by
the same pass-description pin as bloom; ramp snippet single source;
engine untouched) · Todos approved by: TECHLEAD (2026-09-20 —
risk-first: shared ramp + Low sprites first (Low must ship regardless
of the march); ray pin (CGV-007) before any grading; cost measured
(CGV-010) with recorded cut order: sheet sprites → alpha floor → count
stride on Low; steps 48 → 32 → 24 on Medium/High) · UX acceptance rows:
approved 2026-09-20 — UX-1…UX-3 below · DoD verified by: ANALYST
_(pending)_ · Security reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | High march bodies + walls; Low same structure grainier; voids dark | pending | shots + parity overlay | ANALYST _(pending)_ |
| 2 | Demo inside filament: translucent, no wash, hub visible (both modes) | pending | shots | ANALYST _(pending)_ |
| 3 | Smoke/old veil/braid code gone; layout prints mode | pending | `rg` output + log | ANALYST _(pending)_ |
| 4 | Tests listed green (sprites, snippet pin, ray pin, pass pin, tier, env) | pending | test names | ANALYST _(pending)_ |
| 5 | UHD 620 clean; costs recorded | pending | shot paths + numbers | ANALYST _(pending)_ |
| 6 | Docs + links | pending | file list | ANALYST _(pending)_ |
| 7 | Gates + audit + review + one commit | pending | gate log, commit | ANALYST + SECURITY _(pending)_ |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. March target written once by the march pass, read only by the
  resolve [T: pass-description pin].
- A-2. March rays reconstructed from the inverse of the **same**
  view-proj pushed to the draws; NDC top row [T: CGV-007].
- A-3. Fragment loops: texture fetch + `mix`/FMA only; no `exp`/`pow`
  [T: shader pin].
- A-4. One density ramp snippet across splat / sprite / march [T: pin].
- A-5. Low never builds the march pipeline or the 3D image [T:
  `VeilMode::for_tier(Low) == Sprites` + resource count assert in
  headless].
- A-6. No engine / `game` / `tools` diff; `engine::render::cue` CPU
  reference untouched.

UX acceptance rows:

- UX-1. Demo, inside a filament, cruise speed: gas body visible around
  the ship, thickening ahead toward the hub, never a full-screen wash
  (no frame > 50 % pixels above the bloom threshold).
- UX-2. Low and High at the same pose show the same filaments (parity
  overlay) — a phone player and a desktop player see the same web.
- UX-3. Inspector slab (High): walls faintly visible between filaments
  outlining polygonal voids (report §5) without beads on them.

## Risks & Next steps

- R-1 (march cost on Medium iGPU): 32 steps × quarter-res may exceed
  2 ms on the UHD 620; cut order recorded (steps 32 → 24, then
  eighth-res); Low unaffected by construction.
- R-2 (push block overflow): inverse VP + eye + sphere + params can
  exceed 128 B; fall back to a small uniform buffer (recorded; no
  invariant touched).
- R-3 (banding): 32 steps over a 500 Mpc chord = 15 Mpc/step, above
  the 4 Mpc cell; dither + linear 3D filtering hide most of it; if
  shots band, raise steps first, then add a second dither dimension.
- R-4 (sprites lattice imprint on Low): hash jitter of ≤ 0.5 cell may
  still show the grid in sheets; first mitigation is diameter 2.0 →
  2.4 for sheet cells (constant).
- Next: `cosmic-vista-intro` drives the slab/fog terms the march
  already consumes; LOD/culling hardening from measured costs stays a
  post-v0.3.3 roadmap item.
