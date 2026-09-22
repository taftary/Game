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
| CGV-001 | done | `COSMIC_DENSITY_RAMP_GLSL` shared snippet; splat + march shaders paste it verbatim; `cosmic_density_ramp_shared` identity pin | Goals §3, DoD 4 |
| CGV-002 | done | `veil_sprites(field, origin)`: class diameters/tints, alpha clamp, hash jitter; tests (band `[300k, 370k]` nominal seed 1234, determinism, clamp) | Goals §1, FR1, NFR3 |
| CGV-003 | done | Low wiring: sprites into the glow buffer (`kind 1`), old descriptor-glow veil removed from uploads | Goals §1, FR5 |
| CGV-004 | done | `R8_UNORM` 128³ upload per seed + sampler; rebase-safe origin/cell push values | FR2 |
| CGV-005 | done | `build_march_pipeline` + quarter-res HDR target + `MARCH_FRAG` (ray–sphere, steps, dither, ramp, fog/slab, near-eye); push block exactly 128 B (pinned) | Goals §2, FR3, FR7, NFR4 |
| CGV-006 | done | `describe_veil_chain` (march write + resolve read); resolve adds `march·e`; write-once pin green | FR4, NFR2 |
| CGV-007 | done | Ray-reconstruction pin: `march_ray` CPU mirror vs `project_to_screen` (direction ≤ 0.5°, depth bracketed) | NFR5 |
| CGV-008 | done | `VeilMode::for_tier` (Sprites / March 32 / March 48) + `GAME_DEBUG_COSMIC_VEIL` + sprites-off-when-marching; layout line prints mode; tests | FR5, FR6 |
| CGV-009 | done | Retired smoke + old veil + braid/spine/strand helpers + tests; `rg smoke_puffs\|glow_point_cloud\|braid_point\|spine_subsegments` = 0 in `crates/` | Goals §4, FR6, DoD 3 |
| CGV-010 | done | March fetch budget recorded (4.1M Med / 6.2M High per 1080p frame + ≈ 1 MB target); Low ≈ 168k sprites ≈ 6.0 MB; UHD 620 captures clean | NFR1, DoD 5 |
| CGV-011 | done | Intel UHD 620 High captures `slab` + `demo` + `inspector` clean both modes; capture determinism byte-identical per build+seed | NFR2, NFR3, DoD 5 |
| CGV-012 | done | Shots ×3 presets ×2 modes in `shots/`; Low/High parity at `slab` + `demo`; near-eye wash check at `demo` | DoD 1–2, NFR6 |
| CGV-013 | done | Docs: `rendering.md` veil section, `quality.md` veil row (cue-raymarch row removed), techstack 0.45.0, `architecture.md` field-render note; link check | DoD 6 |
| CGV-014 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |
| CGV-015 | done | Grade round 2 (ANALYST finding 2026-09-22): vis-weighted mean normalization in `MARCH_FRAG` + decoupled resolve gain; re-captured + re-audited | DoD 1–2, UX-1 |
| CGV-015 | pending | Grade round 2 (ANALYST finding 2026-09-22): vis-weighted mean normalization in `MARCH_FRAG` — full-depth views wash; re-grade + re-capture + re-audit | DoD 1–2, UX-1 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-only; one new
pass/pipeline/3D image recorded in budgets; march target governed by
the same pass-description pin as bloom; ramp snippet single source;
engine untouched) · Todos approved by: TECHLEAD (2026-09-20 —
risk-first: shared ramp + Low sprites first (Low must ship regardless
of the march); ray pin (CGV-007) before any grading; cost measured
(CGV-010) with recorded cut order: sheet sprites → alpha floor → count
stride on Low; steps 48 → 32 → 24 on Medium/High) · Grade-round-2
fix approved by: TECHLEAD (2026-09-22 — ANALYST wash finding becomes
CGV-015: mean normalization + decoupled resolve gain, no new pass,
no invariant touch) · UX acceptance rows:
approved 2026-09-20 — UX-1…UX-3 below · DoD verified by: ANALYST
(2026-09-22 — per-row audit below) · Security reviewed by: SECURITY
(2026-09-22 — debug-only, no new input surface/dependency/unsafe;
see review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | High march bodies + walls; Low same structure grainier; voids dark | done (slab center-row scan: backdrop 6.3, void floors 6.3 = 1.0× backdrop ≤ 3×, bodies continuous, hub peaks brighter on march (93) than sprites (79); Low same hubs/structure grainier) | `shots/slab-after-march.png` + `shots/slab-after-sprites.png` + scan numbers | ANALYST 2026-09-22 |
| 2 | Demo inside filament: translucent, no wash, hub visible (both modes) | done — after CGV-015 re-grade (round-1 march washed demo: corners 68; round-2: march corners 33 vs sprites 28, hub peaks ≈ 130 both modes, translucent mist; UX-1 hotfrac 0.03% march / 0.02% sprites, limit 50%) | `shots/demo-after-march.png` + `shots/demo-after-sprites.png` + scan numbers | ANALYST 2026-09-22 |
| 3 | Smoke/old veil/braid code gone; layout prints mode | done | `rg smoke_puffs\|glow_point_cloud\|braid_point\|spine_subsegments` = 0 in `crates/`; headless `veilmarch48` / `veilsprites335387` | ANALYST 2026-09-22 |
| 4 | Tests listed green (sprites, snippet pin, ray pin, pass pin, tier, env) | done | `nominal_veil_count_in_band`, `diameters_follow_class`, `alpha_clamps_on_synthetic_high_value`, `jitter_bounded_deterministic_and_rebased`, `cosmic_density_ramp_shared`, `march_ray_agrees_with_project_to_screen`, `veil_chain_covers_the_march_target`, `parse_override_forms_and_rejections`, `for_tier_mapping`, `march_gains_stay_decoupled` — full workspace suite green | ANALYST 2026-09-22 |
| 5 | UHD 620 clean; costs recorded | done (captures made ON the reference Intel UHD 620: no coloured squares; march48 slab byte-identical across builds pre/post ramp-unification; Low 167 694 drawn ≈ 6.0 MB; march 4.1M/6.2M fetches + ≈ 1 MB target) | shot paths + `quality.md` veil row | ANALYST 2026-09-22 |
| 6 | Docs + links | done | `rendering.md` veil section, `quality.md` row (cue-raymarch row removed), techstack 0.45.0, `architecture.md` field-render note; link check green | ANALYST 2026-09-22 |
| 7 | Gates + audit + review + one commit | done | fmt/clippy/build/workspace-tests/doc-tests/`game`/headless both modes/tools-smoke/mobile guards green; this audit + SECURITY note; single commit | ANALYST + SECURITY 2026-09-22 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. March target written once by the march pass, read only by the
  resolve [T: pass-description pin].
- A-2. March rays reconstructed from the inverse of the **same**
  view-proj pushed to the draws; NDC top row [T: CGV-007].
- A-3. Fragment loops: texture fetch + `mix`/FMA only; no `exp`/`pow`
  [T: shader pin].
- A-4. One density ramp snippet across splat / sprite / march [T: pin].
- A-5. Sprites mode never dispatches the march or draws veil sprites
  while marching (one body, not two): `record_veil_march` is
  mode-gated on both the capture and windowed paths; the glow upload
  takes the stride-2 sprite subset only in `Sprites` mode. (As built:
  the tier-less debug binary still creates the march pipeline + 2 MB
  volume at boot in both modes — per-frame march cost is zero in
  sprites mode; lazy creation is a follow-up if a tiered consumer
  needs it. `VeilMode::for_tier_low/medium/high` pins the mapping.)
- A-6. No `game` / `tools` diff; `engine::universe` + hashed paths +
  `engine::render::cue` untouched. One additive-only exception, required
  by FR4: `engine::render::post` gains `resolve_frag_bloom_march` +
  `BloomMarchResolvePush` (new items; no existing shader/struct/test
  touched — the `bloom-mip-chain` post.rs precedent).

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

## ANALYST audit note (2026-09-22)

- Reproduced: full gate suite green on the audited tree (fmt, clippy
  `-D warnings`, build, workspace tests incl. 252 lib + 36 bin,
  doc tests, `game`, headless both veil modes, tools smoke, both
  mobile targets); captures made on the reference Intel UHD 620.
- Finding (fixed as CGV-015): round-1 march washed full-depth views
  (demo corners 68, inspector whiteout) — column integral graded at
  the 30 Mpc slab cannot serve 100–500 Mpc chords. Fix verified:
  vis-weighted mean + resolve gain 30; slab pixels byte-identical to
  round 1, demo corners 33 (≈ sprites 28), no coloured squares.
- Mid-fix incident (process lesson, no shipped effect): one const fed
  both the march-push gain and the resolve gain → 900× blowout in
  test captures; caught by the same scan before any commit. The two
  knobs are now separate consts pinned by
  `march_gains_stay_decoupled`.
- Determinism: slab march48 byte-identical across the ramp-unification
  rebuild; slab sprites byte-identical across the grade rebuild.
- E2E/save scope: n/a (debug-shell visual feature; no save format,
  no journey change).

## SECURITY review note (2026-09-22)

- Surfaces: new files `cosmic_veil.rs` (pure functions), march pass +
  3D texture + quarter-res target (GPU-only, fixed sizes from
  `WebField`), one env override (`GAME_DEBUG_COSMIC_VEIL`,
  strict `sprites|march|march:8..=64` parse, bad values fall back to
  High march with a stderr note — no panic path), six PNG evidence
  shots (≤ 4 MB convention met: largest 1.6 MB).
- No new dependency, no new `unsafe`, no save/migration touch, no
  untrusted-input parsing beyond the env override + existing CLI.
- Verdict: pass, no findings.
