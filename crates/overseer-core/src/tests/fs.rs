//! Tests for the filesystem helpers

use super::*;
use crate::test_support::temp;

#[test]
fn read_opt_is_none_when_missing_and_some_when_present() {
    let (_t, root) = temp();
    assert_eq!(read_to_string_opt(&root.join("nope.txt")).unwrap(), None);
    write(&root.join("a/b.txt"), "hi").unwrap();
    assert_eq!(
        read_to_string_opt(&root.join("a/b.txt"))
            .unwrap()
            .as_deref(),
        Some("hi")
    );
    assert_eq!(read_opt(&root.join("a/b.txt")).unwrap().unwrap(), b"hi");
}

#[test]
fn write_creates_parents_and_atomic_round_trips() {
    let (_t, root) = temp();
    write_atomic(&root.join("deep/x.bin"), b"data").unwrap();
    assert_eq!(
        read_opt(&root.join("deep/x.bin")).unwrap().unwrap(),
        b"data"
    );
}

#[test]
fn remove_file_opt_is_ok_when_absent() {
    let (_t, root) = temp();
    remove_file_opt(&root.join("ghost")).unwrap();
    write(&root.join("real"), "x").unwrap();
    remove_file_opt(&root.join("real")).unwrap();
    assert!(read_opt(&root.join("real")).unwrap().is_none());
}

#[test]
fn backup_corrupt_moves_aside_and_is_noop_when_absent() {
    let (_t, root) = temp();
    backup_corrupt(&root.join("ghost")).unwrap(); // absent: fine
    write(&root.join("c.toml"), "garbage").unwrap();
    backup_corrupt(&root.join("c.toml")).unwrap();
    assert!(
        read_opt(&root.join("c.toml")).unwrap().is_none(),
        "original moved"
    );
    assert_eq!(
        read_opt(&root.join("c.toml.bak")).unwrap().unwrap(),
        b"garbage"
    );
}
