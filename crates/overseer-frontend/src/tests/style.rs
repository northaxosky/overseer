//! Tests for the shared Role -> style map

use super::*;

#[test]
fn style_maps_roles_to_their_semantic_colours() {
    assert_eq!(Role::Success.style().color, Some(Color::Green));
    assert_eq!(Role::Failure.style().color, Some(Color::Red));
    assert_eq!(Role::Warning.style().color, Some(Color::Yellow));
    // Heading and Muted carry emphasis, not colour
    assert_eq!(Role::Heading.style().color, None);
    assert!(Role::Heading.style().bold);
    assert!(Role::Muted.style().dim);
}

#[test]
fn added_and_removed_share_their_base_role_styling() {
    // The aliases must track their primary role so the front ends stay in sync
    assert_eq!(Role::Added.style(), Role::Success.style());
    assert_eq!(Role::Removed.style(), Role::Warning.style());
}
