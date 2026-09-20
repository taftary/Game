# Plan — cosmic-tracer-splat

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only (`cosmic_web.rs` for CPU
derivation, `main.rs` for the pipeline variant + shaders). Consumes
`game_engine::universe::WebField` (read-only) — legal direction
(`debug → engine`). The splat pipeline is a **variant** of the
additive glow `PointList` pipeline (same render pass, same blend, new
vertex input + shader pair) — no new pass, no new draw beyond
replacing the grain/bead sections of the existing glow draw with one
splat draw. **Invariant rows:** un-flipped projection; picking
node-only; marker path; bloom write-once; fragment arithmetic-only;
replay/rebase determinism of CPU derivations.

### Phase 1 — CPU derivation (`cosmic_web.rs`)

`SplatRecord { pos, overdensity, class_tint: u32 }`;
`splat_records(&WebField, &WebDescriptor, origin, tier) ->
Vec<SplatRecord>`: stride subset per tier → hub-proximity C tint via
the 60 Mpc node grid → B flag by `1+δ ≥ 3`. Pure function, no RNG.
Tests: subset property, class bands, proximity, translation invariance,
replay.

### Phase 2 — Pipeline variant + shaders (`main.rs`)

`SplatVertex` (16 B) + `SPLAT_VERT`/`SPLAT_FRAG`; `build_splat_pipeline`
cloning `build_glow_pipeline` with the new vertex input; push block
reuses `GlowPush` (+ `h0`, `alpha_k` — still < 128 B). Vertex: kernel
`h`, projected px clamp, constant-energy alpha, ramp color, emissive,
near-eye fade, Hubble tint. Fragment: rim-zero `(1 − 4d²)²`, optional
2 px warm core lobe for class B. Pins extended.

### Phase 3 — Wiring + retirement (`main.rs`, `cosmic_web.rs`, docs)

`upload_cosmic_splats` (boot, reseed, rebase); glow buffer keeps node
impostors + gas veil only; delete `grain_cloud`, `bead_cloud`,
constants, streams, tests; headless `cosmic_layout=` prints `splats N`;
`rendering.md` grain/bead paragraphs removed.

### Phase 4 — Grade rounds + shots + gates

Ramp/kernel/exposure tuning against the `slab`/`inspector`/`demo`
presets, each round a numbered note + shot; Low overdraw estimate;
near-eye capture check; full gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CTS-001 | done | `SplatRecord` + `splat_records(field, web, origin, tier)`: stride subset, B flag, C tint via node grid; replay + translation-invariance tests | Goals §3–4, FR4, FR7, NFR2, NFR5 |
| CTS-002 | done | Tests: Low ⊂ Medium subset; B fraction ≤ 5 % nominal; C tint only within `1.5·r_vir` of top-1 % hubs | FR4, DoD 4 |
| CTS-003 | done | `SplatVertex` + `build_splat_pipeline` (glow variant, additive, same pass) + `SPLAT_VERT`/`SPLAT_FRAG`; `viewer_shaders_compile` green | FR1, FR2, NFR3 |
| CTS-004 | done | Vertex: kernel `h(δ)` clamp `[0.5, 4] Mpc` → `[1.5, 64] px`, constant-energy alpha, 5-stop ramp + emissive, near-eye fade, bounded Hubble tint | Goals §1–2, FR2, FR3, FR5, FR6 |
| CTS-005 | done | Fragment: rim-zero kernel + class-B warm core lobe; extend `cosmic_shader_safety_pins` (rim-zero literal, no `sin/exp/pow` in fragment) | FR4, NFR3 |
| CTS-006 | done | Upload + rebase + reseed wiring; `cosmic_layout=` prints `splats N`; buffer size logged | FR1, DoD 1 |
| CTS-007 | done | Retire grain + beads: code, constants, streams, tests, `rendering.md` paragraphs; keep braid helpers only for `smoke_puffs` | Goals §5, FR8 |
| CTS-008 | done | Low overdraw estimate (Σ kernel px² / viewport px at `inspector` preset, logged); apply cut order if > 8× | NFR1, DoD 3 |
| CTS-009 | done | Grade rounds vs target §4/§7/§8 at `slab`, `inspector`, `demo`; each round = numbered note + shot; final `*-after.png` ×3 | Goals §6, DoD 2 |
| CTS-010 | done | Near-eye capture check (`demo` preset +5 Mpc into filament): < 50 % pixels above threshold | FR5, DoD 5 |
| CTS-011 | done | Docs: `rendering.md` splat section, `quality.md` splat row, techstack version bump; link check | DoD 6 |
| CTS-012 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Measurements (2026-09-20, Intel UHD 620, dev profile)

Headless (`cargo run -p game_debug -- --headless`):

```text
cosmic_layout=smoke51953 splatsL255900 splatsM511800 splatsH1023599 impostors18000 overdrawL2.0 ok
```

- Low 255 900 ≤ 300k; 16 B vertices → 4.1 MB ≤ 5 MB; no new draw/pass
  (splat draws ride the existing scene + main passes).
- Overdraw 2.0× ≤ 8× — no cut applied (cut order armed: kernel
  64→32 px, then budget 300k→200k).
- Near-eye wash: `demo-after.png` bright fraction (≥200) = 0.0127
  (1.3 %) — pinned by `cosmic_capture::tests::demo_after_shot_has_no_white_flash`.
  The `demo` preset boots inside a filament (spawn guarantee), so the
  boot framing exercises the near-eye fade; advancing 5 Mpc with ticks
  is not capturable offscreen (no ticks at t = 0) — the fade itself is
  shader-pinned (`smoothstep(h, 2.0*h, dist)` literal).

## Grade rounds (vs `target.jpeg` §4/§7/§8)

- Round 0 (constants as specified, `alpha_k = 1.0` both surfaces):
  `demo` good (continuous gaseous bodies, warm cores, dark edge
  voids); `inspector` a white slab — 1 M splats over the full 500 Mpc
  depth column carry ~7× the retired grain's per-px energy at
  zoom-out (constant-energy units stack over depth).
- Round 1 (final): per-surface `alpha_k` (demo 1.0, map 0.2;
  `SPLAT_ALPHA_K_DEMO/MAP`). Inspector now reads as a density map
  (curved/branching threads, D speckle + B warm galaxies + C pink
  near hubs, brightness stacks with density). Voids still filled from
  depth (R-1 as recorded — F3's fog owns that), hubs still 6000 pins
  (F4), no wide bloom (F5). Constants held per R-1 (no tuning against
  the depth problem beyond the energy grade).

## Implementation notes vs plan

- `SplatVertex` is 16 B via a packed `u32` (`log2(1+δ)` q16 over
  `[-8, 8)` + 2-bit C tint + B flag) instead of the specified
  `(pos, overdensity)` + 4th component — Low buffer math (4.8 MB)
  depends on it; pack round-trip pinned within half a quantum step.
- Class-B cores are a shader branch on the same vertex (one point,
  warm center lobe), not a second vertex — the PO-leaning option from
  the open questions (halves buffer growth).
- No runtime tier switching in the debug binary (no device-profile
  plumbing exists there): windowed + capture draw High (all
  tracers); Low/Medium verified CPU-side (stride, buffer math,
  overdraw). A Low profile calls `splat_records(..., Low)`.
- `refine(2)` stays off (High draws all base tracers; decided from
  the `slab` shot — density is sufficient without refinement).

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-only consumer of
the engine sidecar; pipeline variant, not a new pass; retirement of
grain/beads is explicit todos; invariants pinned) · Todos approved by:
TECHLEAD (2026-09-20 — risk-first: CPU derivation with its tests
(CTS-001/002) before GPU work; overdraw measured (CTS-008) before
grading; cut order recorded: kernel px `64 → 32`, then budget
`300k → 200k`) · UX acceptance rows: approved 2026-09-20 — UX-1…UX-3
below · DoD verified by: ANALYST (2026-09-20 — shots opened against
target §4/§7/§8; R-1 depth caveat accepted; wash pin reviewed) ·
Security reviewed by: SECURITY (2026-09-20 — no I/O, no new
dependency, pure CPU derivation + shader constants; `rg` retirement
verified).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Splats drawn on both surfaces; grain/beads gone; layout line | done | `rg grain_cloud\|bead_cloud` = 0 (only plan.md mentions remain); `cosmic_layout=smoke51953 splatsL255900 splatsM511800 splatsH1023599 impostors18000 overdrawL2.0 ok` | ANALYST 2026-09-20 |
| 2 | Shots ×3 judged vs target §4/§7/§8 | done | `shots/*-after.png` + grade rounds above (voids→F3, hubs→F4, bloom→F5 accepted) | ANALYST 2026-09-20 |
| 3 | Low: 300k, ≤ 5 MB, no new draw/pass; overdraw recorded | done | 255 900 pts, 4.1 MB, overdraw 2.0×, no cut | ANALYST 2026-09-20 |
| 4 | Tests listed green; pins extended | done | splat unit tests (7+3 new) + `cosmic_shader_safety_pins` + push/size pins | ANALYST 2026-09-20 |
| 5 | Near-eye: no white flash | done | bright fraction 0.0127 + `demo_after_shot_has_no_white_flash` pin | ANALYST 2026-09-20 |
| 6 | Docs + links | done | `rendering.md` splat section, `quality.md` row, techstack 0.41.0 | ANALYST 2026-09-20 |
| 7 | Gates + audit + review + one commit | done | gate log below; commit hash | ANALYST + SECURITY 2026-09-20 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Projection un-flipped `directx::perspective`; no front-face/cull
  state (points) [T: existing pins].
- A-2. Picking iterates `web.nodes` only; splats never selectable
  [T: existing pick tests unchanged].
- A-3. Marker via `world_to_pixels` only [T: existing].
- A-4. Bloom targets untouched (write-once preserved) [T: HdrChain diff
  empty].
- A-5. Fragment shader contains no `sin`, `cos`, `exp`, `pow`, `log`
  [T: `cosmic_shader_safety_pins`].
- A-6. `splat_records` deterministic per `(field, web, origin, tier)`;
  translation-invariant except `pos` [T].
- A-7. No engine, `game`, or `tools` diff.

UX acceptance rows (player-facing, Game Demo):

- UX-1. At boot (demo spawn) the filament the player sits in reads as a
  continuous body, and the direction toward the home hub is warmer /
  denser within 3 s.
- UX-2. Flying along a filament at cruise: no frame flashes white; the
  body dissolves ahead of the nose and re-forms behind (near-eye fade).
- UX-3. Inspector zoomed out: the picture reads as a density map with
  visibly curved/branching threads (not straight dotted lines); no
  white-slab saturation at the default framing.

## Risks & Next steps

- R-1 (over-full picture before F3): 1M splats over a 500 Mpc depth
  will fill voids from behind; ANALYST judges §4/§7/§8 only, voids are
  F3's DoD. Recorded to avoid tuning the ramp against a depth problem.
- R-2 (overdraw on Low): 300k × up to 64 px kernels at 1080p can exceed
  8× — cut order armed (CTS-008).
- R-3 (lattice imprint): if the `slab` shot shows a grid in voids, first
  fix is `web-field-export` jitter σ (constant), second is an alpha
  floor by `1+δ` in the vertex shader (this feature).
- R-4 (buffer size on rebase): 16 MB re-upload every 50 Mpc rebase;
  acceptable at boot-level frequency; if it hitches, double-buffer the
  upload (TECHLEAD call, no scope change).
- Next: `cosmic-depth-window` (fog/slab makes voids), then
  `cosmic-hub-hierarchy` (class A + members), then `cosmic-gas-veil-v2`
  retires smoke.
