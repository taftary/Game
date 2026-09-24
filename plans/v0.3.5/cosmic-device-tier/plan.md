# Plan — cosmic-device-tier

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

ARCHITECT notes: `crates/debug` lib (`cosmic_tier.rs` pure mapping +
`Action::CycleTier` + `App.cosmic_tier` display state) + bin (boot
selection, knob threading, `F4` handler, `--tier` capture pin) +
`engine` untouched (consumes `QualityTier`, `SplatK::for_tier`,
`VeilMode::for_tier_*`, `MipBloomParams::for_tier` as-is — the three
tier contracts already agree on Low/Medium/High). No new GPU
resource (the F1 pool is reused for measurement); the cycle rebuild
reuses `refresh_cosmic_seed` (buffers + worker) and the
swapchain-recreate HDR path (levels). **Invariant rows:** bloom
write-once at 3/4/5 levels; capture byte-identity at default High;
`F4` never shadows content keys; `--headless` untouched; no
`engine`/`game`/`tools` diff.

### Phase 1 — Lib mapping + registry (CDT-001, CDT-004a)

`cosmic_tier.rs` (pure, unit-tested) + `Action::CycleTier` with the
pinned registry test at 36 static. Risk-first: the mapping is the
contract every later todo consumes.

### Phase 2 — Bin threading (CDT-002, CDT-003)

Boot selection → `splat_k`/`veil_mode`/bloom params; `F4` handler
with rebuilds; FPS-tab tier row; `App.cosmic_tier` display state.

### Phase 3 — Capture pin + measure + docs + gates (CDT-004b, CDT-005, CDT-006)

`--tier` parse + offscreen threading, Medium-vs-High table,
doc updates, full gates, audit, review.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| CDT-001 | done | `crates/debug/src/cosmic_tier.rs`: `CosmicTier { tier, source }`, `auto_tier(device_type)` (Cpu/Virtual→Low, Integrated→Medium, Discrete→High, wildcard→Medium), `splat_k_for`/`veil_for`/`next_tier` on the engine contracts (no second vocabulary — `FromStr`/`name`/`all` reused); unit tests (mapping, knob contracts, cycle) | FR1, FR2, FR3 |
| CDT-002 | done | Bin boot: `GAME_DEBUG_TIER` else auto from `device.physical_device().properties().device_type`; log adapter + tier + source; thread through `splat_k`, `veil_mode`, `MipBloomParams::for_tier` (windowed) + `build_hdr_chain` levels param (all 4 call sites); `App.cosmic_tier` display state | FR1, FR2, FR3, Goals §1–2 |
| CDT-003 | done | `Action::CycleTier` (`F4`, Chrome) + `F4` handler (cycle, `refresh_cosmic_seed` rebuild, console note, `App.cosmic_tier` update) + FPS-tab tier row + registry test 35→36 static (52 actions) | FR4, FR5 |
| CDT-004 | done | `--tier low\|medium\|high` (default High) parse + usage + offscreen threading (splat_k/veil/bloom on the capture path only); parse tests; default-tier captures byte-identical | FR6 |
| CDT-005 | done | Measured Medium-vs-High `cosmic_timing=` table on UHD 620 (seed 1337, 1408×768, inspector + demo) | Goals §5 |
| CDT-006 | done | Docs (`quality.md` tier rows — shell no longer High-only; `rendering.md` tiers note; techstack bump; milestones F2 done) + full gates + mobile guards + ANALYST audit + SECURITY review; single `done` commit | DoD 6 |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-23 — lib pure mapping,
engine contracts reused untouched, rebuild paths reused not
reinvented, `F4` in Chrome clear of content keys; invariants above) ·
Todos approved by: TECHLEAD (2026-09-23 — mapping + registry first,
threading before surfaces, measure before docs; budgets checked —
Medium is the specified tier, no budget risk; gate commands per
`quality.md`) · UX acceptance rows: n-a (shell-only, grades
pre-approved) · DoD verified by: ANALYST (2026-09-23 — per-row audit below) · Security reviewed by: SECURITY
(2026-09-23 — pass, no findings; review note below).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Boot selects Medium on UHD 620 | done | Windowed boot on this box: `cosmic tier selected (auto) tier="medium" device_type=IntegratedGpu` (Intel UHD 620, driver 1656914). Env pin proven: `GAME_DEBUG_TIER=low` → `cosmic tier selected (env) tier="low"`. | DEV 2026-09-23; ANALYST 2026-09-23 (pass — both log lines reproduced) |
| 2 | Knobs follow tier (~4× win) | done | Same boot path derives `splat_k`/`veil_mode`/`MipBloomParams` levels from the tier (per-knob envs still win — F1's `K=1/2` runs proved the precedence machinery). `--tier medium` inspector: prepass 32.69 (was 119.86–139.63) → total 38.56 vs 124.49 (3.2×). Demo Medium 59.32 vs High 195.92 (3.3×). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 3 | `F4` cycle + rebuilds + display | done (review-verified — L-2) | `cycle_cosmic_tier` recomputes knobs (env-wins) + calls `refresh_cosmic_seed` (veil volume, HDR chain at new levels, glow buffers, worker — the tested seed path) + console note; `F4` key arm + `Action::CycleTier` dispatch both route to it; FPS tab shows `tier: X (source) [F4]`; registry 52 pinned. No interactive keypress drivable in this automation (limitation L-2, same class as F1 L-1) — operator's `F4` check closes it. | DEV 2026-09-23; ANALYST 2026-09-23 (pass with L-2) |
| 4 | `--tier` pin + byte-identity | done | `--tier medium/low` inspector + medium demo all print `tier <name>` and real timings (Low: march 0.00 — sprites path confirmed). Default (no flag) inspector capture byte-identical to pre-feature High (`A5AA81…` ×3 runs across both features). Parse tests (`cli_tier_pin_parsing`) green. | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 5 | Measured table recorded | done | UHD 620, seed 1337, 1408×768, dev: inspector High 124.49 (prepass 119.86 / bloom 1.97 / march 1.93 / main 0.73) → Medium 38.56 (32.69 / 3.28 / 1.74 / 0.85) → Low 44.26 (40.92 / 2.40 / march 0.00 / 0.94; sprites overdraw ≈ march saving on inspector — recorded, not hidden); demo High 195.92 (190.16 / 1.94 / 3.15 / 0.67) → Medium 59.32 (53.07 / 2.45 / 2.83 / 0.97). iGPU run variance ±20% (clocks/thermals — the widget avg/max exists for this). | DEV 2026-09-23; ANALYST 2026-09-23 (pass) |
| 6 | Gates + audit + review + one commit | done | `fmt --check`, `clippy --workspace --all-targets --all-features -D warnings`, `build --workspace`, `test --workspace --all-targets` (40+294+46+311+3+5), `test --doc`, `game` run, `game_debug --headless`, `game_tools --headless --tier low`, mobile android + ios — all green (gate log below). ANALYST + SECURITY signed below; single `done` commit on branch `v0.3.5`. | DEV; ANALYST 2026-09-23; SECURITY 2026-09-23 |

## Acceptance criteria

ARCHITECT invariant rows:

- A-1. Bloom write-once at 3/4/5 levels [`assert_write_once` green] [T].
- A-2. Default-tier captures byte-identical to pre-feature High [T: capture gate ×2].
- A-3. `F4` labels exactly one action; no content key shadowed [T: registry tests].
- A-4. `--headless` loads no Vulkan symbols [T: existing headless run].
- A-5. No `engine` / `game` / `tools` diff.

## Risks & Next steps

- R-1 (cycle mid-frame): handler runs on the event loop between
  frames; rebuilds are the tested reseed/recreate paths — no new
  synchronization. OUTCOME 2026-09-23: `cycle_cosmic_tier` calls
  `refresh_cosmic_seed` unconditionally (covers veil + levels +
  worker in one tested path); review-verified, live keypress pending
  (L-2).
- R-2 (veil switch staleness): OUTCOME 2026-09-23 — closed by
  construction: the cycle always runs the seed rebuild, which
  re-uploads the veil-dependent glow buffers and recreates the
  worker with the current `self.veil_mode`.
- L-2 (no interactive `F4` keypress in this automation): the handler
  shares `cycle_cosmic_tier` with the Controls-dispatched
  `Action::CycleTier` path; both arms review-verified. Operator
  presses `F4` on their box and watches the FPS-tab tier row flip
  (Low → Medium → High).
- Next (post-v0.3.5): `cosmic-splat-culling`, `cosmic-splat-bricks`,
  `cosmic-fill-levers` — all consume F1 numbers + this tier switch.

## ANALYST audit note (2026-09-23)

- Re-ran: lib suite (cosmic_tier 3/3, actions 5/5 incl. registry 52,
  frame_timing 8/8), bin suite 46/46 (incl. `cli_tier_pin_parsing`),
  4 `--capture` runs (default byte-identical `A5AA81…`, medium,
  low with march 0.00, demo medium) all printing tier +
  `cosmic_timing=`; 2 windowed boots (auto→medium, env→low log
  lines reproduced).
- DoD 3: code-reviewed (L-2 recorded). E2E: n-a (debug-only
  tooling, no player journey, no save format).
- Verdict: all rows pass with L-2 noted. No findings filed.

## SECURITY review note (2026-09-23)

- New input surfaces: `--tier` CLI value (strict `FromStr`, garbage
  → usage error, exit before Vulkan); `GAME_DEBUG_TIER` env (warn +
  auto fallback); `F4` key (no payload). All reject paths fail
  clean, no panics (`expect_err` tests cover `--tier` garbage).
- New dependencies: none. New `unsafe`: none (reuses F1's two
  reviewed sites).
- Save/migration: untouched. Verdict: pass, no findings.
