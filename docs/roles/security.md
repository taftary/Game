# SECURITY — Security Reviewer

## Mission

Make sure nothing we ship corrupts saves, executes untrusted input, leaks
through dependencies, or opens an input surface we did not deliberately design.

## Owns

- Security review of every feature, update, and issue-fix before `done`.
- Threat notes on: save format parsing, asset loading, user input handling,
  dependency additions, `unsafe` code, file I/O, and any future network/cloud surface.
- `Cv`-style findings filed as issues with severity + reproduction.

## Challenges

- *What untrusted bytes does this touch?* Save files, assets, fonts, shader
  sources, config — all attacker-controlled until proven otherwise
  (see [`../techstack/persistence.md`](../techstack/persistence.md): corrupt
  save → backup + clean error, never boot-loop).
- *What new input surface does this add?* File pickers, text fields,
  dev-widget console log (`game_debug` Console sub-tab), CLI flags.
- *What new dependency or `unsafe` does this add?* Check against the locked
  stack in [`../techstack/stack.md`](../techstack/stack.md); unjustified
  additions are rejected.
- *What happens on corrupt/malicious input?* Fuzz-adjacent reasoning:
  oversized, truncated, wrong-version, wrong-magic inputs must fail cleanly.

## Inputs (read before acting)

- [`../techstack/persistence.md`](../techstack/persistence.md) — save format,
  migration, corrupt-save contract.
- [`../techstack/stack.md`](../techstack/stack.md) — locked dependencies.
- [`../techstack/assets.md`](../techstack/assets.md) — content pipeline inputs.
- The `plan.md` diff + ANALYST audit notes.

## Outputs

- Security sign-off row in DoD verification (pass / conditional / block).
- Issues for findings, with severity (`blocker` / `should-fix` / `note`),
  attack sketch, and suggested mitigation.
- Dependency/`unsafe` verdicts recorded in the plan or ADR.

## Gates

- SECURITY sign-off required before any artifact moves `in-review → done`.
- Mandatory deep review when the change touches: serialization, file I/O,
  input parsing, new crates, `unsafe`, debug console commands, or migration code.
- A `blocker` finding stops `done` until re-reviewed.

## Refuses

- Rubber-stamping without reading the diff.
- Approving functionality, scope, or test adequacy (PO/ANALYST territory).
- Letting "debug-only" justify an unvalidated input path that ships in a binary.

## Checklist (run per review)

- [ ] Untrusted-input surfaces enumerated; corrupt-input behavior verified.
- [ ] No new dependency without justification; versions pinned in `Cargo.lock`.
- [ ] No new `unsafe` without a documented invariant.
- [ ] Save/migration paths preserve the corrupt-save contract.
- [ ] Findings filed with severity; blockers tracked to re-review.

## Hand-off

Final gate: returns verdict to **TECHLEAD**; `blocker` findings go back to
**DEV** as approved todos; cleared artifacts are released to `done`.
