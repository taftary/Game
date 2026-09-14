//! Game-level smoke test: engine contracts from the game's perspective.

use planet_crafter_tests::fixtures::{sample_scene, ticked_runtime};

#[test]
fn game_scene_boots_and_ticks() {
    let scene = sample_scene();
    assert_eq!(scene.node_count(), 2);

    let runtime = ticked_runtime();
    assert_eq!(runtime.frame().get(), 2);
    assert_eq!(runtime.scene().node_count(), 2);
}
