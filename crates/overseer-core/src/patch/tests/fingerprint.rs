//! Tests for file fingerprinting and expected-fingerprint verification

use super::*;
use crate::test_support::temp;

const SHA_GATED: ExpectedFingerprint = ExpectedFingerprint {
    size: 3,
    crc32: 0x1234_5678,
    sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
};
const CRC_GATED: ExpectedFingerprint = ExpectedFingerprint {
    size: 3,
    crc32: 0x1234_5678,
    sha256: None,
};

fn file(size: u64, crc32: u32, sha256: &str) -> FileFingerprint {
    FileFingerprint {
        size,
        crc32,
        sha256: sha256.to_owned(),
    }
}

#[test]
fn fingerprint_file_is_none_for_a_missing_path() {
    let (_tmp, root) = temp();
    assert!(fingerprint_file(&root.join("nope.bin")).unwrap().is_none());
}

/// fingerprint_file measures size, CRC32 and SHA-256 in one pass, and the result clears a matching SHA gate
#[test]
fn fingerprint_file_measures_size_crc32_and_sha256() {
    let (_tmp, root) = temp();
    let path = root.join("bytes.bin");
    std::fs::write(&path, b"abc").unwrap();

    let fp = fingerprint_file(&path).unwrap().expect("present");
    assert_eq!(fp.size, 3);
    assert_eq!(fp.crc32, 0x3524_41C2);
    assert_eq!(
        fp.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(SHA_GATED.matches(&fp));
}

#[test]
fn sha_tier_clears_when_the_hash_matches_even_if_crc32_differs() {
    let f = file(
        3,
        0x0000_0000,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    assert_eq!(SHA_GATED.verify(&f), Some(VerifiedBy::Sha256));
    assert_eq!(SHA_GATED.verified_by(), VerifiedBy::Sha256);
}

#[test]
fn sha_gated_identity_rejects_a_crc32_collision() {
    // The point of SHA-256: a same-size, same-CRC32 file with the wrong hash must not verify
    let forged = file(3, 0x1234_5678, &"00".repeat(32));
    assert_eq!(SHA_GATED.verify(&forged), None);
    assert!(!SHA_GATED.matches(&forged));
}

#[test]
fn crc_tier_clears_only_when_no_hash_is_known() {
    let f = file(3, 0x1234_5678, "irrelevant");
    assert_eq!(CRC_GATED.verify(&f), Some(VerifiedBy::Crc32));
    assert_eq!(CRC_GATED.verified_by(), VerifiedBy::Crc32);
}

#[test]
fn a_size_mismatch_never_verifies() {
    let f = file(4, 0x1234_5678, "irrelevant");
    assert_eq!(CRC_GATED.verify(&f), None);
}
