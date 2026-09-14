# Events and Message Passing

**Status:** Target

## Summary

Use messages when systems should communicate without sharing mutable internals.
An event should carry the facts needed by its consumers and should not expose a
backend object as a shortcut.

## Key points

- Prefer direct function calls inside one cohesive subsystem.
- Use message passing at ownership or scheduling boundaries, not everywhere.
- Input adapters publish normalized actions; game systems emit domain events.
- The engine consumes render commands or renderer-neutral draw data.
- Channels are bounded when producers can outpace consumers.
- Shutdown and cancellation are explicit messages.

## Example

A game system emits domain events across an ownership boundary through a
bounded channel; the consumer drains them without sharing mutable state.

```rust
use std::sync::mpsc;

#[derive(Debug, PartialEq)]
enum GameEvent {
    NodeSplit { name: String },
    Shutdown,
}

fn main() {
    let (sender, receiver) = mpsc::sync_channel::<GameEvent>(8);
    sender.send(GameEvent::NodeSplit { name: "root".into() }).unwrap();
    sender.send(GameEvent::Shutdown).unwrap();

    let events: Vec<_> = receiver.try_iter().collect();
    assert_eq!(events.len(), 2);
}
```

The runnable version of this flow is the [`message_flow` example](../examples/index.md).
