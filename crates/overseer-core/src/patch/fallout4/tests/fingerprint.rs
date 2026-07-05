//! Tests for the Fallout 4 core-binary fingerprint table

use super::*;

#[test]
fn ng_target_table_is_incomplete_until_all_core_binaries_are_known() {
    assert!(!target_table_complete(Generation::NextGen));
    assert!(target_table_complete(Generation::OldGen));
    assert!(target_table_complete(Generation::Anniversary));
}

#[test]
fn every_known_binary_is_sha256_gated() {
    assert!(
        FINGERPRINTS
            .iter()
            .all(|fp| fp.verified_by() == VerifiedBy::Sha256),
        "all fingerprints should be SHA-256-gated once the OG binaries are recorded"
    );
}

#[test]
fn known_source_identifies_the_edition_of_a_matching_file() {
    let exe = target_fingerprint(Generation::Anniversary, "Fallout4.exe").unwrap();
    let file = FileFingerprint {
        size: exe.expected.size,
        crc32: exe.expected.crc32,
        sha256: exe.expected.sha256.unwrap().to_owned(),
    };
    assert_eq!(
        known_source("Fallout4.exe", &file).unwrap().generation,
        Generation::Anniversary
    );
}

#[test]
fn any_known_size_matches_only_recorded_sizes() {
    let exe = target_fingerprint(Generation::OldGen, "Fallout4.exe").unwrap();
    assert!(any_known_size("Fallout4.exe", exe.expected.size));
    assert!(!any_known_size("Fallout4.exe", exe.expected.size + 1));
    assert!(!any_known_size("Data/Unknown.esm", exe.expected.size));
}

#[test]
fn a_sha_backed_binary_rejects_a_crc32_collision() {
    let exe = target_fingerprint(Generation::OldGen, "Fallout4.exe").unwrap();
    let forged = FileFingerprint {
        size: exe.expected.size,
        crc32: exe.expected.crc32,
        sha256: "00".repeat(32),
    };
    assert_eq!(exe.verify_file(&forged), None);
    assert!(!exe.matches_file(&forged));
}

#[test]
fn label_combines_edition_and_build() {
    let exe = target_fingerprint(Generation::Anniversary, "Fallout4.exe").unwrap();
    assert!(exe.label().contains("1.11.221"));
}
