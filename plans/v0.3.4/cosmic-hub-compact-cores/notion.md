# Notion — cosmic-hub-compact-cores

## Status

`planned` (PO sign-off 2026-09-23; UX consulted; ARCHITECT consulted —
debug-only, ADR-026 §5)

## Context

`cosmic-hub-hierarchy` (v0.3.3) gave hubs three tiers, in-sprite
radial ramps and a member scatter — but on every shipped shot the
hubs are **large uniform soft discs** (`vista-after.png`: ≈ 60–130 px
blobs; `slab-after-march.png`: ≈ 20–40 px discs), and the member
scatter is barely visible. The reference (report §3) has compact,
hard bright cores (≈ 6–12 px at its framing) with a soft bloom halo
and **hundreds of discrete member dots** — the members *are* the
visual mass of a cluster; the core is a point.

Causes: the Tier A / B halo layer is world-sized (`1.5–2.5 · r_vir`,
kind 1) and the core sprite scales with `l`; bloom then widens
whatever is above threshold, so a 20 px core becomes a 60 px disc;
member counts (`40 + 200·l` for A) are too low and too dim to read
against the halo.

## Problem & Needs

- **Reading order** (report §9): the eye should land on a bright
  *point* ringed by specks, not on a fog ball.
- **Scale honesty:** a cluster is a few Mpc across in a 500 Mpc web;
  at the slab framing that is ≈ 5–10 px, not 60.
- **Navigation:** the goal hub must stay legible as a point at every
  distance; today it is the largest blob, which reads as "near", not
  "important".

## Goals

1. **Pixel-capped cores.** Tier A: pin ≤ 4 px + core ≤ 10 px at any
   distance beyond `r_vir` (px cap, not world size); Tier B core
   ≤ 6 px; Tier C bead 2 px (unchanged). The near-eye fade handles
   the inside-`r_vir` case as today.
2. **Members are the mass.** Tier A `150 + 250·l` members, Tier B
   `30 + 40·l`, sizes 1.5–2.5 px, emissive raised so they read
   individually against the F4 grading; NFW-like profile kept;
   `MAX_MEMBER_POINTS` raised to 100 000 (≈ 3.2 MB) with the two-pass
   cap.
3. **Halo from bloom, not geometry.** The world-sized kind-1 halo
   layer retires; the bloom chain widens the capped core into a soft
   halo whose measured radius is ≤ 3× the core radius at `slab`.
4. **Suffusion, not disc.** The pink/red tint around Tier A cores
   (report §3) comes from the member scatter's colour mix, plus a
   very low-alpha kind-1 sprite of `1.0·r_vir` only where `r_vir`
   projects ≥ 12 px (so it never becomes a disc from afar).
5. **Blazing count:** ≤ 10 nodes above the bloom threshold at the
   `slab` framing (was ≤ 15), consistent with report §3.

## Non-goals

- No change to tiers, `HubTier::of`, node order, picking, fly-to,
  highlight ring, or spawn.
- No quad impostors / textures; still `PointList` + `gl_PointCoord`
  arithmetic.
- No bloom-chain change (inputs only, as in v0.3.3).
- No engine change.

## Users / Stakeholders

- Player: the goal is a bright point with a swarm, visible as a point
  from far and as a swarm when near.
- Developer: `cosmic_layout=` prints member count; inspector readout
  unchanged (tier letter).
- UX consulted: yes — UX-1 (point + swarm reading), UX-2 (goal hub
  legible at spawn distance and at 10 Mpc).

## Roles

Author: PO. UX consulted (required if player-facing): yes. ARCHITECT
consulted (required if cross-module): yes — debug-only; rebase job
(`cosmic-rebase-async`) carries the larger glow buffer.

## Functional requirements

- FR1 (px cap): glow vertex shader clamps hub core / pin point size
  to per-kind px maxima (`HUB_PIN_PX = 4`, `HUB_CORE_A_PX = 10`,
  `HUB_CORE_B_PX = 6`) after the perspective scale; CPU mirror for
  tests.
- FR2 (members): counts per FR/Goal 2; `MAX_MEMBER_POINTS =
  100_000`; emissive `2.0 + 2.0·l`; alpha 0.95; colour classes
  unchanged (60 / 30 / 10 %).
- FR3 (halo retirement): kind-1 world-sized halo removed for Tier
  A/B; optional low-alpha `1.0·r_vir` suffusion sprite emitted only
  when a CPU projection estimate at the demo's nominal distance gives
  ≥ 12 px — otherwise omitted (deterministic, pure).
- FR4 (bloom inputs): Tier A pin / core ≥ 3.0 premultiplied
  luminance (unchanged), Tier B 1.5, Tier C ≤ 0.9, members ≤ 0.9
  individually (they bloom only where they stack).
- FR5 (counts): headless `cosmic_layout=` prints `hubs=` and
  `members=`; nominal ≈ 60·275 + 600·50 ≈ 46 500 members.

## Non-functional requirements

- NFR1 (budget): glow buffer ≤ 120 000 points (≈ 3.8 MB); rebase job
  build ≤ 40 ms on the worker (off-frame, so not a frame budget).
- NFR2 (invariants): picking / marker / projection / write-once
  untouched.
- NFR3 (determinism): members replay-identical (`cosmic_web/members`
  stream, canonical order).

## Definition of Done

1. `slab-after.png`: largest hub core disc ≤ 12 px diameter before
   bloom (pre-bloom capture or HDR scan), bloom halo radius ≤ 3× core;
   ≤ 10 blazing nodes; members resolved as discrete dots around ≥ 3
   Tier A hubs (ANALYST count on a 64 px crop) — numbers recorded.
2. `demo-after.png` at spawn: goal hub reads as a point + swarm; at
   10 Mpc (timed capture) the swarm fills ≤ 25 % of the frame height.
3. Tests: px-cap mirror; member counts per tier and cap; bloom-input
   bands; halo-retirement pin (`rg` for the old halo literal = 0);
   suffusion emission rule.
4. Docs: `rendering.md` hub paragraph, `quality.md` glow budget row,
   techstack bump; links resolve.
5. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Fifth feature; depends on `cosmic-void-contrast` (grading the
  members read against) and `cosmic-rebase-async` (the glow buffer
  travels through the job).
- Starting values are tuned from shots and recorded as constants.

## Open questions

- Member positions from nearby GPU tracers instead of a synthetic
  NFW scatter? Still synthetic (tracers now live on the GPU; a
  readback would break the "no CPU tracer work" rule). Recorded.
