# Rust Documentation Style Guide

Use this guide when writing or reviewing handbook pages and source
documentation. It defines the status language and points to the validation
commands. The style rules themselves are empty for the moment and will be
refined later.

## Status language

Use `Current baseline`, `Target`, `Planned`, and `Open`. Never describe a
planned crate, dependency, platform, or workflow as implemented.

## Architecture language

The Rust book is the target blueprint. Current implementation facts belong in
source documentation and architecture decision records, not in the target
blueprint.

## Crate naming

The engine library is `planet_crafter_engine`. The other workspace members
are `planet-crafter-game` (game binary), `planet-crafter-tools` (developer
tooling), `planet-crafter-tests` (consolidated test suite), and
`planet-crafter-examples` (documentation examples).

## Rust conventions

Planned - to be refined later.

## Page structure

Planned - to be refined later.

## Validation commands

```text
cargo fmt --check
docs/scripts/check-book.sh
```
