//! Tests for the Fallout 4 DLC consistency policy

use super::*;
use crate::patch::engine::{self, Policy};
use crate::patch::fingerprint::FileFingerprint;
use crate::test_support::temp;

fn no_known_size(_: &str, _: u64) -> bool {
    false
}

fn no_known_source(_: &str, _: &FileFingerprint) -> Option<String> {
    None
}

fn dlc_policy() -> Policy<'static> {
    Policy {
        groups: DLC_GROUPS,
        target_for: &dlc_target,
        any_known_size: &no_known_size,
        known_source: &no_known_source,
    }
}

#[test]
fn table_has_eleven_rows() {
    assert_eq!(DLC_CONSISTENCY.len(), 11);
}

#[test]
fn every_grouped_file_has_a_target() {
    for group in DLC_GROUPS {
        for &rel in group.files {
            assert!(dlc_target(rel).is_some(), "{rel} has no target");
            assert!(dlc_note(rel).is_some(), "{rel} has no note");
        }
    }
}

#[test]
fn dlc_targets_are_sha256_gated() {
    assert!(DLC_CONSISTENCY.iter().all(|d| d.expected.sha256.is_some()));
}

#[test]
fn recover_install_restores_a_crashed_dlc_sentinel() {
    // A crashed sentinel in its backup slot makes the group look unowned; recovery must still restore it
    let (_tmp, root) = temp();
    std::fs::create_dir_all(root.join("Data")).unwrap();
    std::fs::write(
        root.join("Data/DLCCoast.esm.overseer-bak"),
        b"steam-esm-bytes",
    )
    .unwrap();
    assert!(!root.join("Data/DLCCoast.esm").exists());
    engine::recover_install(&root, &dlc_policy()).unwrap();
    assert_eq!(
        std::fs::read(root.join("Data/DLCCoast.esm")).unwrap(),
        b"steam-esm-bytes"
    );
    assert!(!root.join("Data/DLCCoast.esm.overseer-bak").exists());
}

#[test]
fn known_source_is_never_supplied_by_the_dlc_policy() {
    let policy = dlc_policy();
    let fp = FileFingerprint {
        size: 1,
        crc32: 0,
        sha256: "00".repeat(32),
    };
    assert!((policy.known_source)("Data/DLCCoast.esm", &fp).is_none());
}
