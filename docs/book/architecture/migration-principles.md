# Migration Principles

## Summary

The migration changes ownership boundaries before it changes behavior. Each
stage must leave a buildable, tested checkpoint with a clear rollback point.

## Key points

- Establish a buildable workspace before moving implementation files.
- Extract stable, renderer-neutral data before renderer-specific code.
- Preserve behavior and tests at every stage.
- Use adapters temporarily; do not turn compatibility shims into permanent APIs.
- Move one responsibility at a time and record public API changes.
- Keep headless tests independent from display and GPU requirements.
- Remove old entry points only after the target boundary is exercised by its consumers.

Detailed stage gates and rollback points are recorded in the project issue
tracker or pull request history.
