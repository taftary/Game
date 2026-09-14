# Safety Boundaries

**Status:** Target

## Summary

Safe Rust is the default. Unsafe code is permitted only for measured
capabilities that safe Rust cannot provide, such as platform or GPU boundaries.

## Key points

- Every unsafe block must have a small scope and a documented safety invariant.
- Validate invariants at the safe API boundary and test the invariant and failure cases.
- Assign an owner responsible for reviewing unsafe changes.
- FFI and backend handles belong in engine modules; game and tools consume safe
  wrappers and must not reproduce unsafe assumptions.

**References**

[Rustonomicon](https://doc.rust-lang.org/nomicon/)
