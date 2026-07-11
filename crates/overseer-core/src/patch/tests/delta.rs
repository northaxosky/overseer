//! Tests for the path-based pure-Rust VCDIFF adapter

use super::*;
use crate::test_support::temp;
use std::error::Error as _;

const ADD_ABC: &[u8] = &[
    0xD6, 0xC3, 0xC4, 0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x03, 0x01, 0x00, 0x61, 0x62, 0x63, 0x04,
];

/// Create an empty source and write `delta` beneath a temporary root
fn fixture(delta: &[u8]) -> (tempfile::TempDir, Utf8PathBuf, Utf8PathBuf, Utf8PathBuf) {
    let (tmp, root) = temp();
    let source = root.join("source.bin");
    let delta_path = root.join("patch.vcdiff");
    let dest = root.join("output.bin");
    std::fs::write(&source, []).unwrap();
    std::fs::write(&delta_path, delta).unwrap();
    (tmp, source, delta_path, dest)
}

/// Decode a small VCDIFF file through the path adapter
#[test]
fn decodes_a_small_file() {
    let (_tmp, source, delta, dest) = fixture(ADD_ABC);
    RustDeltaDecoder::new(3)
        .apply(&source, &delta, &dest)
        .unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), b"abc");
}

/// Report the source path when the source cannot be opened
#[test]
fn missing_source_is_path_rich_and_creates_no_destination() {
    let (_tmp, source, delta, dest) = fixture(ADD_ABC);
    std::fs::remove_file(&source).unwrap();
    let err = RustDeltaDecoder::new(3)
        .apply(&source, &delta, &dest)
        .unwrap_err();
    assert!(matches!(
        err,
        DeltaError::OpenSource { ref path, .. } if path == &source
    ));
    assert!(!dest.exists());
}

/// Report the delta path when the delta cannot be opened
#[test]
fn missing_delta_is_path_rich_and_creates_no_destination() {
    let (_tmp, source, delta, dest) = fixture(ADD_ABC);
    std::fs::remove_file(&delta).unwrap();
    let err = RustDeltaDecoder::new(3)
        .apply(&source, &delta, &dest)
        .unwrap_err();
    assert!(matches!(
        err,
        DeltaError::OpenDelta { ref path, .. } if path == &delta
    ));
    assert!(!dest.exists());
}

/// Refuse to overwrite a pre-existing destination
#[test]
fn existing_destination_is_unchanged() {
    let (_tmp, source, delta, dest) = fixture(ADD_ABC);
    std::fs::write(&dest, b"keep me").unwrap();
    let err = RustDeltaDecoder::new(3)
        .apply(&source, &delta, &dest)
        .unwrap_err();
    assert!(matches!(
        err,
        DeltaError::CreateDestination { ref path, .. } if path == &dest
    ));
    assert_eq!(std::fs::read(dest).unwrap(), b"keep me");
}

/// Preserve all three paths and the decoder source for malformed input
#[test]
fn malformed_delta_preserves_the_decode_error_chain() {
    let (_tmp, source, delta, dest) = fixture(b"not a VCDIFF stream");
    let err = RustDeltaDecoder::new(64)
        .apply(&source, &delta, &dest)
        .unwrap_err();
    assert!(matches!(
        err,
        DeltaError::Decode {
            ref source_path,
            ref delta_path,
            ref dest_path,
            ..
        } if source_path == &source && delta_path == &delta && dest_path == &dest
    ));
    assert!(
        err.source()
            .is_some_and(|source| source.downcast_ref::<vcdiff_rs::DecodeError>().is_some())
    );
}

/// Reject a decoded target larger than the caller's explicit limit
#[test]
fn target_size_limit_is_enforced() {
    let (_tmp, source, delta, dest) = fixture(ADD_ABC);
    let err = RustDeltaDecoder::new(2)
        .apply(&source, &delta, &dest)
        .unwrap_err();
    assert!(matches!(
        err,
        DeltaError::Decode {
            source: vcdiff_rs::DecodeError::TargetSizeLimit {
                attempted: 3,
                limit: 2,
                ..
            },
            ..
        }
    ));
}
