# Type-Driven Design

**Status:** Target

## Summary

Model domain invariants in types so invalid states are difficult to construct.
Use enums for closed state sets, newtypes for units and identifiers, and traits
for behavior that has multiple valid implementations.

## Key points

- Newtypes distinguish coordinates, indices, handles, and sizes.
- Enums represent lifecycle states and exhaustive decisions.
- Constructors validate invariants and return `Result` when validation can fail.
- Traits describe stable capabilities, not class-like inheritance.
- Keep generic abstractions small until two real consumers require them.

## Example

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VertexIndex(u32);

impl VertexIndex {
    pub fn new(value: u32) -> Self { Self(value) }
}
```

**References**

[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
