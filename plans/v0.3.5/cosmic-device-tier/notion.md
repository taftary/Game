# Notion — cosmic-device-tier

## Status

`done` (PO sign-off 2026-09-23; UX consulted — n-a, shell-only:
per-tier grades were approved in their own features and Medium is
the already-specified tablet/low-desktop look; ARCHITECT consulted —
lib+bin+engine tier enums, HDR-chain rebuild reuse; DEV CDT-001..006
done 2026-09-23; ANALYST audit + SECURITY review recorded in
`plan.md` DoD table; single `done` commit on branch `v0.3.5`)

## Context

The debug shell hard-codes High on every GPU (`splat_k_default → 8`,
`veil_mode → March{48}`, `MipBloomParams::for_tier(High)`, F1's
baseline table in `cosmic-frame-timing/plan.md` DoD 6). `quality.md`
already budgets Medium/Low for UHD-class hardware; the shell never
reads `device_score`. F1 measured the prize on Intel UHD 620 (seed
1337, 1408×768): inspector prepass 126–140 ms at k8 vs 30 ms at k2 —
the tier knob moves the frame ~4×, exactly as the invocation ratio
predicts.

## Problem & Needs

- **Wrong default:** integrated GPUs run the discrete tier and land
  at ~5–7 fps on cosmic views; the operator must discover three
  separate env knobs (`K`, `VEIL`, and no bloom knob exists) to reach
  the specified look.
- **No single switch:** splat count, veil body, and bloom depth are
  set in three places with no shared vocabulary; captures must stay
  High for byte-identity while the window follows the device.
- **No visibility:** nothing shows which tier is active or how to
  change it.

## Goals

1. **Boot tier from the device.** `Cpu | VirtualGpu → Low`,
   `IntegratedGpu → Medium`, `DiscreteGpu → High`, `Other → Medium`;
   logged with the adapter name at boot. `GAME_DEBUG_TIER`
   (`low|medium|high`, strict) overrides the mapping.
2. **One tier drives all three knobs.** `splat_k` (1/2/8),
   `VeilMode` (Sprites / March 32 / March 48), bloom levels (3/4/5
   via the existing `MipBloomParams::for_tier`). The per-knob envs
   (`GAME_DEBUG_COSMIC_K`, `GAME_DEBUG_COSMIC_VEIL`) keep overriding
   their knob (strict parse rules untouched).
3. **Runtime cycle.** `F4` cycles Low → Medium → High (Controls
   listing + widget row show the active tier and its source);
   veil-affecting changes rebuild the cosmic buffers, level changes
   rebuild the HDR chain (existing paths, no new machinery).
4. **Capture pin.** `--tier low|medium|high` (default High) drives the
   offscreen path; existing DoD shot hashes hold without passing it.
5. **Measured win.** Medium vs High `cosmic_timing=` lines on the
   UHD 620 recorded in the plan (F1's pool is the instrument).

## Non-goals

- No new tier, no grade change inside any tier, no shader change.
- No dynamic resolution, no auto-stepping by frame time (manual +
  boot mapping only).
- No change to `--headless` (GPU-free), `game`, or `tools`.
- No culling/bricks/fill work (deferred follow-ups, gated on F1).

## Users / Stakeholders

- Operator (UHD 620 laptop): opens the viewer and gets the specified
  Medium look at ~4× the frame rate, no env spelunking.
- DEV/ANALYST: one switch for grading rounds; captures stay pinned.

## Roles

Author: PO. UX consulted (required if player-facing): n-a —
debug-shell-only; tier grades pre-approved per tier. ARCHITECT
consulted (required if cross-module): yes — `debug` lib + bin +
`engine` tier enums; HDR-chain rebuild path reuse; new `Action`.

## Functional requirements

- FR1 (mapping): `auto_tier(device_type)` per Goals §1 (pure,
  unit-tested, wildcard arm for future device types).
- FR2 (env): `GAME_DEBUG_TIER` parsed by the engine
  `QualityTier::from_str` contract (`low|medium|med|high`,
  ASCII-case-insensitive); garbage → stderr warning + fall back to
  auto (the `GAME_DEBUG_COSMIC_VEIL` precedent: never silently
  regrade).
- FR3 (knobs): tier sets `splat_k`, `veil_mode`, `MipBloomParams`
  levels; per-knob envs win per knob (mixed states legal and logged,
  e.g. tier Medium + `K=1`).
- FR4 (cycle): `Action::CycleTier` (`F4`, Chrome group — shell
  chrome, never a content key), handler cycles tiers, rebuilds
  buffers + chain as needed, console-notifies; registry size test
  updated (35 → 36 static).
- FR5 (display): FPS tab shows `tier: medium (auto|env|manual)`;
  Controls lists the action with its key.
- FR6 (capture): `--tier` parsed strictly, default High, threaded
  into splat_k/veil/bloom on the offscreen path only; usage string
  updated; parse tests extended.

## Non-functional requirements

- NFR1 (invariants): write-once bloom holds at 3/4/5 levels
  (`assert_write_once` covers `describe_bloom_chain(levels)`);
  capture byte-identity at default tier (timestamps + tier add no
  draws at High); `--headless` untouched.
- NFR2 (chrome rule): `F4` is not a content key on any tab
  (ADR-022); exactly one key + Controls row, no new button (tier is
  set at boot or by key; a button would need a new chrome surface).
- NFR3 (robustness): unknown `PhysicalDeviceType` futures map to
  Medium (never crash on non-exhaustive enum); cycle never loses the
  env pin (manual cycle overrides env for the session, logged).

## Definition of Done

1. Boot on UHD 620 selects Medium automatically (log line).
2. All three knobs follow the tier (windowed `cosmic_timing=` at
   Medium ≈ F1's k2 prediction, ~4× under High).
3. `F4` cycles with correct rebuilds (veil + levels), widget +
   Controls show it.
4. `--tier` capture pin works; default-tier captures byte-identical
   to pre-feature High captures.
5. Measured Medium-vs-High table recorded.
6. Full gate list + mobile guards green; ANALYST + SECURITY signed;
   single `done` commit.

## Constraints & Assumptions

- Assumes `MipBloomParams::for_tier` levels (3/4/5) stay the tier
  contract (engine-owned; this feature only stops hard-coding High).
- The cycle rebuild reuses `refresh_cosmic_seed` + the
  swapchain-recreate HDR path — heavier than needed for splat_k-only
  changes, but always consistent; manual action, hitch acceptable.

## Open questions

- Should the window remember a manual cycle across restarts? PO
  decision: no — boot mapping + env are the persistence; the key is
  a session override.
