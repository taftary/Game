# Ownership and Borrowing

**Status:** Target

**Summary**

Use ownership to make the lifetime and mutation authority of engine data
explicit. Borrow when a caller should not retain data, move when ownership must
transfer, and use shared ownership only when the domain genuinely shares a
lifetime.

**Use**

- Borrow mesh and scene data for short render submissions.
- Move owned resources into the subsystem that releases them.
- Use `Arc` only across thread boundaries and `Rc` only for single-threaded shared graphs.
- Use interior mutability at a deliberate boundary, not to hide unclear ownership.

**Avoid**

- Cloning large buffers to silence the borrow checker.
- Global mutable state for renderer or game services.
- `Arc<Mutex<T>>` as the default architecture.

**Minimal example**

```rust
pub struct Mesh { vertices: Vec<[f32; 3]> }

pub fn vertex_count(mesh: &Mesh) -> usize {
    mesh.vertices.len()
}
```

**Pitfalls**

Prefer changing the owner or splitting a structure before adding lifetimes that
spread through unrelated APIs. Document why shared ownership is required.

**Test**

The example is exercised by a unit test in the owning crate. Boundary examples
must also prove that borrowed data cannot outlive its owner.

**References**

[Rust Book: Understanding Ownership](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
