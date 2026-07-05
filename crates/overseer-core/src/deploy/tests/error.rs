//! Tests for deploy error types and their messages

use super::*;
use camino::Utf8Path;

#[test]
fn io_err_attaches_path_and_preserves_source_kind() {
    let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
    let err = io_err(Utf8Path::new("C:/x/y.dds"), source);
    assert_eq!(err.path, Utf8PathBuf::from("C:/x/y.dds"));
    assert_eq!(err.source.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn missing_staging_display_mentions_mod_and_path() {
    let err = DeployError::MissingStaging {
        mod_name: "CoolMod".to_string(),
        path: Utf8PathBuf::from("C:/mods/CoolMod"),
    };
    let text = err.to_string();
    assert!(text.contains("CoolMod"));
    assert!(text.contains("C:/mods/CoolMod"));
}

#[test]
fn non_utf8_path_display_includes_the_offending_value() {
    let err = DeployError::NonUtf8Path("weird\u{FFFD}name".to_string());
    assert!(err.to_string().contains("weird"));
}
