# DRY (Don't Repeat Yourself)

**Status:** Target

## Summary

Every rule and fact has a single authoritative home - in code and in
documentation. Duplicated knowledge drifts out of sync; express each piece of
knowledge once and reference it everywhere else. DRY targets knowledge, not
superficially similar text: coincidental similarity is not duplication.

## Key points

- **Single source of truth** - each rule, constant, and behavior lives in
  exactly one place; everything else calls or links to it.
- **Rule of three** - abstract on the third concrete occurrence, not the
  first; a premature abstraction is harder to remove than a late one is to
  extract.
- **Duplication vs. coincidence** - two fragments that change for the same
  reason are knowledge duplication and must be unified; fragments that look
  alike but change for different reasons are coincidence - leave them.
- **Documentation follows the same rule** - canonical homes are
  `AGENTS.md` (assistant instructions), `docs/STYLEGUIDE.md` (documentation
  style), and `docs/book/practices/` (engineering practices; `testing.md` owns
  the validation commands).
  Entry points such as `README.md` summarize and link instead of restating.
- **Deliberate mirrors** - when an audience genuinely needs its own copy (the
  viewer controls in `README.md`), name the owner and
  guard the mirror with an accuracy check in the definition of done.

## Example

```rust
// Before: (a + b) * 0.5 inlined at every call site.
// After: one home for the midpoint rule (Vec3 comes from glam).
fn midpoint(a: Vec3, b: Vec3) -> Vec3 {
    (a + b) * 0.5
}
```

**References**

[Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
