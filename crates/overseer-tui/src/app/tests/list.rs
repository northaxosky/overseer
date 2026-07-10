//! Tests for shared Ratatui list cursor state

use super::*;

#[test]
fn first_selects_only_non_empty_lists() {
    assert_eq!(ListCursor::first(3).index(), Some(0));
    assert_eq!(ListCursor::first(0).index(), None);
}

#[test]
fn select_first_preserves_a_non_empty_scroll_offset() {
    let mut selection = ListCursor::first(8);
    *selection.state_mut().offset_mut() = 4;

    selection.select_first(8);

    assert_eq!(selection.index(), Some(0));
    assert_eq!(selection.state_mut().offset(), 4);
}

#[test]
fn select_first_deselects_an_empty_list() {
    let mut selection = ListCursor::first(3);
    selection.select_first(0);
    assert_eq!(selection.index(), None);
}

#[test]
fn reset_first_replaces_state_and_resets_scroll_offset() {
    let mut selection = ListCursor::first(8);
    *selection.state_mut().offset_mut() = 4;

    selection.reset_first(8);

    assert_eq!(selection.index(), Some(0));
    assert_eq!(selection.state_mut().offset(), 0);
}

#[test]
fn empty_reset_and_clamp_deselect() {
    let mut selection = ListCursor::first(3);
    selection.reset_first(0);
    assert_eq!(selection.index(), None);

    selection.select(Some(2));
    selection.clamp(0);
    assert_eq!(selection.index(), None);
}

#[test]
fn clamp_bounds_an_existing_selection_without_inventing_one() {
    let mut selection = ListCursor::default();
    selection.clamp(3);
    assert_eq!(selection.index(), None);

    selection.select(Some(7));
    selection.clamp(3);
    assert_eq!(selection.index(), Some(2));
}

#[test]
fn movement_clamps_without_wrapping() {
    let mut selection = ListCursor::first(3);
    selection.move_by(3, -1);
    assert_eq!(selection.index(), Some(0));

    selection.move_by(3, 8);
    assert_eq!(selection.index(), Some(2));

    selection.move_by(3, 1);
    assert_eq!(selection.index(), Some(2));
}

#[test]
fn movement_treats_no_selection_as_zero() {
    let mut selection = ListCursor::default();
    selection.move_by(3, 1);
    assert_eq!(selection.index(), Some(1));
}

#[test]
fn empty_movement_is_a_no_op() {
    let mut selection = ListCursor::first(3);
    *selection.state_mut().offset_mut() = 2;

    selection.move_by(0, 1);

    assert_eq!(selection.index(), Some(0));
    assert_eq!(selection.state_mut().offset(), 2);
}
