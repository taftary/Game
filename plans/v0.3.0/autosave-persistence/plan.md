# Plan — autosave-persistence

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Save envelope codec (`engine::save::format`)

ARCHITECT: binary envelope over the frozen `flight::ShipSnapshot` payload —
magic `GSAVE`, format version, metadata (master seed, real-world + sim
timestamps, playtime, catalog version IDs), FNV-1a 64 checksum trailer.
Decode order per ADR-004: magic/length → checksum → version → parse. Pure,
headless, no new dependencies. Invariant: procedural content never enters the
envelope (ADR-019); real-world timestamp is metadata-only.

### Phase 2 — Atomic writer + rotation (`engine::save::store`)

Thin fs layer (same precedent as `catalog::io`): temp-file + rename atomic
write, 3-deep rotation ring, `load_latest` walking newest → oldest,
quarantining corrupt files (`*.corrupt`) and falling back cleanly — never
panic, never boot-loop (persistence contract).

### Phase 3 — Trigger policy (`game::save`)

`Autosave` manager: confirmed frame transitions, completed SOI handoffs,
fly-to start/completion, clean quit, and a caller-fed periodic accumulator
(default 120 s, clamped 30–600 s). Returns save reasons; the shell composes
snapshot capture + ring write + indicator. Bounded event log for evidence.

### Phase 4 — Headless evidence + docs

Scripted scenario in the `game` demo printing the `save:` event log (every
ADR-004 trigger), truncation → recovery demonstration, and reload/resume.
Round-trip tests: mid-flight, mid-fly-to, post-handoff bit-identical. Docs:
`persistence.md` concrete layout, ADR-004 → binding, architecture, controls,
techstack version, milestones.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| AP-20260917-001 | done | Envelope codec: encode/decode, checksum-first, reject bad magic/truncation/version/checksum/metadata/payload. | Goals 1; Non-goals |
| AP-20260917-002 | done | Atomic writer + 3-slot rotation + quarantine + load-fallback tests (hand-rolled temp dirs). | Goal 3; Functional requirements |
| AP-20260917-003 | done | `game::save::Autosave` trigger manager: 6 save reasons, interval clamp 30–600, bounded log. | Goal 2; UX notes |
| AP-20260917-004 | done | Headless demo scenario: every trigger fires (event log), truncation recovery, reload + bit-identical resume mid-flight/mid-fly-to/post-handoff. | DoD 1–3 |
| AP-20260917-005 | done | Docs: persistence.md layout, ADR-004 binding, architecture, controls, techstack 0.29.0, milestones. | Non-functional requirements |

## Role sign-off

Breakdown approved by: ARCHITECT (2026-09-17) · Todos approved by: TECHLEAD
(2026-09-17) · UX acceptance rows: UX (2026-09-17) · DoD verified by: ANALYST
(2026-09-17) · Security reviewed by: SECURITY (2026-09-17).

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | Round-trip: resume mid-flight, mid-fly-to, post-handoff — state identical. | verified | `engine::save::format` tests `round_trip_mid_flight_is_identical`, `round_trip_mid_fly_to_resumes_exactly`, `round_trip_post_handoff_is_identical`; demo `resume: ... state-identical ok`. | ANALYST 2026-09-17 |
| 2 | Corruption: truncated save rejected via checksum, recovery snapshot loads. | verified | `corrupt_inputs_reject_cleanly`, store `corrupt_newest_quarantines_and_falls_back`; demo `corrupt:`/`recovery:` trace. | ANALYST 2026-09-17; SECURITY 2026-09-17 |
| 3 | Every ADR-004 trigger demonstrably fires. | verified | demo `save-log:` lines (6 reasons) + `every_adr004_trigger_fires_and_logs`. | ANALYST 2026-09-17 |

## Acceptance criteria

- UX: interval default 120 s honored; values clamp to 30–600 s; a save
  success produces a transient notice line; failures surface a distinct
  warning and the process continues.
- Corrupt or truncated saves never block startup: quarantine + fallback or
  clean `NoValidSave`, per the persistence contract.
- Checksum is verified before version/parse (ADR-004 decode order).
- No new dependencies; `game` stays vulkano-free; procedural content never
  enters the save payload.

## Risks & Next steps

- The quit trigger fires on clean exit only; crash/power-loss recovery relies
  on the rotation ring (documented limitation).
- Real-world timestamp is caller-supplied metadata; embedding it in the
  resume hash would break determinism — it is excluded by construction.
- SECURITY deep review (2026-09-17): file I/O is confined to fixed
  `autosave.N.bin` names under the caller-chosen directory (no path
  injection); all decode paths reject without panics; metadata strings are
  bounded (≤16 stamps, ≤64 B); corrupt files are quarantined, never
  deleted; no new dependencies, no `unsafe`, no network surface.
- Next: version close — merge `v0.3.0` → `main` when this feature is `done`.
