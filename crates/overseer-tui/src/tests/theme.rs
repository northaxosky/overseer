//! Tests for mapping shared roles to concrete ratatui styles

use super::*;

#[test]
fn roles_map_to_distinct_styles() {
    assert_eq!(style(Role::Success), style(Role::Added));
    assert_ne!(style(Role::Success), style(Role::Muted));
}
