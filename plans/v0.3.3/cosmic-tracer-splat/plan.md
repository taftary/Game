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
| CTS-001 | pending | `SplatRecord` + `splat_records(field, web, origin, tier)`: stride subset, B flag, C tint via node grid; replay + translation-invariance tests | Goals §3–4, FR4, FR7, NFR2, NFR5 |
| CTS-002 | pending | Tests: Low ⊂ Medium subset; B fraction ≤ 5 % nominal; C tint only within `1.5·r_vir` of top-1 % hubs | FR4, DoD 4 |
| CTS-003 | pending | `SplatVertex` + `build_splat_pipeline` (glow variant, additive, same pass) + `SPLAT_VERT`/`SPLAT_FRAG`; `viewer_shaders_compile` green | FR1, FR2, NFR3 |
| CTS-004 | pending | Vertex: kernel `h(δ)` clamp `[0.5, 4] Mpc` → `[1.5, 64] px`, constant-energy alpha, 5-stop ramp + emissive, near-eye fade, bounded Hubble tint | Goals §1–2, FR2, FR3, FR5, FR6 |
| CTS-005 | pending | Fragment: rim-zero kernel + class-B warm core lobe; extend `cosmic_shader_safety_pins` (rim-zero literal, no `sin/exp/pow` in fragment) | FR4, NFR3 |
| CTS-006 | pending | Upload + rebase + reseed wiring; `cosmic_layout=` prints `splats N`; buffer size logged | FR1, DoD 1 |
| CTS-007 | pending | Retire grain + beads: code, constants, streams, tests, `rendering.md` paragraphs; keep braid helpers only for `smoke_puffs` | Goals §5, FR8 |
| CTS-008 | pending | Low overdraw estimate (Σ kernel px² / viewport px at `inspector` preset, logged); apply cut order if > 8× | NFR1, DoD 3 |
| CTS-009 | pending | Grade rounds vs target §4/§7/§8 at `slab`, `inspector`, `demo`; each round = numbered note + shot; final `*-after.png` ×3 | Goals §6, DoD 2 |
| CTS-010 | pending | Near-eye capture check (`demo` preset +5 Mpc into filament): < 50 % pixels above threshold | FR5, DoD 5 |
| CTS-011 | pending | Docs: `rendering.md` splat section, `quality.md` splat row, techstack version bump; link check | DoD 6 |
| CTS-012 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 7 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-only consumer of
the engine sidecar; pipeline variant, not a new pass; retirement of
grain/beads is explicit todos; invariants pinned) · Todos approved by:
TECHLEAD (2026-09-20 — risk-first: CPU derivation with its tests
(CTS-001/002) before GPU work; overdraw measured (CTS-008) before
grading; cut order recorded: kernel px `64 → 32`, then budget
`300k → 200k`) · UX acceptance rows: approved 2026-09-20 — UX-1…UX-3
below · DoD verified by: ANALYST _(pending)_ · Security reviewed by:
SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Splats drawn on both surfaces; grain/beads gone; layout line | pending | `rg grain_cloud\|bead_cloud` = 0; headless log | ANALYST _(pending)_ |
| 2 | Shots ×3 judged vs target §4/§7/§8 | pending | `shots/*-after.png` + ANALYST note | ANALYST _(pending)_ |
| 3 | Low: 300k, ≤ 5 MB, no new draw/pass; overdraw recorded | pending | log line + cut record | ANALYST _(pending)_ |
| 4 | Tests listed green; pins extended | pending | test names | ANALYST _(pending)_ |
| 5 | Near-eye: no white flash | pending | capture + pixel count | ANALYST _(pending)_ |
| 6 | Docs + links | pending | file list | ANALYST _(pending)_ |
| 7 | Gates + audit + review + one commit | pending | gate log, commit | ANALYST + SECURITY _(pending)_ |

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
