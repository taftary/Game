# Plan — cosmic-hub-hierarchy

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` only — `cosmic_web.rs` (tiering,
impostor + member records), `main.rs` (`GLOW_FRAG` `kind 2` radial
ramp, window-snippet floor by kind, upload order), inspector readout.
Reads `WebDescriptor` (rank, mass, `r_vir`) — no engine change, no
`WebField` read (synthetic members, recorded). No new pipeline: the
glow `PointList` gains one `kind` value. **Invariant rows:** picking
node-only and unchanged; marker path; projection; bloom write-once;
determinism of the `members` stream.

### Phase 1 — Tiering + impostor records (`cosmic_web.rs`)

`HubTier`, `HubTier::of(rank, n)`, within-tier `l`; `hub_impostors`
replacing `node_impostors` (A: pin + core `kind 2` + halo `kind 1`;
B: core `kind 2` + halo; C: bead `kind 0`); bloom-input band tests;
delete `mass_level`/`node_color`/`node_size_px` or reduce to tier
helpers (whatever `node_point_cloud` test still needs — that legacy
function retires here too).

### Phase 2 — Member scatter (`cosmic_web.rs`)

`hub_members` under `"cosmic_web/members"`: two-pass budget
(`MAX_MEMBER_POINTS = 40_000`), NFW-like `r_vir·u²`, trig-free
direction, class proportions, emissive by `l`. Tests: counts per tier,
inside `r_vir`, proportions ±10 %, replay, translation invariance.

### Phase 3 — Shader + wiring (`main.rs`)

`GLOW_FRAG`: `kind 2` → `mix(pale-yellow, orange, smoothstep(0.2, 0.5,
r))` × rim-zero mask (arithmetic-only); window snippet applies the
0.25 fog floor to `kind ≥ 1`; `upload_cosmic_glow` order: splats
(separate buffer) · veil · members · impostors; headless line prints
`hubs N members M`. Pin extended.

### Phase 4 — Readout, goal check, shots, gates

Inspector readout `tier A|B|C`; FR6 goal-tier test; shots ×3 with the
"≤ 15 blazing nodes" count; docs; gates; audit; single commit.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CHH-001 | pending | `HubTier` + `of(rank, n)` + within-tier `l`; tests: cut-offs at 1 % / 11 %, `l` monotone | Goals §1, FR1 |
| CHH-002 | pending | `hub_impostors` (A 3 sprites, B 2, C 1; `kind 2` cores); retire `node_impostors`, `mass_level`, `node_point_cloud`; bloom-input band tests per tier | Goals §1–2, §5, FR2, FR4 |
| CHH-003 | pending | `hub_members` under `cosmic_web/members`, two-pass cap 40k, NFW-like radius, trig-free direction, class proportions; tests (counts, inside `r_vir`, proportions, replay, translation) | Goals §1, FR3, NFR2, NFR5 |
| CHH-004 | pending | `GLOW_FRAG` `kind 2` radial ramp; window-snippet fog floor for `kind ≥ 1`; extend `cosmic_shader_safety_pins` | FR2, FR5, NFR3 |
| CHH-005 | pending | Upload wiring + headless `cosmic_layout=` prints `hubs N members M`; rebase/reseed paths | DoD 2 |
| CHH-006 | pending | Inspector readout `tier A\|B\|C`; existing select/highlight tests green | FR7, DoD 4 |
| CHH-007 | pending | FR6 goal-tier test on the nominal seed (or documented UX-1 alternative) | FR6 |
| CHH-008 | pending | Shots ×3; ANALYST count of blazing nodes at `slab` (≤ 15), members visible on Tier A | DoD 1 |
| CHH-009 | pending | Docs: `rendering.md` hub paragraph, `quality.md` point note, techstack version bump; link check | DoD 5 |
| CHH-010 | pending | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-20 — debug-only; one new
`kind` value on the existing glow pipeline; descriptor read-only;
picking untouched and pinned) · Todos approved by: TECHLEAD
(2026-09-20 — risk-first: tiering + bloom-input bands (CHH-001/002)
before members; member budget capped at 40k with the two-pass
pattern; net point delta < +29k) · UX acceptance rows: approved
2026-09-20 — UX-1…UX-3 below · DoD verified by: ANALYST _(pending)_ ·
Security reviewed by: SECURITY _(pending)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | ≤ 15 blazing nodes at slab; members visible; C = beads | pending | shots + ANALYST count | ANALYST _(pending)_ |
| 2 | Counts in layout line; legacy impostors gone | pending | headless log; `rg node_impostors\|mass_level` = 0 | ANALYST _(pending)_ |
| 3 | Tests listed green | pending | test names | ANALYST _(pending)_ |
| 4 | Selection/highlight unchanged; tier readout | pending | existing tests + readout screenshot | ANALYST _(pending)_ |
| 5 | Docs + links | pending | file list | ANALYST _(pending)_ |
| 6 | Gates + audit + review + one commit | pending | gate log, commit | ANALYST + SECURITY _(pending)_ |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. `select_at` iterates `web.nodes` only, radius unchanged; members
  and beads never selectable [T: existing pick tests].
- A-2. Marker via `world_to_pixels`; projection un-flipped [T: existing].
- A-3. Bloom chain untouched (write-once) [T: HdrChain diff empty].
- A-4. Fragment arithmetic-only; `kind 2` ramp pinned [T].
- A-5. `hub_members` deterministic per `(seed, web, origin)`; canonical
  node order; translation-invariant except `pos` [T].
- A-6. No engine / `game` / `tools` diff.

UX acceptance rows:

- UX-1. Demo at spawn: the first fly-to goal reads as the brightest
  thing in view (Tier A/B) or carries the highlight ring — one obvious
  destination within 3 s.
- UX-2. Inspector zoomed out: eye lands on one dominant hub, then a
  handful of secondaries; the rest are beads (report §9 reading order).
- UX-3. Clicking a Tier C bead still selects its node and shows the
  readout with `tier C` (no behaviour change).

## Risks & Next steps

- R-1 (home node is Tier C): the Local-Group analog is small by design;
  if the spawn's goal chain does not reach a Tier A/B hub within one
  link, UX-1 falls back to the highlight ring (recorded), and
  `cosmic-vista-intro` frames the nearest Tier A hub explicitly.
- R-2 (point-size clamp): Tier A cores at 20 px hit the 256 px clamp
  only within ~1 Mpc — existing near-eye fade covers it; quad impostors
  stay a roadmap item.
- R-3 (member scatter reads as noise on Low): if 20k members are
  illegible at 1080p, cut Tier B members first (`10 + 20·l → 0`),
  recorded.
- Next: `bloom-mip-chain` widens Tier A halos; `cosmic-vista-intro`
  frames a Tier A hub.
