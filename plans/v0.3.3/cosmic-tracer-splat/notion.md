# Notion — cosmic-tracer-splat

## Status

`done` (PO sign-off 2026-09-20; UX consulted; ARCHITECT + TECHLEAD
breakdown in `plan.md`; ANALYST audit + SECURITY review recorded in
`plan.md` DoD table; single commit on branch `v0.3.3`)

## Context

Today the cosmic surfaces draw ~800k **grain** points (`grain_cloud`,
`cosmic_web.rs:711`) and ~60k **gold beads** (`bead_cloud`, `:807`)
scattered along the straight node-to-node links with sinusoidal
"braid" wander. Both are fixed-pixel sprites (1.5–2.5 px) with a flat
per-link palette; they read as dotted lines. The target
([`target.jpeg`](../../../docs/reports/images/target.jpeg), report §4,
§7, §8) reads as **gas**: continuous, curved, braided filaments whose
brightness tracks projected mass and whose hue tracks density (deep
blue → lavender → white → yellow → red), with four point classes by
environment.

`web-field-export` delivers the data such a picture needs: ~1M
Zel'dovich tracers with a smoothed local overdensity each. This feature
renders them as the Illustris renders do — one additive splat per
particle, kernel size and color from local density — and retires grain
and beads.

## Problem & Needs

- **Continuity.** Filament bodies must be continuous along their real
  (curved, branching) path — impossible with segments, natural with
  particles that followed the flow.
- **Brightness ≈ projected mass.** Where many tracers overlap the
  picture must get brighter; nowhere else. Additive constant-energy
  splats give this for free; the current per-link alpha does not.
- **Hue ≈ density.** One ramp, applied per particle, replaces the
  three hand-tuned palettes (smoke, grain, bead) that never agree.
- **Point classes (report §7).** D = faint blue-white speckle
  everywhere along filaments; B = warm yellow galaxies on filaments;
  C = pink/red in and around nodes; A = cluster cores (not this feature
  — `cosmic-hub-hierarchy`). Classes must emerge from density and
  proximity to hubs, not from a separate hand-placed list.
- **Mobile budget.** 1M points is fine on desktop; Low tier must draw a
  subset that still reads as the same web (ADR-025 §4).

## Goals

1. **Adaptive-kernel splat.** Each tracer is one additive point sprite
   with world-space kernel `h = h0 · (1+δ)^(-1/3)` clamped to `[0.5,
   4.0] Mpc`, projected to `[1.5, 64] px`; per-splat energy is constant
   (alpha ∝ 1/h² on screen) so a dense clump is bright because it has
   many particles, not because each is bigger.
2. **Density ramp.** Color = `ramp(log2(1+δ))`: deep indigo (δ ≲ 0,
   voids) → lavender (filament body) → white (dense filament core) →
   pale yellow → orange → pink-red (node cores). One 5-stop constant
   ramp, arithmetic-only in the shader (mix of stops, no textures).
3. **Environment classes.** D = every tracer (the ramp's low end);
   B = tracers with `1+δ ≥ 3` get a 1.5–2.5 px warm "galaxy" core
   sprite on top of their kernel; C = tracers within `1.5 · r_vir` of a
   top-tier hub (rank list from `cosmic-hub-hierarchy`, or the 1 %
   most massive nodes until it lands) tint toward pink-red. Classes
   are derived, not stored.
4. **Tiered counts.** Low 300k / Medium 1.0M / High all tracers (+
   optional `refine(2)` later). Selection is deterministic stride by
   tracer index (no RNG), so Low is a strict subset of Medium.
5. **Retire grain + beads.** `grain_cloud`, `bead_cloud`, their
   constants, streams, tests, and `upload_cosmic_glow` sections go;
   the glow `PointList` buffer holds splats + (until F4) node impostors
   + (until F6) gas veil.
6. **Shots.** `inspector`, `slab`, `demo` presets before/after in
   `shots/`.

## Non-goals

- No hub impostors / member galaxies (`cosmic-hub-hierarchy`).
- No fog / slab / FOV change (`cosmic-depth-window`) — although this
  feature will look **over-full** until F3 lands; that is expected and
  recorded, not tuned around.
- No gas veil / raymarch (`cosmic-gas-veil-v2`); smoke puffs stay
  until then.
- No bloom change (`bloom-mip-chain`).
- No new pipeline: splats ride the existing additive glow `PointList`
  with a new vertex format variant (`SplatVertex`) and a new shader
  pair — a pipeline **variant**, not a new pass or draw.
- No sorting / occlusion (additive, order-independent by construction).
- No change to `WebField`, `WebDescriptor`, or the engine.

## Users / Stakeholders

- **Player (Game Demo):** flies through filament bodies that are
  continuous and gaseous; sees warmer, denser threads ahead toward
  hubs.
- **Developer (Cosmic Web tab):** the inspector reads as a density
  map — structure, not dots.
- **ANALYST:** judges shots against target §4/§7/§8.
- UX consulted: yes (player-facing demo surface; near-eye behaviour).

## Roles

Author: PO. UX consulted (required if player-facing): yes — near-eye
fade + no white flash, legibility of the hub direction. ARCHITECT
consulted (required if cross-module): yes — consumes
`engine::universe::WebField` from `debug`; pipeline variant (same
pass), invariants pinned.

## Functional requirements

- FR1 (vertex): `SplatVertex { pos: [f32;3], overdensity: f32 }` (16 B,
  ~16 MB at 1M) uploaded once per surface at the upload origin;
  re-uploaded on rebase (50 Mpc rule) and reseed, exactly like the
  current glow buffer.
- FR2 (kernel): vertex shader computes `h` from `overdensity`
  (`pow(x, -1/3)` is `exp2(-log2(x)/3)` — allowed in the **vertex**
  stage; fragment stays arithmetic-only), projects to pixels with
  `px_scale / clip.w`, clamps `[1.5, 64]`, sets `gl_PointSize`;
  alpha = `k / max(px², 1)` × per-surface exposure so integrated
  energy per splat is constant above the 1.5 px floor.
- FR3 (ramp): 5 stops at `log2(1+δ)` = `−2, 0, 1.5, 3, 4.5` →
  `[0.10,0.08,0.35]`, `[0.35,0.32,0.80]`, `[0.85,0.85,1.00]`,
  `[1.00,0.92,0.60]`, `[1.00,0.45,0.40]`; linear mix between stops;
  emissive scale `1 + 0.5·max(0, log2(1+δ) − 1.5)` so dense cores cross
  the bloom threshold. Starting values — tuned from shots, recorded as
  constants.
- FR4 (classes): B core sprite emitted CPU-side as a second
  `SplatVertex` with a flag (`overdensity` sign bit or a 4th component)
  → shader draws a 2 px warm point (`[1.0, 0.85, 0.55]`, emissive 2)
  on top of the kernel; ≤ 5 % of tracers qualify at nominal (band
  test). C tint applied CPU-side by hub proximity (`1.5 · r_vir` of
  the 1 % most massive nodes) as a per-vertex color shift toward
  `[1.0, 0.5, 0.55]` — stored in the same 4th component as a packed
  `u8` pair `(class, tint)`.
- FR5 (near-eye): the demo player flies inside filaments; splats
  closer than `2·h` to the eye fade out (`smoothstep(h, 2h, dist)`) so
  no single kernel fills the screen (the v0.3.2 white-flash lesson).
- FR6 (Hubble tint): the existing bounded redshift tint
  (`z ≤ 0.5`, `COSMIC_REDSHIFT_PER_MPC`) applies to splats unchanged.
- FR7 (tiers): `SPLAT_BUDGET[tier] = {300_000, 1_000_000, u32::MAX}`;
  stride selection `i % ceil(n/budget) == 0`; count logged in the
  headless `cosmic_layout=` line.
- FR8 (retire): remove `grain_cloud`, `bead_cloud`, `GRAIN_*`, `BEAD_*`
  constants and streams, their tests and the `rendering.md`
  paragraphs; the strand/braid helpers stay **only** if
  `smoke_puffs` still needs them (they retire with F6).

## Non-functional requirements

- NFR1 (budgets, `quality.md`): Low ≤ 300k points, ≤ 16 MB/surface
  buffers (Low ≈ 5 MB), no new draw call, no new pass; overdraw at the
  `inspector` preset measured (stacked-px estimate from kernel sizes,
  logged) and recorded; if Low overdraw > 8× average at 1080p → cut
  order: max kernel px `64 → 32`, then budget `300k → 200k`.
- NFR2 (determinism): CPU-side class/tint derivation is a pure
  function of `(WebField, WebDescriptor)` — no RNG; replay test.
- NFR3 (shaders): fragment shader arithmetic-only (`fract`/`mix`/
  polynomials), rim-zero kernel `(1 − 4d²)²` as today; vertex may use
  `exp2/log2` (per-vertex, not per-pixel). Pinned by
  `cosmic_shader_safety_pins`.
- NFR4 (invariants): projection un-flipped `directx::perspective`;
  picking untouched (splats pick-ignored — `select_at` iterates
  `web.nodes`); marker via `world_to_pixels`; bloom write-once
  untouched; rebase re-upload correctness test (translation-invariant
  except `pos`).
- NFR5 (upload time): building 1M `SplatVertex` + class derivation
  ≤ 150 ms on the reference desktop (measured at boot, recorded);
  hub-proximity test uses the node grid already built for links
  (60 Mpc cells) — no O(N·M) loop.

## Definition of Done

1. Both surfaces draw tracer splats from `WebField`; grain and beads
   are gone (no symbol, no constant, no doc paragraph); headless
   `cosmic_layout=` prints `splats N` per tier.
2. Shots `inspector-after.png`, `slab-after.png`, `demo-after.png`
   committed; ANALYST judges vs target §4 (continuous curved bodies),
   §7 (three of four classes visible: D speckle, B warm galaxies,
   C pink near hubs), §8 (brightness stacks, hue follows density).
   Expected and accepted at this stage: voids still filled from depth
   (F3 pending), hubs still 6000 pins (F4 pending), no wide bloom (F5).
3. Low tier: 300k splats, ≤ 5 MB buffer, no new draw/pass; overdraw
   estimate recorded; any cut applied per NFR1 and recorded.
4. Tests: kernel-size monotone in density, alpha×px² constant above
   floor, ramp stops in band, class B fraction ≤ 5 %, C tint only
   within `1.5·r_vir` of top-1 % hubs, stride subset property (Low ⊂
   Medium), rebase translation invariance, replay identity; shader
   pins extended (rim-zero, no transcendental in fragment).
5. Near-eye: flying through a filament in the demo shows no frame with
   > 50 % of pixels above the bloom threshold (capture-based check at
   the `demo` preset advanced 5 Mpc into the filament).
6. Docs: `rendering.md` cosmic section rewritten for splats, grain/bead
   paragraphs removed, `quality.md` splat row, techstack version bump;
   links resolve.
7. Full gate list green + mobile guards; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Depends on `web-field-export` (data) and `cosmic-capture-harness`
  (shots). Third feature on branch `v0.3.3`.
- Smoke puffs keep drawing until `cosmic-gas-veil-v2`; their exposure
  may be **lowered** here (constant only) if they fight the splats.
- Ramp/kernel constants are starting points; tuning rounds are recorded
  as numbered notes in `plan.md` (the D-1…D-7 precedent) and each round
  ships a shot.

## Open questions

- Whether B "galaxy" cores should be a separate vertex (double count)
  or a shader branch on the same vertex (one point, two lobes). PO
  leans shader branch (halves buffer growth); TECHLEAD decides on
  measured Low overdraw.
- `refine(2)` on High: on or off by default? Decide from High-tier
  shots at the `slab` preset; not a DoD item.
