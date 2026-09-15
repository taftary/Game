# Decision log (ADRs to write as features land)

- [x] ADR-001: `vulkano` / `winit` / `naga` / `fontdue` / `glam` versions + `winit`↔`raw-window-handle` compat + `vulkano` kill-switch — [`ADR-001.md`](ADR-001.md).
- [x] ADR-002: planet representation — hex-dominant geodesic dual mesh (`engine::hexsphere`) — [`ADR-002.md`](ADR-002.md).
- [ ] ADR-003: atmosphere/sky model (analytic choice + mobile fallback).
- [ ] ADR-004: save binary encoding + migration strategy.
- [ ] ADR-005: UI framework (custom immediate vs. retained) + localization keys.
- [ ] ADR-006: audio backend.
- [ ] ADR-007: reference devices per tier + device floor + perf harness — draft in [`ADR-007.md`](ADR-007.md), devices finalized by M6.
- [x] ADR-008: ECS — adopt `hecs`, custom scheduling — [`ADR-008.md`](ADR-008.md).
- [x] ADR-009: allocator default (`mimalloc`, validate M1) + `tracing` logging — [`ADR-009.md`](ADR-009.md).
- [x] ADR-010: cell-chunk identity (chunk = dual cell index, grouping deferred to M2) — [`ADR-010.md`](ADR-010.md).
