# Decision log (ADRs to write as features land)

Navigation-engine ADRs (004, 006, 012–021) derive from the adopted spec
[`../techstack/cosmic-navigation-engine-v0.4.md`](../techstack/cosmic-navigation-engine-v0.4.md)
("spec §N" references below); drafts become binding when their owning
[`plans/`](../../plans/) feature lands.

- [x] ADR-001: `vulkano` / `winit` / `naga` / `fontdue` / `glam` versions + `winit`↔`raw-window-handle` compat + `vulkano` kill-switch — [`ADR-001.md`](ADR-001.md).
- [x] ADR-002: planet representation — hex-dominant geodesic dual mesh (`engine::hexsphere`) — [`ADR-002.md`](ADR-002.md).
- [x] ADR-003: atmosphere/sky model (analytic choice + mobile fallback) — Rayleigh/Mie source terms with Low-tier fallback; [`ADR-003.md`](ADR-003.md).
- [x] ADR-004: save binary encoding + migration strategy — binary autosave, atomic writes, rotating snapshots (spec §10) — binding in [`ADR-004.md`](ADR-004.md), landed with `plans/v0.3.0/autosave-persistence`.
- [ ] ADR-005: UI framework (custom immediate vs. retained) + localization keys — spec §10 constrains HUD to a decoupled fast overlay; framework choice stays open.
- [x] ADR-006: audio backend — **deferred to spec v0.5**; v0.4 scope ships silent, no audio-clock dependency — [`ADR-006.md`](ADR-006.md).
- [ ] ADR-007: reference devices per tier + device floor + perf harness — draft in [`ADR-007.md`](ADR-007.md), devices finalized by M6.
- [x] ADR-008: ECS — adopt `hecs`, custom scheduling — [`ADR-008.md`](ADR-008.md).
- [x] ADR-009: allocator default (`mimalloc`, validate M1) + `tracing` logging — [`ADR-009.md`](ADR-009.md).
- [x] ADR-010: cell-chunk identity (chunk = dual cell index, grouping deferred to M2) — [`ADR-010.md`](ADR-010.md).
- [x] ADR-011: build-order re-sequence — M5 universe maps executes ahead of M2–M4 (numbers are labels; landing-site selection moves M5 → M2) — [`ADR-011.md`](ADR-011.md).
- [x] ADR-012: coordinate architecture — hierarchical nested reference frames, composed transform chain (spec §3) — draft in [`ADR-012.md`](ADR-012.md), lands with `plans/v0.1.0/frame-hierarchy`.
- [x] ADR-013: GPU precision — floating-origin rendering, camera-relative `f32` upload (spec §3) — draft in [`ADR-013.md`](ADR-013.md), lands with `plans/v0.1.0/frame-hierarchy`.
- [x] ADR-014: SOI handoff — soft patched-conic blending, Laplace vs Hill sphere (spec §3) — draft in [`ADR-014.md`](ADR-014.md), lands with `plans/v0.1.0/soi-handoff`.
- [x] ADR-015: time compression — explicit state machine per reference frame, never a global multiplier (spec §2) — draft in [`ADR-015.md`](ADR-015.md), lands with `plans/v0.1.0/time-compression`.
- [x] ADR-016: depth strategy — log-depth buffer + multi-pass depth compositing (spec §4) — draft in [`ADR-016.md`](ADR-016.md), lands with `plans/v0.2.0/log-depth-rendering`.
- [x] ADR-017: star catalog streaming — HEALPix order 12–14, 2 GB budget, deterministic fallback (spec §4) — draft in [`ADR-017.md`](ADR-017.md), lands with `plans/v0.2.0/star-catalog-streaming`.
- [x] ADR-018: physics per scale — cheapest-correct models, symplectic integration, non-dimensionalization (spec §5) — draft in [`ADR-018.md`](ADR-018.md), lands with `plans/v0.1.0/scale-physics`.
- [x] ADR-019: procedural seeding — hierarchical seeds, domain separation, real-data override (spec §6) — draft in [`ADR-019.md`](ADR-019.md), lands with `plans/v0.1.0/hierarchical-seeding`.
- [x] ADR-020: real-data policy — VSOP87/DE440 ephemeris, Gaia ±1000 yr window, WGS84 (spec §5/§8) — draft in [`ADR-020.md`](ADR-020.md).
- [x] ADR-021: environment visuals — auto-exposure/ACES/dark adaptation, per-regime depth cueing, analytic zodiacal light (spec §9) — draft in [`ADR-021.md`](ADR-021.md).
- [x] ADR-022: unified debug shell — single window (reverses `debug-ui-reorganize` two-window model), key↔button parity, chrome-never-shadows-game-keys — [`ADR-022.md`](ADR-022.md), lands with `plans/v0.3.1/unified-debug-view`.
- [x] ADR-023: cosmic-scale player — stage-0 generated cosmic web, Game Demo rebuilt around the player, Cosmic Web inspector tab, real pill/console feed (extends ADR-022) — [`ADR-023.md`](ADR-023.md), lands with `plans/v0.3.2/cosmic-scale-player`.
- [x] ADR-024: cosmic-web Illustris look — render-only enrichment (frayed strands, stretched smoke sheaths, gold beads over the bifurcation/spine skeleton); no descriptor/hash/pipeline change (extends ADR-023) — [`ADR-024.md`](ADR-024.md), lands with `plans/v0.3.2/cosmic-web-illustris-look`.
