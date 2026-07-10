//! Tests for application state and update logic

use super::*;

#[test]
fn cycle_variant_wraps_in_both_directions() {
    assert_eq!(cycle_variant(Workspace::Plugins, 1), Workspace::Conflicts);
    assert_eq!(cycle_variant(Workspace::Saves, 1), Workspace::Plugins);
    assert_eq!(cycle_variant(Workspace::Plugins, -1), Workspace::Saves);
}
