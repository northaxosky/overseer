//! Tests for the xdelta3 delta decoder and the CRC-32 helper

use super::*;
use crate::test_support::temp;

#[test]
fn decode_args_are_force_decode_with_source() {
    let args = decode_args(
        Utf8Path::new("src.exe"),
        Utf8Path::new("patch.vcdiff"),
        Utf8Path::new("out.exe"),
    );
    assert_eq!(
        args,
        ["-d", "-f", "-s", "src.exe", "patch.vcdiff", "out.exe"].map(String::from)
    );
}

#[test]
fn a_missing_binary_is_a_spawn_error() {
    let decoder = Xdelta3CliDecoder::new("definitely-not-a-real-xdelta3-binary");
    let err = decoder
        .apply(Utf8Path::new("s"), Utf8Path::new("d"), Utf8Path::new("o"))
        .expect_err("a missing binary can't be spawned");
    assert!(matches!(err, DeltaError::Spawn { .. }));
}

#[test]
fn crc32_file_matches_the_standard_vector() {
    let (_tmp, root) = temp();
    let path = root.join("check.bin");
    std::fs::write(&path, b"123456789").unwrap();
    // The canonical CRC-32/ISO-HDLC check value for "123456789"
    assert_eq!(crc32_file(&path).unwrap(), 0xCBF4_3926);
}

// A real xdelta3 round-trip, gated on `OVERSEER_XDELTA3` (CI has no xdelta3):; encode with the binary, decode through the trait, assert byte-exact
#[test]
fn xdelta3_round_trip_is_byte_exact() {
    let Ok(exe) = std::env::var("OVERSEER_XDELTA3") else {
        return;
    };
    let (_tmp, root) = temp();
    let (source, target) = (root.join("source.bin"), root.join("target.bin"));
    let (delta, dest) = (root.join("patch.vcdiff"), root.join("out.bin"));

    std::fs::write(&source, vec![7u8; 20_000]).unwrap();
    let mut t = vec![7u8; 20_000];
    t.splice(5_000..5_000, *b"OVERSEER"); // an insertion, so the delta is non-trivial
    std::fs::write(&target, &t).unwrap();

    let status = Command::new(&exe)
        .args([
            "-e",
            "-f",
            "-s",
            source.as_str(),
            target.as_str(),
            delta.as_str(),
        ])
        .status()
        .expect("run xdelta3 encode");
    assert!(status.success(), "encode should succeed");

    Xdelta3CliDecoder::new(exe)
        .apply(&source, &delta, &dest)
        .expect("decode should succeed");
    assert_eq!(
        crc32_file(&dest).unwrap(),
        crc32_file(&target).unwrap(),
        "decoded output must match the target byte-for-byte"
    );
}
