//! Visibility culling outcome contracts.

use planet_crafter_engine::visibility::{CullStats, Visibility};

#[test]
fn outcomes_distinguish_visible_from_culled() {
    assert_ne!(Visibility::Visible, Visibility::Culled);
}

#[test]
fn ratio_reports_visible_fraction() {
    let stats = CullStats::new(4, 3);
    assert_eq!(stats.checked(), 4);
    assert_eq!(stats.visible(), 3);
    assert_eq!(stats.visible_ratio(), Some(0.75));
}

#[test]
fn empty_pass_has_no_ratio() {
    assert_eq!(CullStats::default().visible_ratio(), None);
}

#[test]
fn visible_saturates_at_checked() {
    let stats = CullStats::new(2, 9);
    assert_eq!(stats.visible(), 2);
    assert_eq!(stats.visible_ratio(), Some(1.0));
}
