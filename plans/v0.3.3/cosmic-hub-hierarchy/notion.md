# Notion — cosmic-hub-hierarchy

## Status

`done` (PO sign-off 2026-09-20; UX consulted; ARCHITECT + TECHLEAD
breakdown in `plan.md`; ANALYST audit + SECURITY review recorded in
`plan.md` DoD table; single commit on branch `v0.3.3`)

## Context

`node_impostors` (`cosmic_web.rs:907-948`) emits **three emissive
sprites for every one of the ~6000 nodes**: a near-white pin at every
mass (`1.05·(1.2+2.0l)`), a gold core, and a world-sized halo. The
mass ramp (`mass_level`: 10^12.3 → 0, 10^15 → 1) compresses the
hierarchy, so a 10^13 M☉ group and a 10^15 M☉ cluster differ by a
factor ~2 in size and ~1.6 in brightness. The result is 6000
near-identical white points — "Christmas lights" — where the target
(report §3) has **one** dominant node, **one** left complex,
**~6 secondary** nodes, and everything else is a bead.

The target's nodes also carry a **member-galaxy scatter**: hundreds of
orange / yellow / pink / white dots around each core, denser toward
the centre, with a pink/red suffusion (report §3 "Central node", §7
class C). The current build has no member population at all.

`web-field-export` gives tracers with `overdensity`; `WebNode` gives
`mass_msun` and `virial_radius_mpc`. This feature builds the visual
hierarchy from those two facts and nothing else.

## Problem & Needs

- **Focal hierarchy** (report §2): the eye must land on one node, then
  travel to a second anchor, then a ring of minor nodes — impossible
  when 6000 nodes are equally bright.
- **Class A + C galaxies** (report §7): cluster cores and their red
  members are missing; `cosmic-tracer-splat` covers D and B only.
- **Colour logic** (report §3): cores white / pale yellow → orange →
  pink rim; today's cores are one gold with a cyan-to-amber halo.
- **Navigation goal**: the player's fly-to target should be the one
  bright thing in view; today it competes with thousands of pins
  (`issue-2026-09-18-2122-target-highlight-missing` was the first
  symptom).
- **Bloom budget**: 18 000 emissive sprites all above threshold make
  the bloom chain bloom everything; the reference blooms ~10 things.

## Goals

1. **Three hub tiers by mass rank** (rank is already the descriptor's
   order: `node_index` ascending = densest/most massive first, ADR-023
   + mass-rank fix):
   - **Tier A — top 1 % (≈ 60 nodes):** 3-layer impostor (white-hot
     pin 4–6 px, core 12–20 px pale-yellow → orange rim via a 2-stop
     radial ramp inside the sprite, world-sized halo `1.5–2.5 · r_vir`
     warm pink-amber at low alpha) **+ member galaxies**: `40 + 200·l`
     points inside `r_vir` with an NFW-like radial profile (`r ∝ u²`),
     colours from the report §7 table (60 % pink/red C, 30 % orange
     B, 10 % white A), 1.5–3 px, emissive 1.5–3.
   - **Tier B — next 10 % (≈ 600 nodes):** single core sprite 4–8 px
     warm gold, small halo `1.0 · r_vir`, `10 + 20·l` members (pink /
     orange).
   - **Tier C — remaining 89 %:** one 2 px warm bead (`[1.0, 0.85,
     0.55]`, emissive 1.2), no halo, no members. These are the report's
     "junctions without a bright cluster" (§4).
2. **Bloom only where it belongs.** Tier A cores and pins cross the
   bloom threshold by ≥ 3×; Tier B cores by ~1.5×; Tier C beads stay
   below it.
3. **Selection / target highlight unchanged in behaviour**, improved in
   legibility: the selected / fly-to node gets the existing highlight
   ring; nothing else changes in `select_at`.
4. **Fog floor** from `cosmic-depth-window` applies to Tier A/B
   impostors only (Tier C beads fog like everything else) so the
   distant goal stays visible without lighting the far field.
5. **Retire `node_impostors` 3-per-node**; `mass_level` is replaced by
   rank-tiering + a per-tier `l` (mass within tier) for size/brightness.

## Non-goals

- No change to `WebDescriptor`, node order, masses, `r_vir`, home
  node, spawn, fly-to, or picking radius.
- No quad impostors / textured sprites: the radial core ramp is done
  inside the point-sprite fragment (arithmetic-only on `gl_PointCoord`)
  — the 256 px point-size clamp remains the near-camera limit, handled
  by the existing near-eye fade.
- No member-galaxy *physics* (no orbits, no time evolution): members
  are a static seeded scatter (`cosmic_web/members` stream).
- No bloom chain change (`bloom-mip-chain`) — this feature sets the
  *inputs* to bloom correctly; the chain widens the halos later.
- No inspector readout change beyond showing the tier letter.

## Users / Stakeholders

- **Player:** one obvious bright hub ahead (the goal), a few secondary
  ones, beads everywhere else — the target's reading order (report §9).
- **Developer (inspector):** tier letter in the node readout; the
  inspector's zoomed-out view shows the same hierarchy.
- UX consulted: yes — goal legibility + the fly-to target must be
  Tier A or B in practice (home node is a **group** at 10^12–10^13,
  i.e. Tier C by mass — see FR6).

## Roles

Author: PO. UX consulted (required if player-facing): yes — the home
node is deliberately small (Local-Group analog); the player's *first
goal* must therefore be the nearest Tier A hub, which the demo already
targets via `strongest_link_from` chains — UX row UX-1 checks the goal
hub is Tier A/B at spawn. ARCHITECT consulted (required if
cross-module): yes — reads `WebField` tracers for member placement
(optional path), no engine change.

## Functional requirements

- FR1 (tiers): `HubTier::of(rank, n) -> A | B | C` with cut-offs
  `rank < ceil(0.01·n)` → A, `< ceil(0.11·n)` → B, else C; constants
  `HUB_TIER_A_FRACTION = 0.01`, `HUB_TIER_B_FRACTION = 0.10`.
  Within-tier level `l = 1 − rank_in_tier / tier_size` (linear by rank,
  not by log-mass).
- FR2 (impostor records): `hub_impostors(&WebDescriptor, origin) ->
  Vec<(pos, color, misc)>` on the existing glow `PointList` format:
  Tier A → 3 sprites, Tier B → 2, Tier C → 1; `misc.z` `kind` gains
  value `2` = "hub core with in-sprite radial ramp" so the glow
  fragment shader applies `mix(white-yellow, orange, smoothstep(0.2,
  0.5, r))` × rim-zero mask for kind 2 only. Total ≈ 60·3 + 600·2 +
  5340·1 ≈ 6700 sprites (was 18 000).
- FR3 (members): `hub_members(&WebDescriptor, seed, origin) ->
  Vec<(pos, color, misc)>`; stream `"cosmic_web/members"`, canonical
  node order; per node `count = tier == A ? 40 + 200·l : tier == B ?
  10 + 20·l : 0`; position = node + `r_vir · u² · dir` with `u` uniform
  and `dir` from two uniforms (arithmetic-only sphere mapping, no trig:
  Marsaglia rejection or `ihalf3` normalised); colour class by a third
  uniform (`< 0.6` pink-red `[1.0, 0.45, 0.50]`, `< 0.9` orange
  `[1.0, 0.70, 0.40]`, else white `[1.0, 0.97, 0.90]`), size 1.5–3 px,
  emissive `1.5 + 1.5·l`, alpha 0.9. Budget cap `MAX_MEMBER_POINTS =
  40_000` with the two-pass scale pattern. Nominal ≈ 60·140 + 600·20 ≈
  20 000.
- FR4 (bloom inputs): Tier A pin `≥ 3.0` premultiplied luminance, core
  `≥ 3.0` at centre; Tier B core `1.5`; Tier C bead `≤ 0.9` (below the
  1.0 threshold after the 0.9 alpha). Band tests on the emitted
  colours.
- FR5 (fog floor): Tier A/B sprites are emitted with `kind` values the
  window snippet recognises for the 0.25 floor (`kind 1` halo, `kind
  2` core, pin as `kind 2` with tiny size); Tier C beads `kind 0` (no
  floor).
- FR6 (goal check): `cosmic_player::spawn()` is unchanged; a test
  asserts that the demo's initial fly-to candidate (the far node of the
  home's strongest link, or the first `plan_fly_to` target) is Tier A
  or B on the nominal seed **or** that UX-1 documents the alternative
  (highlight ring carries the goal). No spawn logic change in this
  feature.
- FR7 (inspector readout): node readout line gains `tier A|B|C`.

## Non-functional requirements

- NFR1 (budgets): points −18 000 + 6 700 + ≤ 40 000 → net ≤ +29 000
  points vs today (< 1 MB); no new draw or pipeline (glow `PointList`
  with a new `kind` branch); fragment branch on `kind` is a `mix` on a
  uniform-per-vertex flag — no divergent texture reads.
- NFR2 (determinism): tiering is a pure function of rank; members are
  replay-identical under the `members` stream in canonical order;
  translation-invariant except `pos`.
- NFR3 (shaders): fragment arithmetic-only; rim-zero mask preserved;
  `cosmic_shader_safety_pins` extended with the `kind 2` ramp literal.
- NFR4 (invariants): picking unchanged (`select_at` over `web.nodes`,
  radius `COSMIC_PICK_RADIUS_PX`); marker via `world_to_pixels`;
  projection un-flipped; bloom write-once untouched.
- NFR5 (upload): `hub_members` at nominal ≤ 20 ms.

## Definition of Done

1. Shots `slab-after.png`, `inspector-after.png`, `demo-after.png`:
   ANALYST counts ≤ 15 "blazing" nodes at the slab framing (report §3:
   1 central + 1 complex + ~6 secondary + a few), members visible as
   a pink/orange scatter around Tier A cores, Tier C junctions read as
   small warm beads.
2. Sprite count ≈ 6 700 impostors + ≤ 40 000 members in the headless
   `cosmic_layout=` line; `node_impostors` (3-per-node) and
   `mass_level` gone.
3. Tests: tier cut-offs; within-tier level monotone; bloom-input bands
   per tier; member counts per tier, all inside `r_vir`, class
   proportions ±10 %, replay + translation invariance; goal-tier test
   (FR6); shader pin for the `kind 2` ramp.
4. Selection + fly-to highlight behave exactly as v0.3.2 (existing
   tests green); readout shows tier letter.
5. Docs: `rendering.md` hub paragraph replaces the 3-layer impostor
   text; `quality.md` point budget note; techstack version bump; links
   resolve.
6. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Depends on `cosmic-tracer-splat` (class B/D exist so the hub scatter
  reads as class A/C on top) and `cosmic-depth-window` (fog floor
  `kind` contract). Fifth feature on branch `v0.3.3`.
- Assumes the descriptor's node order is mass rank (true since the
  mass-rank fix; pinned by `masses_decrease_with_density_rank`).
- Tier fractions and member counts are starting values; tuned from
  shots and recorded as constants.

## Open questions

- Should member positions prefer nearby **tracers** from `WebField`
  (physically consistent) over a synthetic NFW scatter? Cheaper and
  deterministic to synthesise; tracers within `r_vir` are ≈ 1–8 per
  node at 4 Mpc cells — too few. Decision: synthetic (recorded).
- Tier A fraction 1 % (≈ 60) vs. an absolute count (e.g. 40): a
  fraction survives `target_node_count` changes; fraction chosen.
