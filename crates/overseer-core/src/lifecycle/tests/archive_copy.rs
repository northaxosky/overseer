//! Injectable archive copy seam tests

use std::io::Write;

use super::*;
use crate::test_support::temp;

#[test]
fn failed_external_copy_removes_only_owned_partial() {
    let (_temp, base) = temp();
    let source = base.join("Failure.zip");
    let destination = base.join("Download.zip");
    std::fs::write(&source, b"source").expect("write source");

    let error = copy_new_with(
        &source,
        &destination,
        |_, output| {
            output.write_all(b"partial")?;
            Err(std::io::Error::other("injected copy failure"))
        },
        crate::fs::remove_file_opt,
    )
    .expect_err("copy failure");

    assert!(matches!(error, LifecycleError::Io(_)));
    assert!(!destination.exists());
    assert_eq!(std::fs::read(&source).expect("read source"), b"source");
}

#[test]
fn failed_partial_cleanup_names_retained_download() {
    let (_temp, base) = temp();
    let source = base.join("Retained.zip");
    let destination = base.join("Download.zip");
    std::fs::write(&source, b"source").expect("write source");

    let error = copy_new_with(
        &source,
        &destination,
        |_, output| {
            output.write_all(b"partial")?;
            Err(std::io::Error::other("injected copy failure"))
        },
        |path| Err(operation_failure(path)),
    )
    .expect_err("cleanup failure");

    assert!(matches!(
        &error,
        LifecycleError::PartialCopy { path, .. } if path == &destination
    ));
    assert_eq!(
        std::fs::read(&destination).expect("read partial"),
        b"partial"
    );
    assert!(error.to_string().contains(destination.as_str()));
}

/// Build one path-aware synthetic cleanup failure
fn operation_failure(path: &Utf8Path) -> crate::IoError {
    crate::error::io_err(path, std::io::Error::other("injected cleanup failure"))
}
