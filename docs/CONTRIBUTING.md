# Contributing to the Rust Documentation

Documentation and migration changes are submitted through pull requests.
Contributors are expected to follow the [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

A pull request should include:

- a concise problem statement and summary;
- the architecture or practice category;
- source links and reliability notes for external claims;
- a minimal runnable example when guidance introduces code;
- tests or validation commands when applicable;
- explicit status labels as defined in [`STYLEGUIDE.md`](STYLEGUIDE.md).

Migration pull requests should move one responsibility at a time, preserve a
buildable checkpoint, and explain temporary adapters and their deletion plan.
Architecture changes require updates to the relevant Rust book chapter or an
architecture decision record as appropriate. When a change retires a document,
record its replacement in [`RETIREMENT.md`](RETIREMENT.md).

Reviewers should follow the [`REVIEW_CHECKLIST.md`](REVIEW_CHECKLIST.md) for
every documentation, pattern, or architecture change.

## Validation

Before submitting a documentation change, run the handbook build and link
checker from the repository root:

```text
docs/scripts/check-book.sh
```

This script reads the pinned `mdbook` version from `docs/.mdbook-version`,
installs it under `target/mdbook` if needed, builds the book, and checks for
broken internal links and missing required page sections. The first run may
take a few minutes while `cargo` compiles `mdbook`.

Rust source changes must also pass the standard checks listed in
[Testing and doctests](book/practices/testing.md): formatting, Clippy,
workspace and doc tests, plus a compile-check of the `gpu`-gated `viewer`
example.
