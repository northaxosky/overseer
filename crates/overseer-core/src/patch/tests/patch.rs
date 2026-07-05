//! Tests for the game-agnostic BA2 version-field patch

use super::*;
use crate::archive::Ba2Error;
use crate::test_support::{ba2_bytes, temp};
use camino::{Utf8Path, Utf8PathBuf};

fn write_ba2(root: &Utf8Path, version: u32, tag: &[u8; 4], body: &[u8]) -> Utf8PathBuf {
    let path = root.join("test.ba2");
    std::fs::write(&path, ba2_bytes(version, tag, body)).expect("write ba2");
    path
}

#[test]
fn flips_only_the_version_field() {
    let body = b"archive body that must survive untouched";
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 8, b"GNRL", body);
    let original = std::fs::read(&path).unwrap();

    assert_eq!(
        set_version(&path, 1).unwrap(),
        VersionChange::Changed { from: 8, to: 1 }
    );

    let patched = std::fs::read(&path).unwrap();
    assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
    // Restoring just the version field reproduces the original byte-for-byte
    let mut restored = patched.clone();
    restored[4..8].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(restored, original);
}

#[test]
fn unchanged_when_already_at_the_value() {
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 1, b"GNRL", b"body");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        set_version(&path, 1).unwrap(),
        VersionChange::Unchanged { version: 1 }
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn sets_any_value_without_judging_it() {
    // The mechanism is game-agnostic: which versions are valid is the caller's policy
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 8, b"DX10", b"body");
    assert_eq!(
        set_version(&path, 3).unwrap(),
        VersionChange::Changed { from: 8, to: 3 }
    );
    assert_eq!(
        &std::fs::read(&path).unwrap()[4..8],
        3u32.to_le_bytes().as_slice()
    );
}

#[test]
fn rejects_a_non_ba2_file() {
    let (_tmp, root) = temp();
    let path = root.join("x.ba2");
    std::fs::write(&path, b"NOPE plus enough trailing bytes to fill a header").unwrap();
    assert!(matches!(set_version(&path, 1), Err(Ba2Error::BadMagic)));
}

#[test]
fn rejects_a_too_short_file() {
    let (_tmp, root) = temp();
    let path = root.join("x.ba2");
    std::fs::write(&path, b"BTDX").unwrap();
    assert!(matches!(set_version(&path, 1), Err(Ba2Error::TooShort)));
}
