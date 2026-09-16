# Roles — project lifecycle

Seven roles. One cycle. Every feature, update, and issue flows through them in order.

## The cycle

```text
PO → ARCHITECT + TECHLEAD + UX → DEV → ANALYST → SECURITY
 │         design (in parallel)        │       │         │
 │                                     │       │         └─ approves release of the artifact
 │                                     │       └─ audits + tests, moves to in-review
 │                                     └─ implements todos, runs gates
 └─ challenges the need, writes notion, owns Definition of Done
```

## Role map

| Role | File | One line |
|---|---|---|
| PO | [`po.md`](po.md) | Challenges the need; owns `notion.md` and the Definition of Done. |
| UX | [`ux.md`](ux.md) | Owns user journey, controls, affordances; consulted on anything player-facing. |
| ARCHITECT | [`architect.md`](architect.md) | Owns module boundaries, ADRs, invariants; approves every `plan.md` breakdown. |
| TECHLEAD | [`techlead.md`](techlead.md) | Owns implementation strategy, budgets, gate list; approves the todo table. |
| DEV | [`dev.md`](dev.md) | Implements todos, runs gates, writes DoD evidence. |
| ANALYST | [`analyst.md`](analyst.md) | Audits, writes the test plan, runs e2e tests, verifies DoD row-by-row. |
| SECURITY | [`security.md`](security.md) | Reviews serialization, I/O, input surfaces, dependencies; final sign-off. |

## Artifact → roles matrix

| Artifact | Author | Consulted | Approver |
|---|---|---|---|
| Feature `notion.md` | PO | UX (if player-facing), ARCHITECT | PO |
| Feature `plan.md` | TECHLEAD (+ ARCHITECT phases) | DEV, ANALYST, SECURITY | ARCHITECT + TECHLEAD |
| Implementation (todos) | DEV | TECHLEAD | ANALYST (audit) |
| DoD verification | DEV (evidence) | — | ANALYST + SECURITY |
| `update-*/notion.md` | PO | roles affected by the delta | PO |
| `update-*/plan.md` | TECHLEAD | ANALYST, SECURITY | ARCHITECT + TECHLEAD |
| Issue `specs.md` | reporter (any role) | DEV (triage) | PO (priority) |
| Issue `report.md` | DEV | ARCHITECT (if invariant touch) | TECHLEAD |
| Issue `plan.md` + fix | DEV | ANALYST | ANALYST + SECURITY |

Gates are documented, not scripted: the rule lives in
[`../../plans/README.md`](../../plans/README.md) § *Role gates*; each profile
below tells an agent exactly how to act as that role. When an agent picks up a
task it reads the matching profile first.

## How agents use these profiles

1. Read [`po.md`](po.md)–[`security.md`](security.md) for the role you are acting as.
2. Fill the `## Roles` / sign-off rows in the `plans/_template/` files you touch.
3. Never skip your gate: if you are DEV, do not self-approve DoD; if you are
   ANALYST, do not implement; if you are SECURITY, do not rubber-stamp.
