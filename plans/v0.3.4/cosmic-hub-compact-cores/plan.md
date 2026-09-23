# Plan — cosmic-hub-compact-cores

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_hubs.rs` (counts, cap
constants, suffusion rule, halo retirement), `main.rs` (`GLOW_VERT`
px cap per kind, layout line). Glow buffer grows; it travels through
the `cosmic-rebase-async` job unchanged in shape. **Invariant rows:**
picking / marker / projection untouched; bloom inputs only; fragment
arithmetic-only.

### Phase 1 — Caps + members (`cosmic_hubs.rs`, `GLOW_VERT`)

Px cap per kind in the vertex shader + CPU mirror; member counts /
emissive / cap; tests.

### Phase 2 — Halo retirement + suffusion

Remove kind-1 world halo for A/B; suffusion rule; `rg` pin.

### Phase 3 — Shots + scans + UX

`slab` / `demo` / timed 10 Mpc capture; core / halo / blazing /
member-dot scans; UX rows.

### Phase 4 — Docs + gates

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CHC-001 | pending | `GLOW_VERT` per-kind px cap (`HUB_PIN_PX`, `HUB_CORE_A_PX`, `HUB_CORE_B_PX`) after perspective scale; CPU mirror + test | FR1, Goals §1 |
| CHC-002 | pending | Member counts `150 + 250·l` (A) / `30 + 40·l` (B), emissive `2 + 2·l`, alpha 0.95; `MAX_MEMBER_POINTS = 100_000`; tests: counts, cap, inside `r_vir`, class proportions, replay | FR2, Goals §2 |
| CHC-003 | pending | Retire kind-1 world-sized halo for A/B; suffusion sprite rule (`≥ 12 px` projection estimate, pure); `rg` pin for the old halo literal | FR3, Goals §3–4 |
| CHC-004 | pending | Bloom-input band tests per tier + members ≤ 0.9 | FR4 |
| CHC-005 | pending | `cosmic_layout=` prints `hubs=` / `members=`; headless pin | FR5 |
| CHC-006 | pending | Shots `slab-after.png`, `demo-after.png`, timed `demo-after-10mpc.png`; scans: core ≤ 12 px, halo ≤ 3× core, ≤ 10 blazing, member dots on ≥ 3 Tier A crops; record numbers | DoD 1, DoD 2 |
| CHC-007 | pending | UX-1 / UX-2 recorded | UX rows |
| CHC-008 | pending | Docs: `rendering.md` hub paragraph, `quality.md` glow budget row, techstack bump; link check | DoD 4 |
| CHC-009 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 5 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — debug-only; bloom
inputs, not chain; buffer shape unchanged for the rebase job) · Todos
approved by: TECHLEAD (2026-09-23 — cap + mirror first (the visual
lever), members second, retirement third so a shot exists before the
old halo is deleted; scans numeric) · UX acceptance rows: UX-1, UX-2
below · DoD verified by: ANALYST _(pending)_ · Security reviewed by:
SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `slab`: core ≤ 12 px, halo ≤ 3×, ≤ 10 blazing, member dots | pending | | |
| 2 | `demo`: point + swarm at spawn; ≤ 25 % height at 10 Mpc | pending | | |
| 3 | Tests + pins | pending | | |
| 4 | Docs + links | pending | | |
| 5 | Gates + audit + review + one commit | pending | | |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `select_at` / highlight ring / marker paths unchanged [T:
  existing tests].
- A-2. Glow fragment arithmetic-only; kind-2 ramp literal pinned [T].
- A-3. No bloom-chain / pass change [T: pass-description pin].

UX acceptance rows:

- UX-1. At `slab`, a Tier A hub is a bright point with a visible
  swarm of coloured specks, not a disc.
- UX-2. From spawn, the goal hub is a legible point; approaching to
  10 Mpc it resolves into a swarm without filling the screen.

## Risks & Next steps

- R-1 (member wash): 46k members at 0.95 alpha can bloom as a blob
  where they stack; lower member emissive before touching counts
  (recorded order).
- R-2 (px cap vs near-eye): inside `r_vir` the cap must release or
  the core vanishes; keep the existing near-eye fade authority.
- Next: `cosmic-vista-reframe` composes the headline shot on these
  hubs.
