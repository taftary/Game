# Builder and Typestate

**Status:** Target

## Summary

Use a builder when construction has many optional values or validation rules.
Use typestate when a small, stable state transition must be enforced at compile
time.

## Key points

- Builders improve call-site readability but add API surface.
- Typestate prevents invalid transitions but can increase the number of types.
- Prefer a validated constructor when neither benefit is needed.
- Test both successful construction and rejected invariants.

## Example

```rust
#[derive(Debug, PartialEq)]
pub struct RenderConfig { width: u32, height: u32 }

pub struct RenderConfigBuilder { width: u32, height: u32 }

impl RenderConfigBuilder {
    pub fn new() -> Self { Self { width: 1, height: 1 } }
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    pub fn build(self) -> Result<RenderConfig, &'static str> {
        (self.width > 0 && self.height > 0)
            .then_some(RenderConfig { width: self.width, height: self.height })
            .ok_or("render size must be positive")
    }
}
```

## Typestate example

Typestate moves the validity check from runtime to the type system. A config
can only be built after it has been explicitly sized.

```rust
#[derive(Debug, PartialEq)]
pub struct RenderConfig { width: u32, height: u32 }

pub struct Unconfigured;
pub struct Configured;

pub struct RenderConfigBuilder<State = Unconfigured> {
    width: u32,
    height: u32,
    _state: std::marker::PhantomData<State>,
}

impl RenderConfigBuilder<Unconfigured> {
    pub fn new() -> Self {
        Self { width: 0, height: 0, _state: std::marker::PhantomData }
    }
    pub fn size(self, width: u32, height: u32) -> RenderConfigBuilder<Configured> {
        RenderConfigBuilder {
            width,
            height,
            _state: std::marker::PhantomData,
        }
    }
}

impl RenderConfigBuilder<Configured> {
    pub fn build(self) -> RenderConfig {
        RenderConfig { width: self.width, height: self.height }
    }
}
```