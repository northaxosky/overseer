//! Tests for the shared Role -> palette styling map

use super::*;

#[test]
fn palette_maps_roles_to_their_semantic_colours() {
    assert_eq!(Role::Success.palette().color, Some(Color::Green));
    assert_eq!(Role::Failure.palette().color, Some(Color::Red));
    assert_eq!(Role::Warning.palette().color, Some(Color::Yellow));
    // Heading and Muted carry emphasis, not colour
    assert_eq!(Role::Heading.palette().color, None);
    assert!(Role::Heading.palette().bold);
    assert!(Role::Muted.palette().dim);
}

#[test]
fn added_and_removed_share_their_base_role_styling() {
    // The aliases must track their primary role so the front ends stay in sync
    assert_eq!(Role::Added.palette(), Role::Success.palette());
    assert_eq!(Role::Removed.palette(), Role::Warning.palette());
}
