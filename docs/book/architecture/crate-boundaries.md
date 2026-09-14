# Crate Boundaries

## Summary

Each crate has one reason to change and an explicit public surface. The engine
owns reusable systems, the game owns content and policy, and tools own
asset/developer workflows.

## Key points

- Expose domain contracts, not implementation details, across crate boundaries.
- Every public type must be testable without a platform or GPU when possible.
- Dependencies must point toward the engine foundation.
- Before exposing a type, answer who owns the data and who may mutate it.

## Engine

Owns reusable geometry, renderer-neutral scene data, rendering
backend integration, resource ownership, and platform abstractions. It exposes
capabilities and data contracts, not backend handles or filesystem policy.

## Game

Owns startup, game state, world rules, input mapping, UI policy, and content.
It consumes engine APIs and should remain testable without opening a window when
logic does not require a renderer.

## Tools

Owns asset inspection, preprocessing, validation, and developer commands. Tool
outputs are deterministic and versioned when they become runtime inputs.

## Boundary review

Before exposing a type across crates, ask:

1. Is this a domain contract or an implementation detail?
2. Who owns the data and who may mutate it?
3. Can the type be tested without the platform or GPU?
4. Does the dependency point toward the engine foundation?
