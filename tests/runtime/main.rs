//! Headless runtime contracts.

use planet_crafter_engine::runtime::Runtime;
use planet_crafter_tests::fixtures::{sample_scene, ticked_runtime};

#[test]
fn new_runtime_starts_at_frame_zero() {
    let runtime = Runtime::new(sample_scene());
    assert_eq!(runtime.frame().get(), 0);
}

#[test]
fn tick_advances_frame_and_keeps_scene() {
    let runtime = ticked_runtime();
    assert_eq!(runtime.frame().get(), 2);
    assert_eq!(runtime.scene().node_count(), 2);
}
