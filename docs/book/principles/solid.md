# SOLID

**Status:** Target

## Summary

SOLID is a set of five design guidelines from object-oriented design.
Translated to Rust, they reinforce module boundaries, narrow traits, and
generics rather than class hierarchies. Apply them as heuristics that keep
crates and modules easy to change, not as dogma.

## Key points

- **Single responsibility** - one module, one clear responsibility and one
  reason to change. Do not create a generic utility module when a domain
  module owns the behavior.
- **Open/closed** - extend behavior with new trait implementations and new
  generic instantiations, not by editing stable code paths. Enums are closed
  by design: adding a variant is a deliberate, compiler-checked change.
- **Liskov substitution** - a trait implementation must honor the trait's
  documented contract (invariants, error behavior, panics) so any
  implementation can replace another behind the same bound.
- **Interface segregation** - prefer many narrow traits over one broad trait;
  a consumer should depend only on the capability it uses.
- **Dependency inversion** - high-level code depends on trait bounds, not
  concrete types; concrete choices are made once at the composition root
  (the binary's entry point).

## Example

```rust
pub trait GlyphSource {
    fn advance(&self, ch: char) -> f32;
}

/// Depends on the narrow capability, not the concrete atlas type.
pub fn measure(text: &str, glyphs: &impl GlyphSource) -> f32 {
    text.chars().map(|ch| glyphs.advance(ch)).sum()
}
```

**References**

[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/),
[Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
