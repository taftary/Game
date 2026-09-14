//! Domain events cross an ownership boundary through a channel.
//!
//! Demonstrates the [events pattern](../../book/patterns/events.md): the
//! producer owns sending, the consumer owns receiving, and neither shares
//! mutable state.

use std::sync::mpsc::channel;

use planet_crafter_engine::node::NodeId;

fn main() {
    let (events, inbox) = channel::<NodeId>();
    events.send(NodeId::new(0)).expect("receiver is alive");
    drop(events);

    let received: Vec<NodeId> = inbox.iter().collect();
    assert_eq!(received.len(), 1);
    println!("message_flow: one node event crossed the boundary");
}
