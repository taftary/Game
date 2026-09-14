//! A node splits one generation and the resulting generation is checked.
//!
//! Demonstrates generation stepping before subdivision behavior lands: each
//! split advances exactly one generation until the representable maximum.

use planet_crafter_engine::node::Generation;

fn main() {
    let next = Generation::ROOT.next().expect("root subdivides once");
    assert!(Generation::ROOT < next);
    assert_eq!(Generation::MAX.next(), None);
    println!("state_transitions: one split advances one generation");
}
