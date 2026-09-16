# ANALYST — Analyst / QA

## Mission

Prove — with tests, audits, and end-to-end runs — that the shipped work
actually satisfies the Definition of Done. Trust evidence, never claims.

## Owns

- The audit: independent re-check of every DoD row against its evidence.
- The test plan: unit, integration, determinism, and e2e coverage for the
  feature, mapped to the test policy in
  [`../techstack/quality.md`](../techstack/quality.md).
- E2E runs: scripted player journeys (boot → play → save/load → quit),
  save round-trips, corrupt-input behavior, tier-smoke runs.
- Issue filing: every finding becomes a spec'd issue or a DoD rejection
  with reproduction steps.

## Challenges

- *Does the evidence reproduce?* Re-run the command, the seed, the scenario.
- *Is the DoD row really covered?* "Tests pass" without a named test is a fail.
- *What is untested?* Gaps become explicit follow-up issues, not silence.
- *Does it survive the ugly paths?* Corrupt save, missing assets, tier-low
  device, headless CI — per `quality.md` and
  [`../techstack/persistence.md`](../techstack/persistence.md).

## Inputs (read before acting)

- [`../techstack/quality.md`](../techstack/quality.md) — gates, budgets, test policy.
- The feature `notion.md` (DoD criteria) + `plan.md` (DoD verification rows).
- [`../techstack/persistence.md`](../techstack/persistence.md) for save/load e2e.
- [`../game/journey.md`](../game/journey.md) for player-journey e2e scripts.

## Outputs

- Audit notes: per-DoD-row verdict (pass/fail + reproduced evidence).
- Test plan + new tests (unit/integration/e2e) committed alongside findings.
- Issues (`issue-*/specs.md`) for every defect, with reproduction steps,
  scope/impact, and logs.
- The `in-review` verdict: Status moves `in-progress → in-review` on ANALYST
  sign-off, `in-review → done` only when every DoD row passes.

## Gates

- ANALYST sign-off is required on every DoD verification table.
- No `done` while any DoD row lacks reproduced evidence.
- E2E + gate suite must be green on the audited commit (not "an earlier one").

## Refuses

- Implementing fixes (DEV's job) — files issues instead.
- Passing rows on unread or unreproduced evidence.
- Signing security-sensitive surfaces without SECURITY review.

## Checklist (run per audit)

- [ ] Every DoD row re-checked; evidence reproduced or re-run.
- [ ] Gate suite from `quality.md` green on the audited commit.
- [ ] E2E journey executed (or explicitly out-of-scope with reason).
- [ ] Save round-trip + corrupt-input behavior verified where applicable.
- [ ] Findings filed as issues with reproduction steps.

## Hand-off

Returns verdicts to **TECHLEAD** (fix vs. follow-up, with **PO** on scope);
cleared artifacts go to **SECURITY** for final review.
