# Notion — cosmic-sphere-clip

## Status

`done` (PO sign-off 2026-09-23; ARCHITECT consulted — hashed path,
ADR-026 §1; scope extension 2026-09-23 — vista dive-exact hub
shortlist folded in per PO decision, `cosmic-vista-reframe` CVR-004
dive core; ANALYST audit + SECURITY review recorded in `plan.md` DoD
table; single commit on branch `v0.3.4`)

## Context

Every cosmic surface shows web nodes drawn **outside** the 250 Mpc
descriptor sphere (visible on
[`../../v0.3.3/cosmic-gas-veil-v2/shots/slab-after-march.png`](../../v0.3.3/cosmic-gas-veil-v2/shots/slab-after-march.png)
and `inspector-after-march.png`: hub sprites well beyond the veil's
limb). Root cause (investigation 2026-09-23): `descriptor_radius_mpc`
is a camera-extent / validation value only. Stage C peak extraction
(`crates/engine/src/universe/web/classify.rs`, `border_cells() = 0`)
scans the whole 128³ periodic box and `assemble()`
(`descriptor.rs`) converts every accepted peak to box-centred Mpc
with no radial test. Tracers (`field_export.rs`), veil sprites
(`cosmic_veil.rs`) and the raymarch (`MARCH_FRAG` ray–sphere) **are**
cut to the sphere, so the corner shell of the cube — ≈ 51 % of the
box volume — holds nodes, links, glow, hub impostors and members with
no field around them. Only the home node is pinned inside the sphere
today (`mod.rs` `home outside sphere` assert).

This was deferred by PO decision in v0.3.2
(`../../v0.3.2/cosmic-scale-player/update-2026-09-18-2328/notion.md`
"cubic-boundary treatment: clip vs fade"). The PO decision of
2026-09-23 is **generation-side clip**: gameplay (selection, fly-to,
home, spawn) and render must agree on what exists.

## Problem & Needs

- **Player:** a fly-to target or selected node must always sit inside
  the visible web; today ≈ half the candidates are in empty space
  beyond the veil.
- **Developer:** the inspector's "500 Mpc sphere framed with margin"
  (`MapOrbitCamera::new(…, 250.0)`) assumes content inside 250 Mpc;
  that assumption is false, so framing and picking radius reasoning
  is off.
- **Docs honesty:** `WebNode::position_mpc` is documented as
  "relative to the descriptor centre" with "relative precision at
  250 Mpc"; positions actually reach ≈ 440 Mpc.

## Goals

1. Every emitted `WebNode` satisfies `|position_mpc| ≤
   descriptor_radius_mpc`; links and glow inherit the bound because
   they derive from nodes.
2. The descriptor change is versioned: `UNIVERSE_VERSION` 3 → 4,
   committed web vectors and calibration bands re-recorded, so the
   determinism contract is kept explicitly, never silently.
3. Calibration stays inside its bands (void fraction, void diameters,
   filament lengths, halo mass function, home-node band); if node
   density inside the sphere pushes a band, `target_node_count` is
   retuned and the value recorded.
4. Downstream render code loses its implicit dependence on
   out-of-sphere nodes: hub / member / link derivations gain radius
   tests.

## Non-goals

- No edge *fade* on the field (that is `cosmic-void-contrast`'s rim
  fade — render-side, non-hashed).
- No change to the periodic lattice, the sphere radius, the box size,
  or the stage A/B math.
- No render-side node filtering (the engine is the single authority).
- No save migration code beyond what the `UNIVERSE_VERSION` path
  already does (ADR-004; precedent `cosmic-web-mass-rank-fix`).

## Users / Stakeholders

- Player (Game Demo): targets exist where the web is.
- Developer (inspector / Dimensions Universe tab): the framed sphere
  is the content bound.
- ANALYST: one pinned invariant instead of a per-shot judgement.

## Roles

Author: PO. UX consulted (required if player-facing): no — behaviour
fix, no new interaction. ARCHITECT consulted (required if
cross-module): yes — hashed engine path, version bump (ADR-026 §1).

## Functional requirements

- FR1 (cut): a peak is rejected when its **refined** Mpc position
  (the same `to_mpc` used for acceptance) has `x² + y² + z² > r²`,
  before greedy separation / `target_node_count` truncation, so the
  cap applies to in-sphere peaks only.
- FR2 (version): `UNIVERSE_VERSION = 4`; `committed_web_vectors_pin_stage0`
  and every other pinned vector updated in the same feature; no other
  hashed change rides along.
- FR3 (bands): existing calibration tests stay green on the nominal
  seed set; any retuned constant is recorded in `plan.md` with
  before/after numbers.
- FR4 (pins): a new engine test asserts all nodes ≤ radius on the
  nominal seeds; `hub_impostors`, `hub_members`, `link_segments` get
  radius tests (`≤ r + r_vir` for members).
- FR5 (docs): `descriptor.rs` doc comments and
  `docs/reports/2026-09-19-cosmic-web-visual-architecture.md` §
  parameter table say "generation cut", not "camera extent".

## Non-functional requirements

- NFR1 (determinism): integer decisions + `sqrt`-only on the hashed
  path; the radial test is `r² ≤ R²` in `f64`, no `sqrt` needed.
- NFR2 (cost): the cut adds one comparison per peak candidate; boot
  cost unchanged within noise.
- NFR3 (blast radius): `engine::universe::web` only; `game`, `tools`,
  debug shaders untouched except the tests in FR4.

## Definition of Done

1. Engine test: every `WebNode` of the nominal seed set has
   `|position_mpc| ≤ descriptor_radius_mpc`; hub / member / link
   radius tests green.
2. `UNIVERSE_VERSION = 4`; pinned vectors re-recorded; full
   determinism suite green.
3. Calibration bands green; any retune recorded with numbers.
4. `slab-after.png` and `inspector-after.png`: no hub sprite outside
   the veil limb (ANALYST pixel check on the corner regions).
5. Docs: `descriptor.rs` comments, visual-architecture report table
   row, `docs/game/universe.md` sphere sentence, techstack version
   bump; links resolve.
6. Full gate list green; ANALYST + SECURITY signed; single `done`
   commit on branch `v0.3.4`.

## Constraints & Assumptions

- First feature of the version so that every later evidence shot is
  taken on the bounded descriptor.
- Assumes the sphere / box volume ratio (≈ 0.49) means roughly half
  the previous peaks drop before the `target_node_count` cap; the cap
  then admits deeper in-sphere peaks, raising in-sphere node density.
- Saves from `UNIVERSE_VERSION 3` regenerate per the existing path.

## Open questions

- Keep `target_node_count = 6 000` (denser in-sphere web, closer to
  the reference's bead density) or scale it by the volume ratio to
  preserve today's density? Decided by the calibration bands in FR3;
  recorded, not guessed.
