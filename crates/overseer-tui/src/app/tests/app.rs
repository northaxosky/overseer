//! Tests for application state and update logic

use super::*;

#[test]
fn select_first_selects_row_zero_only_when_non_empty() {
    let mut list = ListState::default();
    select_first(&mut list, 3);
    assert_eq!(list.selected(), Some(0));
    select_first(&mut list, 0);
    assert_eq!(list.selected(), None);
}

#[test]
fn cycle_variant_wraps_in_both_directions() {
    assert_eq!(cycle_variant(Workspace::Plugins, 1), Workspace::Conflicts);
    assert_eq!(cycle_variant(Workspace::Saves, 1), Workspace::Plugins);
    assert_eq!(cycle_variant(Workspace::Plugins, -1), Workspace::Saves);
}
