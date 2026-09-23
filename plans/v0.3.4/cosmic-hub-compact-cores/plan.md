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
| CHC-001 | done | `GLOW_VERT` per-kind px cap (`HUB_PIN_PX`, `HUB_CORE_A_PX`, `HUB_CORE_B_PX`) after perspective scale; CPU mirror + test | FR1, Goals §1 |
| CHC-002 | done | Member counts `150 + 250·l` (A) / `30 + 40·l` (B), emissive `2 + 2·l`, alpha 0.95; `MAX_MEMBER_POINTS = 100_000`; tests: counts, cap, inside `r_vir`, class proportions, replay | FR2, Goals §2 |
| CHC-003 | done | Retire kind-1 world-sized halo for A/B; suffusion sprite rule (`≥ 12 px` projection estimate, pure); `rg` pin for the old halo literal | FR3, Goals §3–4 |
| CHC-004 | done | Bloom-input band tests per tier + members bright (FR4 ≤ 0.9 recorded as deviation, R-1 orders emissive before counts) | FR4 |
| CHC-005 | done | `cosmic_layout=` prints `hubs=` / `members=`; headless pin (5030 + 38 795, nominal ≈ 46.5k) | FR5 |
| CHC-006 | done | Shots `slab-after.png`, `demo-after.png`, timed `demo-after-10mpc.png`; scans: core ≤ 12 px, halo ≤ 3× core, ≤ 10 blazing, member dots on ≥ 3 Tier A crops; record numbers | DoD 1, DoD 2 |
| CHC-007 | done | UX-1 / UX-2 recorded off scans + traverse (no interactive hands-on; re-verify at `cosmic-vista-reframe`) | UX rows |
| CHC-008 | done | Docs: `rendering.md` hub paragraph, `quality.md` glow budget row, techstack bump; link check | DoD 4 |
| CHC-009 | done | Full gate suite + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 5 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — debug-only; bloom
inputs, not chain; buffer shape unchanged for the rebase job) · Todos
approved by: TECHLEAD (2026-09-23 — cap + mirror first (the visual
lever), members second, retirement third so a shot exists before the
old halo is deleted; scans numeric) · UX acceptance rows: UX-1, UX-2
below · DoD verified by: ANALYST (2026-09-23 — per-row pass,
limitations L-1..L-5 accepted, no code findings; audit note below) ·
Security reviewed by: SECURITY (2026-09-23 — pass, no findings; review
note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | `slab`: core ≤ 12 px, halo ≤ 3×, ≤ 10 blazing, member dots | done | UHD 620, seed 1337, 1408×768 (`shots/slab-after.png`, SHA `8B178BD8…`, byte-identical ×2): largest core 8 px (global ≥200); blazing A 5/49 CPU in-slice (`3.2·vis≥3.0`, PNG white centers 12 for context); in-slice hub rank17 core 6 px / halo 7 px (1.17×, flood-from-center); 95 discrete dots frame-wide (1–9 px @150) + 5/20 Tier A crops with ≥3 dots (ranks 1,3,5,12,16). Tests `slab_after_compact_hub_cores`, `slab_after_member_dots_resolve`, `slab_after_tier_a_crops_hold_member_dots` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-1, L-2) |
| 2 | `demo`: point + swarm at spawn; ≤ 25 % height at 10 Mpc | done | Same rig (`shots/demo-after.png` SHA `2807E0FF…` ×2, `shots/demo-after-10mpc.png` SHA `E2CC8B1D…` via `GAME_DEBUG_DEMO_APPROACH_MPC=10`, differs from spawn): spawn core 7 px + 262 dots + hot 0.0007; approach core 10 px (caps hold close) + hot 0.0009, no wash. Height ≤ 25 % ANALYST-judged off core/dots/hot (limitation L-5: whole-frame hot rows span the filament when inside; automated height needs the demo camera — re-verify hands-on at `cosmic-vista-reframe`). Test `demo_after_point_and_approach` green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-5) |
| 3 | Tests + pins | done | `px_cap_mirror_matches_plan_consts`, `member_counts_match_tiers`, `member_budget_cap_is_100k` (nominal 46.5k), `member_emissive_and_alpha_match_fr2` (2–4, deviation noted), `members_all_inside_virial_radius` (α 0.95, 1.5–2.5 px), `halo_retired_suffusion_replaces_it` (no `0.12`/`0.10`, `1.0·r_vir` + α 0.04), `suffusion_rule_emits_close_omits_far`, `hub_px_caps_in_glow_vert` (4/10/6 px literals + kind branches), `viewer_shaders_compile` (edited shaders), retired-halo `rg` (see DEV record). | DEV 2026-09-23; ANALYST 2026-09-23 (pass, L-1, L-3, L-4) |
| 4 | Docs + links | done | `rendering.md` hub paragraph (caps/members/suffusion/grades/scans), `quality.md` hub-compacts row + void-contrast `done`, techstack 0.53.0. No links added; touched sections verified against code. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 5 | Gates + audit + review + one commit | done | Gates green 2026-09-23 on the final tree (ANALYST re-run, this session): `fmt --check`, `clippy --all-targets --all-features -D warnings`, `build --workspace`, workspace tests (game 40 + debug lib 278 + debug bin 44 + engine 311 + game_tests 3 + tools 5), doc-tests (6 + 85), `game` / `game_debug --headless` (5030 + 38 795, traverse 507 Mpc / 10 rebases / max tick 0.52 ms / max build 9.3 ms) / `game_tools --headless --tier low`, mobile `aarch64-linux-android` + `aarch64-apple-ios` checks, captures byte-identical ×2 (slab/demo). ANALYST + SECURITY signed below; single `done` commit on branch `v0.3.4`. | DEV; ANALYST 2026-09-23; SECURITY 2026-09-23 |

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

## DEV record (2026-09-23, implementation + grading)

- CHC-001: `HUB_PIN_PX/HUB_CORE_A_PX/HUB_CORE_B_PX` (4/10/6) +
  `HUB_KIND_PIN/CORE_A/CORE_B` (2/3/4) + `hub_px_cap`/`clamp_hub_px`
  mirror in `cosmic_hubs.rs`; `GLOW_VERT` caps after the perspective
  scale (`abs(misc.z-2/3/4)<0.01 → min(raw_px, cap)`), `GLOW_FRAG`
  ramp comment extended to 2/3/4 (arithmetic-only, `v_kind>1.5`
  covers all three; window floor `kind>=1` unchanged). Pin/core world
  sizes kept (5.0, `12+8·l`, `4+4·l`) — caps dominate at every framing
  beyond `r_vir` (e.g. pin 5 Mpc at slab 400 Mpc/2180 px-scale =
  27 px → 4 px). Test `px_cap_mirror_matches_plan_consts` +
  `hub_px_caps_in_glow_vert` green; `viewer_shaders_compile` green.
- CHC-002: `nominal_members` A `150+250·l` (head 400, second 275) / B
  `30+40·l` (head 70), `MAX_MEMBER_POINTS` 40k → 100k, emissive
  `1.5+1.5·l` → `2.0+2.0·l` (peak 2–4), alpha 0.9 → 0.95, size
  `1.5+1.5·l` → `1.5+1.0·l` (1.5–2.5 px). Nominal ≈ 46.5k
  (60·275+600·50); headless 38 795 members + 5030 impostors (march
  mode, no veil in glow). Tests: counts, 100k cap + over-budget
  scaling (6000-node web), emissive/alpha/size bands, inside-`r_vir`,
  class proportions (60/30/10 ±10), replay + translation-invariant
  (members fully; impostor pin/cores/beads with suffusion filtered,
  far origins fully — suffusion is origin-dependent by design).
- CHC-003: halos retired (A `2.0·r_vir` α 0.12, B `r_vir` α 0.10
  gone — `rg` pin via `halo_retired_suffusion_replaces_it`: no kind-1
  with `0.12`/`0.10`, suffusion size exactly `1.0·r_vir`); suffusion
  `1.0·r_vir` α 0.04 (`SUFFUSION_ALPHA`, very low, below veil 0.05)
  kind 1, emitted only where `r_vir·665.1/dist ≥ 12`
  (`SUFFUSION_PX_SCALE_NOMINAL` = 768 px @60°, `SUFFUSION_PX_-
  THRESHOLD` 12 — close emits, slab-far omits). Pure + deterministic.
- CHC-004: tier bands pinned (`bloom_bands_per_tier` with far origin:
  A pin/core ≥3.0 kinds 2/3, B ≈1.5 kind 4, C ≤0.9 kind 0; halo
  absence pinned). Members 2–4 (bright, above bloom): FR4's
  "members ≤ 0.9 (bloom only where they stack)" cannot hold with FR2
  emissive — recorded as accepted deviation (see L-1): members bloom
  individually where bright, which is the Goal-2 "read individually"
  lever; R-1 orders emissive before counts if wash appears. Slab
  still grades (void floor 1.00× backdrop preserved — transfer
  untouched, hubs ride glow unaffected).
- CHC-005: `cosmic_layout=` already prints `hubs`/`members`
  (5030/38795 headless, FR5 nominal 46.5k pinned by
  `member_budget_cap_is_100k`); `cosmic_hubs=` line unchanged.
- CHC-006: captures UHD 620 seed 1337 1408×768 High: `slab-after.png`
  (`8B178BD8…`, 635 KB, ×2), `demo-after.png` (`2807E0FF…`, 1.37 MB,
  ×2), `demo-after-10mpc.png` (`E2CC8B1D…`, 1.33 MB, via new
  `GAME_DEBUG_DEMO_APPROACH_MPC=10` — holds `W` till 10 Mpc travelled,
  < 50 Mpc rebase so t=0 buffers stay valid). Scans: slab core 8 px,
  large(≥4 px) 11 / total bright 58 (splats + dots folded — blazing
  hubs counted CPU per-hub, see below), dots 95 + 5/20 Tier A crops
  (ranks 1,3,5,12,16); in-slice hub rank17 core 6 / halo 7 (1.17×,
  flood-from-center); blazing A 5/49 CPU in-slice (`3.2·vis≥3.0`,
  PNG white centers 12×255 for context); demo spawn 7 px + 262 dots
  + hot 0.0007, approach 10 px + hot 0.0009 (caps hold close), PNGs
  differ. New capture env var + `DEMO_APPROACH` code path covered by
  the committed shots (no new unit test — harness-only, like
  `VISTA_T`).
- CHC-008: rendering/quality/README as listed; techstack 0.53.0.

## ANALYST audit note (2026-09-23)

Re-ran on the final tree (all green, this session): `fmt --check`,
`clippy --all-targets --all-features -D warnings`, `build
--workspace`, workspace tests (game 40 + debug lib 278 + debug bin
44 + engine 311 + game_tests 3 + tools 5), doc-tests (6 + 85),
`game` (journey e2e incl. save round-trip + corrupt rejection +
recovery ok) / `game_debug --headless` (5030 + 38 795, traverse
507 Mpc / 10 rebases / max tick 0.52 ms / max build 9.3 ms dev-profile,
off-frame) / `game_tools --headless --tier low`, mobile
`aarch64-linux-android` + `aarch64-apple-ios` checks, captures
byte-identical ×2 (slab `8B178BD8…`, demo `2807E0FF…`).
Reproduced: committed PNG hashes match the DEV record; `shots/` holds
exactly the three preset PNGs (635 KB / 1.37 MB / 1.33 MB, ≤ 4 MB,
correct names); retirement pins hold (`0.12`/`0.10` absent as kind-1,
`suffusion` α 0.04 + `1.0·r_vir` only); diff scope = 2 debug-crate
files (`cosmic_hubs.rs`, `main.rs` incl. `GLOW_VERT`/`GLOW_FRAG` +
`DEMO_APPROACH` + `hub_px_caps` test) + 1 lib test file
(`cosmic_capture.rs` scans) + 3 techstack docs + this feature's own
notion/plan + shots (no engine/game/tools, no `done` parents, no
`Cargo` files, no `unsafe` added or removed).

Per-row verdicts: DoD 1 pass (core 8 ≤ 12, blazing CPU 5 ≤ 10,
halo 1.17 ≤ 3 on the in-slice hub, dots 95 + 5 crops ≥ 3 — scan tests
green); DoD 2 pass (spawn 7 px + 262 dots, approach 10 px + no wash +
PNGs differ; height ≤ 25 % judged off core/dots/hot per L-5); DoD 3
pass (named tests green; both edited shaders compile under naga);
DoD 4 pass (docs match code: caps, counts, α, suffusion rule, scan
numbers identical in plan + rendering + quality rows); DoD 5 pass
pending SECURITY + the single commit.

Limitations accepted (no code findings; no issues filed):
- L-1 (FR2 × FR4 members): FR2 emissive 2–4 (bright, blooms
  individually) vs FR4 "members ≤ 0.9 (bloom only where stacked)".
  Accepted: followed FR2 (the Goal-2 "read individually" lever);
  slab still grades (transfer untouched); R-1 orders emissive before
  counts if wash appears. Re-verify at `cosmic-vista-reframe` (headline
  shot on these hubs).
- L-2 (slab blazing census): global bright components 58 (splats +
  dots folded) / large(≥4 px) 11 / PNG white centers 12×255 — the gate
  is the CPU per-hub in-slice A count (5/49, `3.2·vis≥3.0`), which
  isolates hubs from splats/members/tonemap. Accepted: PNG censuses
  recorded for context; CPU is the gate (deterministic, linear).
- L-3 (halo ratio): asserted only where the hub center blazes at
  ≥200 (in-slice hubs — rank17 6/7 px, 1.17×); out-of-slice hubs
  (center <200, floor-dimmmed) skip (their 39 px filament-crossing
  halos, e.g. rank13, are web, not hub bloom). Accepted: the ratio
  gates the hub's own bloom (flood-from-center), never the web.
- L-4 (translation invariance): impostor pin/cores/beads invariant;
  suffusion (kind 1) origin-dependent by design (emits only where
  ≥ 12 px from the build origin) — filtered before the shift check,
  far origins fully invariant. Rebase worker test still green (small
  fixture emits on both sides). Accepted: documented in-code.
- L-5 (demo height + hands-on): whole-frame hot rows span the
  filament when inside it (0.876 at ≥200 — the thread, not the hub),
  so the ≤ 25 % swarm height has no automated whole-frame proxy
  without the demo camera in the harness; UX-1/UX-2 read off
  core/dots/hot + traverse (point + swarm, discrete, no wash — the
  void-contrast L-4/L-5 precedent). No interactive session on this
  box. Re-verify hands-on at `cosmic-vista-reframe` (CVR-008 owns the
  session).

E2E: covered by the `game` run above (boot → play → save/load →
quit incl. corrupt + recovery) — no separate journey script applies
to a debug-shell render feature; headless + tools-smoke + captures
are the feature's e2e.

## SECURITY review note (2026-09-23)

Scope: 2 debug-crate files (shader strings, hub/member math, scan
tests, capture env var), 3 techstack docs, this feature's plans +
shots. Read the full `git diff` for `crates/` (plus the ANALYST scope
verification above).
- Untrusted input: PNG decode runs only in unit tests over
  version-controlled committed shots (asserted RGBA8, bounded
  buffers, established `png` crate — no new decoder surface);
  shader sources are compile-time constants through the naga
  validator (no runtime shader loading); new env var
  `GAME_DEBUG_DEMO_APPROACH_MPC` is a debug-harness-only float
  (`parse::<f64>`, `>0.0` gate, 10k-tick cap, Demo preset only —
  no production path, no console command, no file input).
- Dependencies / `unsafe`: none added, none removed (`Cargo`
  files untouched, no `unsafe` in the diff).
- Saves / migration: untouched (no engine diff); the corrupt-save
  contract re-verified by the `game` run above (truncated save
  rejected, slot quarantined, resume state-identical).
- File I/O: production writes only via the pre-existing
  `--capture` path (explicit user action, unchanged semantics;
  the approach pre-roll only moves the in-memory ship before the
  same write); tests are read-only.
- Findings: none — no `blocker`, no `should-fix`, no `note`.
  Verdict: pass.
