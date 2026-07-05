//! Tests for plugin error formatting

use super::*;

#[test]
fn non_utf8_path_display_includes_the_offending_value() {
    let err = PluginError::NonUtf8Path("weird\u{FFFD}name".to_string());
    assert!(err.to_string().contains("weird"));
}
