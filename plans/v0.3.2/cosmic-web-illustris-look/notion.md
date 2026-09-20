# Notion — cosmic-web-illustris-look

## Status

`in-review` (DEV implementation + full gate suite green 2026-09-20,
evidence in `plan.md` DoD table; ANALYST audit + SECURITY review
pending human go-ahead; single `done` commit pending)

## Context

v0.3.2 `cosmic-scale-player` (done) generates the stage-0 web
(`engine::universe::web`: seeded Zel'dovich → T-web + Press–Schechter →
`WebDescriptor` with ~6,000 nodes / ~22k links / ~150k glow) and renders
it two ways: the Game Demo player scene and the Cosmic Web inspector tab
(`debug::cosmic_web` enrichment: `1–3` braid strands per link,
`BRAID_AMPLITUDE 1.5 Mpc`, `800k` grain points, `~52k` smoke billboard
puffs at `2.0–6.5 Mpc` diameter, 3-layer node impostors, 5-target HDR
bloom). Reference comparison lives in
[`docs/reports/2026-09-19-cosmic-web-visual-architecture.md`](../../../docs/reports/2026-09-19-cosmic-web-visual-architecture.md)
(`target.jpeg` vs `current.png`).

The agreed visual target (`[Image 1]`, Illustris-style projection:
brightness ≈ projected mass density, hue ≈ mean projected gas
temperature; DM splatted with an SPH kernel scaling with local density)
shows **hair-thin blue threads + thousands of golden beads strung along
them + clean dark voids + one dominant white-gold hub**. Research
(`DisPerSE`/COWS literature): real filaments are thin (overdensity falls
to the mean within `<3 Mpc` of the spine), widths/lengths vary `5–100×`,
and the skeleton branches at bifurcation nodes — not just straight
node-to-node links.

A full iterative N-body replacement (`z=50→0`, Barnes-Hut/PM over
millions of particles) was evaluated and **rejected**: `0.5 s → minutes`
of boot (cold-start gate `<3–5 s`), breaks the `same (seed, version) →
byte-identical` determinism contract (integer decisions + `sqrt`-only in
hashed paths), and raw `δ`-threshold voxels cannot produce the
gameplay-required `WebNode`/`WebLink`/`home_node` data. The Zel'dovich
step *is* the supercomputer initial-condition integrator in one analytic
timestep — it stays as the data authority.

PO decision (2026-09-20): close the visual gap with a **render-only
enrichment pass** on the same version (`v0.3.2`, same branch — no
`v0.3.3`). No descriptor, hash, or save change.

## Problem & Needs

- Filaments render as `Mpc`-wide round mist blobs, not hairlines: the
  smoke puffs (`2.0–6.5 Mpc` isotropic billboards, `±1 Mpc` jitter)
  wash out thin detail and stack `~50/px` into white slabs in the
  zoomed-out inspector.
- No branching: links are straight `a↔b` node pairs (≤8/node, ≤60 Mpc);
  the target's threads split mid-filament at bifurcations.
- No gold dust: thousands of tiny gold/white beads along the threads in
  the target; we discard all sub-threshold peaks instead of rendering
  them, leaving only `6k` massive nodes on bare blue mist.
- Hubs read as blue cotton balls, not sharp white-gold points with blue
  threads radiating out.

## Goals

1. **Branching skeleton.** Deterministic 1-cell spines + bifurcation
   points derived from the `FILAMENT|NODE` mask (COWS-lite medial-axis
   thinning, integer BFS).
2. **Hair threads.** `3–7` frayed sub-strands per spine segment (seeded
   midpoint displacement), expanded on-GPU as camera-facing ribbons at
   `0.1–0.4 Mpc` half-width with a `1–1.5 px` floor.
3. **Smoke v2 sheath.** Same `≤65k` puff budget, reshaped: tangent-aligned
   stretched impostors (`3–8 Mpc` long × `0.3–1.0 Mpc` thin), jitter
   tightened `1.0→0.35 Mpc`, two-tier alpha (bright core + faint halo),
   finer `fract`-only dust. Mist becomes a sheath around threads.
4. **Gold beads.** Sub-threshold peaks rendered as dwarf-galaxy sprites
   (`1.5–2.5 px`, white-gold, emissive `>1.0` to catch bloom) strung
   along spines.
5. **Illustris grade.** Thread brightness by segment density, bead/hub hue
   by mass; faint threads sink fully into the `[0.008, 0.005, 0.024]`
   clear; hubs warm amber.

## Non-goals

- No N-body / 2LPT / nested-grid generation change (roadmap-deferred);
  `engine::universe::web` is untouched.
- No volumetric 3D raymarch pass (deferred to discrete-GPU tiers per the
  milestones cosmic-roadmap); the `quality.md` cue budget
  (`Low 64×64 ≤16` steps) is untouched.
- No `UNIVERSE_VERSION` bump, no descriptor/hash/content-ID change, no
  save migration, no galaxy/system/planet stream change.
- No new pipelines (reuse glow `PointList` + smoke `TriangleList`), no
  new dependencies, no new `unsafe`, no `game`-crate change, no
  release-binary UI change.
- No reference image under `assets/` (PO precedent from
  `cosmic-scale-player`: target captured textually here).

## Users / Stakeholders

- Player (Game Demo tab): first impression of the game is this vista;
  goal legible in 3 s — bright hub ahead, threads to follow.
- Developers (Cosmic Web inspector tab): structure readable zoomed out,
  no white-slab saturation.
- UX consulted: yes (player-facing demo surface). ARCHITECT consulted:
  yes (shader/enrichment design, invariant pins).

## Roles

Author: PO. UX consulted (required if player-facing): yes — demo vista +
inspector readability, acceptance rows UX-1…UX-4 in `plan.md`.
ARCHITECT consulted (required if cross-module): yes — render-only
enrichment, no boundary crossing, no ADR except extending ADR-023 notes.

## Functional requirements

- FR1 (skeleton): spine segments + bifurcation points derive
  deterministically from `(seed, web)`; replay-identical; empty-web input
  yields empty output (no panic).
- FR2 (threads): every spine segment draws `≥3` sub-strands on dense
  links; strand width `0.1–0.4 Mpc` world with `1–1.5 px` minimum;
  strands taper into endpoint hubs (melt profile, shader-side).
- FR3 (smoke v2): every puff carries a link-tangent axis; quads elongate
  along it (`3–8 Mpc`) and stay thin across (`0.3–1.0 Mpc`); halo tier may
  vanish subpixel, core tier never vanishes above the `2 px` clamp.
- FR4 (beads): dwarf sprites emit along spines only (never in voids),
  capped by a two-pass budget mirroring the glow precedent; each bead is
  pick-ignored (selection stays node-only).
- FR5 (grade): per-surface exposures stay independent (`COSMIC_DEMO_*`
  vs `COSMIC_MAP_*`); near-eye fade (`1→6 Mpc`) and bounded redshift
  (`z ≤ 0.5`) preserved on every cosmic draw.

## Non-functional requirements

- NFR1 (determinism): enrichment is render-only — `web_hash` vectors
  unchanged; hashed paths keep integer decisions + `sqrt`-only; `sin`/
  `cos` confined to `braid_point`-style CPU derivations already covered
  by same-platform replay (not hashed); fragment shaders use
  `fract`-hash only (no `sin`/`exp` per pixel — mobile fill-rate).
- NFR2 (budgets, `quality.md`): Low `tris <500k`, draws unchanged in
  count, memory delta `<5 MB`/surface, cold start unaffected (CPU
  enrichment stays `<0.5 s` on nominal web); overdraw measured, not vibed
  (`tools --headless --tier low`).
- NFR3 (gates): full `quality.md` gate list green + mobile compile guards
  before `in-review`; tier-gate or cut (not blow) on any Low breach:
  beads off on Low first, halo smoke second, strands `7→3` third.
- NFR4 (invariants): un-flipped `directx::perspective`, `FrontFace/CCW`
  (points/lines exempt as today), `world_to_pixels` marker path,
  `ndc = (2u−1, 1−2v)` picking, bloom write-once (every HDR target
  written exactly once, then only read — Intel UHD 620 rule).

## Definition of Done

1. Demo + inspector show branching hair threads with gold beads and a
   dark-void background, side-by-side closer to `target.jpeg` (thread
   width down, void darkness up, hub gold purity up — analyst-judged
   A/B with shot paths as evidence).
2. Stage-0 descriptor, `web_hash` vectors, and content IDs bit-identical
   before/after (existing determinism tests green, no version bump).
3. Low-tier budgets hold (`--headless --tier low` + tris/draws asserts;
   any breach tier-gated or cut per NFR3, recorded in `plan.md`).
4. New enrichment covered by replay-identical + bound tests
   (skeleton/strand/puff/bead counts finite, in-budget, rebase-safe).
5. Docs sweep + links resolve: `rendering.md` cosmic section,
   `2026-09-19-cosmic-web-visual-architecture.md` follow-up note,
   `techstack/README.md` version bump, milestones row `done`, ADR-023
   extension note; ANALYST audit + SECURITY review signed.

## Constraints & Assumptions

- Same version + branch (`v0.3.2`); lands as exactly one commit on `done`
  (version-branch rule); `cosmic-web-mass-rank-fix` (in-progress) merges
  first if both touch `descriptor.rs` — this feature must not touch it.
- Assumes nominal web shape (`~22k` links, `128³` lattice) for budget
  math; `CosmicWebParams` values untouched (re-grade via surface consts
  only).
- Enrichment seeds reuse domain-separated streams (`cosmic_web/braid`,
  `/grain`, `/smoke` + new `/bead`); canonical link order consumption.

## Open questions

- None blocking. Low-fallback order default (beads → halo → strands,
  NFR3); PO may reorder before implementation without a notion edit
  (tuning, not scope).
