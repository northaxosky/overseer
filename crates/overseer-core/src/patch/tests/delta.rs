//! Tests for the path-based pure-Rust VCDIFF adapter

use super::*;
use crate::test_support::temp;
use std::error::Error as _;

const ADD_ABC: &[u8] = &[
    0xD6, 0xC3, 0xC4, 0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x03, 0x01, 0x00, 0x61, 0x62, 0x63, 0x04,
];
const VCD_TARGET_ABC: &[u8] = &[
    0xD6, 0xC3, 0xC4, 0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x03, 0x01, 0x00, 0x61, 0x62, 0x63, 0x04,
    0x02, 0x03, 0x00, 0x08, 0x03, 0x00, 0x00, 0x02, 0x01, 0x13, 0x03, 0x00,
];
const DJW_PAYLOAD: &[u8] = include_bytes!("fixtures/vcdiff/djw-one-xdelta-3.2.0.payload.bin");
const DJW_RAW: &[u8] = include_bytes!("fixtures/vcdiff/djw-one-xdelta-3.2.0.raw.bin");
const ID2_DELTA: &[u8] = include_bytes!("fixtures/vcdiff/xdelta-3.2.0-lzma.vcdiff");
const ID2_TARGET: &[u8] = include_bytes!("fixtures/vcdiff/target.bin");

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

/// Encode one VCDIFF big-endian base-128 integer
fn varint(value: u64) -> Vec<u8> {
    let mut bytes = vec![(value & 0x7f) as u8];
    let mut remaining = value >> 7;
    while remaining > 0 {
        bytes.push(0x80 | (remaining & 0x7f) as u8);
        remaining >>= 7;
    }
    bytes.reverse();
    bytes
}

/// Wrap the reviewed external DJW anchor in one DATA-compressed VCDIFF window
fn id1_anchor_delta() -> Vec<u8> {
    assert_eq!(DJW_PAYLOAD.len(), 197);
    assert_eq!(DJW_RAW.len(), 512);

    let mut data = varint(DJW_RAW.len() as u64);
    data.extend_from_slice(DJW_PAYLOAD);
    let mut instructions = vec![1];
    instructions.extend(varint(DJW_RAW.len() as u64));

    let mut encoding = varint(DJW_RAW.len() as u64);
    encoding.push(0x01);
    encoding.extend(varint(data.len() as u64));
    encoding.extend(varint(instructions.len() as u64));
    encoding.push(0);
    encoding.extend(data);
    encoding.extend(instructions);

    let mut delta = vec![0xD6, 0xC3, 0xC4, 0x00, 0x01, 0x01, 0x00];
    delta.extend(varint(encoding.len() as u64));
    delta.extend(encoding);
    delta
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

/// Read prior target bytes through the adapter's readable destination handle
#[test]
fn two_window_vcd_target_decodes_through_adapter() {
    let (_tmp, source, delta, dest) = fixture(VCD_TARGET_ABC);
    RustDeltaDecoder::new(6)
        .apply(&source, &delta, &dest)
        .unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), b"abcabc");
}

/// Decode the reviewed external ID-1 anchor through real files
#[test]
fn external_id1_anchor_decodes_through_adapter() {
    let delta_bytes = id1_anchor_delta();
    assert_eq!(
        &delta_bytes[..7],
        &[0xD6, 0xC3, 0xC4, 0x00, 0x01, 0x01, 0x00]
    );
    let (_tmp, source, delta, dest) = fixture(&delta_bytes);
    RustDeltaDecoder::new(DJW_RAW.len() as u64)
        .apply(&source, &delta, &dest)
        .unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), DJW_RAW);
}

/// Decode the reviewed persistent six-window ID-2 fixture through real files
#[test]
fn persistent_six_window_id2_decodes_through_adapter() {
    assert_eq!(ID2_DELTA.len(), 16_714);
    assert_eq!(ID2_TARGET.len(), 6 * 16_384);
    assert_eq!(&ID2_DELTA[..6], &[0xD6, 0xC3, 0xC4, 0x00, 0x01, 0x02]);
    let (_tmp, source, delta, dest) = fixture(ID2_DELTA);
    RustDeltaDecoder::new(ID2_TARGET.len() as u64)
        .apply(&source, &delta, &dest)
        .unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), ID2_TARGET);
}
