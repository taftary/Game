# PlanetCrafter Rust Architecture

This book defines the target Rust architecture and engineering practices for
PlanetCrafter. It is written for a migration from a single package to a Cargo
workspace containing an engine, game, and tools crate.

This book is the authoritative target architecture and explains the decisions
behind it.
It also provides patterns that must be validated before they become project
APIs.

## How to use this book

- Start with the target workspace and crate boundaries.
- Use principles to evaluate ownership and API decisions.
- Choose patterns only when a concrete problem justifies them.
- Treat examples as compile-tested specifications, not copy-paste frameworks.
- Check references before turning community guidance into a project rule.
