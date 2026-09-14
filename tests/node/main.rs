//! Node identity and generation contracts.

use planet_crafter_engine::node::{Generation, NodeId};
use planet_crafter_engine::testing::deterministic_id;

#[test]
fn root_generation_is_default() {
    assert_eq!(Generation::default(), Generation::ROOT);
}

#[test]
fn subdivision_advances_generation() {
    let first = Generation::ROOT.next().expect("root subdivides");
    let second = first.next().expect("first generation subdivides");
    assert_eq!(first.as_u32(), 1);
    assert_eq!(second.as_u32(), 2);
    assert!(Generation::ROOT < first);
    assert!(first < second);
}

#[test]
fn generation_overflow_returns_none() {
    assert_eq!(Generation::MAX.next(), None);
}

#[test]
fn deterministic_ids_roundtrip_raw_values() {
    assert_eq!(deterministic_id(7).as_u64(), 7);
    assert_eq!(NodeId::new(7), deterministic_id(7));
}
