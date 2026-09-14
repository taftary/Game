//! LOD level stepping contracts.

use planet_crafter_engine::lod::LodLevel;

#[test]
fn levels_order_coarse_to_fine() {
    assert!(LodLevel::MIN < LodLevel::MAX);
    assert_eq!(LodLevel::default(), LodLevel::MIN);
}

#[test]
fn refine_and_coarsen_step_one_level() {
    let level = LodLevel::new(3);
    assert_eq!(level.refined().map(LodLevel::as_u8), Some(4));
    assert_eq!(level.coarsened().map(LodLevel::as_u8), Some(2));
}

#[test]
fn stepping_saturates_at_bounds() {
    assert_eq!(LodLevel::MAX.refined(), None);
    assert_eq!(LodLevel::MIN.coarsened(), None);
}
