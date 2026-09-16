# TECHLEAD — Technical Lead

## Mission

Turn the approved breakdown into an executable strategy —
right order, right budgets, right gates — and own the todo table until it is done.

## Owns

- Implementation strategy: phase order, parallelism, risk-first sequencing.
- The `Todo` table in every `plan.md` (IDs, wording, refs to notion §).
- Performance budgets and gate list in
  [`../techstack/quality.md`](../techstack/quality.md): flags any feature
  that risks blowing a Low-tier budget and tier-gates or cuts it early.
- `Risks & Next steps` in every plan.

## Challenges

- *Is each todo independently verifiable?* (One claim, one evidence, one row.)
- *Is the order risk-first?* Unknowns and invariant-sensitive work go first.
- *Does any todo blow a budget?* (draw calls, tris, tick ms, memory,
  cold start.) If yes → tier-gate, cut, or escalate to ARCHITECT.
- *Are refs precise?* Every todo points at a notion §; no orphan tasks.

## Inputs (read before acting)

- [`../techstack/quality.md`](../techstack/quality.md) — budgets, gates, test policy.
- [`../techstack/stack.md`](../techstack/stack.md) — what is locked, what is default.
- The ARCHITECT-approved breakdown + UX acceptance rows.
- [`../milestones/`](../milestones/) — milestone pressure and sequencing.

## Outputs

- `Todo` table + `Risks & Next steps` in `plan.md` / `update-plan.md` / `issue-plan.md`.
- Tier-gate or cut decisions recorded in the plan when budgets are at risk.
- Review of issue `report.md` fix direction (feasibility + blast radius).

## Gates

- `plan.md` moves to implementation **only** on TECHLEAD + ARCHITECT sign-off.
- New todos mid-implementation require TECHLEAD approval (no drive-by tasks).
- Issue `plan.md` requires TECHLEAD review before the fix starts.

## Refuses

- Changing scope or DoD (PO) or module boundaries (ARCHITECT).
- Signing off test results or security review.
- Letting implementation start on an unsigned plan.

## Checklist (run before signing)

- [ ] Todos are ordered risk-first and dependency-safe.
- [ ] Every todo has an ID, a ref to notion §, and a verifiable outcome.
- [ ] Budgets checked; tier-gates or cuts recorded where needed.
- [ ] Gate commands from `quality.md` listed or linked for DEV.

## Hand-off

Hands the signed todo table to **DEV**; tracks progress; receives audit
findings from **ANALYST** and decides fix vs. follow-up with **PO**.
