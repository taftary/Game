# Issue specs — cosmic-device-tier / issue-2026-09-24-0856-high-to-low-stale-frame (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`done` (report: [`report.md`](report.md), fix plan: [`plan.md`](plan.md) — all fix todos checked, ghost confirmed gone by the operator; fps-instability follow-up: [`../issue-2026-09-24-0944-low-tier-fps-unstable/specs.md`](../issue-2026-09-24-0944-low-tier-fps-unstable/specs.md))

## Roles

Reported by (role): operator (windowed `game_debug` session) · Priority set by PO: _(pending)_

## Observed vs Expected

Observed: pressing `F4` from High to Low (through Medium, or Medium to Low)
leaves the cosmic debug views showing a weird composite — the new Low
scene with a ghost of the previous quality's frame stuck over/under it.
Persists every frame until leaving Low / restarting. Affects both cosmic
surfaces that share the HDR chain (Cosmic Web inspector tab and Game Demo
tab).

Expected: each `F4` step recomputes `splat_k` (8/2/1), veil body
(March 48 / March 32 / Sprites) and bloom levels (5/4/3) and the next
frame draws the selected tier cleanly — per `cosmic-device-tier` notion
Goals §3 / FR4 ("veil-affecting changes rebuild the cosmic buffers,
level changes rebuild the HDR chain").

## Reproduction steps

1. `cargo run -p game_debug` on an HDR-capable GPU (not `GAME_DEBUG_COSMIC_POST=0`).
2. Open the Cosmic Web tab (inspector) or the Game Demo tab.
3. Note the starting tier in the FPS widget (`tier: high/medium (...) [F4]`).
4. Press `F4` until the widget reads `tier: low (manual)`.
   - Discrete GPU boot: High → Medium → Low (two presses).
   - UHD 620 boot: Medium → High → Low, or Medium → Low via `GAME_DEBUG_TIER=medium` then `F4` ×2.
5. Observe: Low view is not the clean sprites look from `--tier low`
   captures — it carries a stale frame from the previous tier.

Variants expected to hit the same path (unconfirmed on GPU, by code
reading): `GAME_DEBUG_TIER=low` boot followed by any resize
(swapchain-recreate rebuild), and fresh Low boot on HDR hardware.

## Scope & Impact

- Debug shell only (`crates/debug` bin). No `engine` / `game` / `tools`,
  no save format, no player journey.
- HDR mode only. LDR bypass (`hdr_format == None`, `GAME_DEBUG_COSMIC_POST=0`)
  draws glow + splats direct-to-swapchain and never touches the march target.
- Trigger is any entry into `VeilMode::Sprites`: High → Low and
  Medium → Low. March → March steps (High ↔ Medium) keep writing the
  march target every frame and are unaffected.
- Severity: Low tier unusable windowed after a cycle on HDR hardware;
  captures with `--tier low` on a fresh device may pass by luck (fresh
  allocation reads as zero) while the windowed cycle fails
  deterministically (allocator reuses the old march pages).

## Logs / Evidence

Code-reading evidence (no GPU run in this automation):

- `cycle_cosmic_tier` (`crates/debug/src/main.rs:7803`) recomputes
  `veil_mode`/`splat_k` then calls `refresh_cosmic_seed`
  (`crates/debug/src/main.rs:7825`), which rebuilds the veil volume,
  displacement volume, cell list, `HdrChain` at the new level count
  (`crates/debug/src/main.rs:7852`), glow buffers, and the rebase worker.
- `build_hdr_chain` (`crates/debug/src/main.rs:8444`) creates a fresh
  quarter-res march image + framebuffer (`crates/debug/src/main.rs:8576`)
  plus `resolve_set`/`resolve_nobloom_set` sampling
  scene + bloom-top + march (`crates/debug/src/main.rs:8592`).
- The march pass is conditional in both recording paths:
  windowed (`crates/debug/src/main.rs:10658`) and offscreen capture
  (`crates/debug/src/main.rs:2410`) — `if March { record_veil_march }`,
  with the comment "skipped in sprites mode (cleared target adds ~0)".
- No clear is recorded on the skip path. The clear only exists inside
  `begin_post_pass` (`crates/debug/src/main.rs:6721`, black clear) which
  `record_veil_march` (`crates/debug/src/main.rs:6910`) opens — skipped
  means the fresh march image is never written.
- The resolve always samples march with full gain:
  `record_cosmic_view_arm` (`crates/debug/src/main.rs:6964`),
  composition `scene + bloom·i + march·e`
  (`crates/debug/src/main.rs:11792`), gain always
  `VEIL_MARCH_RESOLVE_GAIN = 30.0`
  (`crates/debug/src/main.rs:823`, passed at
  `crates/debug/src/main.rs:10800` and `crates/debug/src/main.rs:2447`).
- Post pass is `load_op: Clear` (`crates/debug/src/main.rs:5724`) but
  that only clears when the pass begins — irrelevant on the skip path.

## Suspected area

Stale veil-march HDR target on entry into Sprites mode: the resolve
reads an unwritten (allocator-recycled) march image ×30. See `report.md`
for the ruled-in / ruled-out analysis. Invariant touch: bloom
write-once (`docs/techstack/rendering.md` HDR bloom section, Intel UHD
620 rule) — the `assert_write_once` pin asserts the march-included
description unconditionally.
