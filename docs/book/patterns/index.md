# Pattern Selection

**Status:** Target

## Summary

Patterns are vocabulary for recurring problems, not mandatory architecture.
Rust often replaces classic patterns with enums, traits, closures, ownership,
and iterators.

## Key points

- Use a pattern only when it reduces coupling or makes an invariant clearer.
- Do not introduce a pattern to anticipate an unconfirmed future requirement.
- The project prioritizes builders, newtypes, state machines, and messages.

## Example

The target project prioritizes:

- builder and typestate for validated construction;
- newtypes for domain-safe identifiers and units;
- state machines for lifecycle transitions;
- command and event messages for decoupled systems;
- adapters and facades around platform or renderer boundaries.

The Rust Patterns catalog emphasizes the same principle through YAGNI: avoid
complexity when a language feature already solves the problem.
