//! Scene ownership contracts.

use planet_crafter_engine::scene::Scene;
use planet_crafter_tests::fixtures::{root_id, sample_scene};

#[test]
fn new_scene_is_empty() {
    let scene = Scene::new();
    assert!(scene.is_empty());
    assert_eq!(scene.node_count(), 0);
}

#[test]
fn insert_tracks_membership() {
    let mut scene = Scene::new();
    scene.insert(root_id());
    assert!(scene.contains(root_id()));
    assert_eq!(scene.node_count(), 1);
}

#[test]
fn duplicate_inserts_are_ignored() {
    let mut scene = sample_scene();
    let count = scene.node_count();
    scene.insert(root_id());
    assert_eq!(scene.node_count(), count);
}
