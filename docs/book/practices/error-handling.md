# Error Handling

**Status:** Target

## Summary

Use `Result<T, E>` for recoverable failures and `Option<T>` for an expected
absence. Add context at subsystem boundaries while preserving the underlying
cause for diagnostics.

## Key points

- Engine errors describe resource, device, validation, and lifecycle failures.
- Game errors describe content and state-transition failures.
- Tools errors identify invalid input paths, formats, and reproducibility failures.
- `panic!` is reserved for violated internal invariants or unrecoverable startup conditions.
- Library APIs expose meaningful typed errors; application code attaches context.

## Boundary policy

Choose an error crate only after the workspace's library/application boundary is
clear. Library APIs should expose meaningful typed errors; application code may
attach context and decide how to report or terminate.

**References**

[Rust Book: Recoverable Errors](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
