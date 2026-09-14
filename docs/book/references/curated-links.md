# Curated References

Sources are research inputs. Official Rust documentation takes precedence over
community pattern catalogs and general design-pattern tutorials.

| Source | Type | Reliability and use |
| --- | --- | --- |
| [Rust](https://rust-lang.org/) | Official | Primary source for language goals, tooling, and ecosystem entry points. |
| [The Rust Book](https://doc.rust-lang.org/book/) | Official | Primary guidance for ownership, error handling, traits, and testing. |
| [Rustonomicon](https://doc.rust-lang.org/nomicon/) | Official | Primary guidance for unsafe Rust and low-level invariants. |
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | Community-maintained | Practical public API and library design conventions. |
| [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) | Community catalog | Rust-specific idioms, patterns, anti-patterns, and the YAGNI principle. |
| [Patterns repository](https://github.com/rust-unofficial/patterns) | Open-source repository | Source history and contribution context for the pattern catalog. |
| [Software Patterns Lexicon](https://softwarepatternslexicon.com/rust/) | Secondary guide | Broad architecture and pattern survey; verify claims against official docs. |
| [W3Reference Rust patterns](https://www.w3reference.com/rust-tutorial/rust-design-patterns-a-tutorial-on-code-architecture/) | Secondary tutorial | Introductory pattern examples; do not treat unsafe singleton examples as project guidance. |
| [Refactoring Guru Rust examples](https://refactoring.guru/design-patterns/rust) | Secondary catalog | Useful vocabulary and examples; adapt patterns to Rust ownership and type design. |
| [Cargo documentation](https://doc.rust-lang.org/cargo/) | Official | Workspace, dependency, build, and test behavior. |
| [rustdoc documentation](https://doc.rust-lang.org/rustdoc/) | Official | API documentation and doctest behavior. |
| [Clippy](https://doc.rust-lang.org/clippy/) | Official project | Linting and maintainability checks. |

Review links quarterly or when a documented decision depends on an evolving
library or platform constraint.

## Review log

- **2026-09-04** - All links reachable, with one caveat: `w3reference.com`
  blocks plain HEAD requests (returns 520) but serves the page with a browser
  user agent; the source remains valid.
