//! Tests for the Fallout 4 BA2 edition policy and patching

use super::*;
use crate::archive::{Ba2Header, Ba2Kind};
use crate::test_support::{ba2_bytes, temp};
use camino::{Utf8Path, Utf8PathBuf};

/// Generation maps to a BA2 edition; Next-Gen and Anniversary share the v7/8 archives
#[test]
fn generation_maps_to_edition() {
    assert_eq!(
        Ba2Edition::from_generation(Generation::OldGen),
        Ba2Edition::OldGen
    );
    assert_eq!(
        Ba2Edition::from_generation(Generation::NextGen),
        Ba2Edition::NextGen
    );
    assert_eq!(
        Ba2Edition::from_generation(Generation::Anniversary),
        Ba2Edition::NextGen
    );
}

fn header(version: u32, kind: Ba2Kind) -> Ba2Header {
    Ba2Header {
        version,
        kind,
        file_count: 0,
    }
}

fn write_ba2(root: &Utf8Path, version: u32, tag: &[u8; 4], body: &[u8]) -> Utf8PathBuf {
    let path = root.join("test.ba2");
    std::fs::write(&path, ba2_bytes(version, tag, body)).expect("write ba2");
    path
}

#[test]
fn edition_mapping() {
    assert_eq!(Ba2Edition::from_version(1), Some(Ba2Edition::OldGen));
    assert_eq!(Ba2Edition::from_version(7), Some(Ba2Edition::NextGen));
    assert_eq!(Ba2Edition::from_version(8), Some(Ba2Edition::NextGen));
    for non_fo4 in [0u32, 2, 3, 9, 999] {
        assert_eq!(Ba2Edition::from_version(non_fo4), None);
    }
    assert_eq!(Ba2Edition::OldGen.target_version(), 1);
    assert_eq!(Ba2Edition::NextGen.target_version(), 8);
}

#[test]
fn plan_downgrades_next_gen_of_either_kind() {
    assert_eq!(
        plan(&header(8, Ba2Kind::General), Ba2Edition::OldGen),
        PatchOutcome::Patched { from: 8, to: 1 }
    );
    assert_eq!(
        plan(&header(7, Ba2Kind::Texture), Ba2Edition::OldGen),
        PatchOutcome::Patched { from: 7, to: 1 }
    );
}

#[test]
fn plan_upgrades_old_gen_to_v8() {
    assert_eq!(
        plan(&header(1, Ba2Kind::General), Ba2Edition::NextGen),
        PatchOutcome::Patched { from: 1, to: 8 }
    );
}

#[test]
fn plan_leaves_v7_alone_when_targeting_next_gen() {
    // v7 is already Next-Gen — we never silently canonicalise it to v8
    assert_eq!(
        plan(&header(7, Ba2Kind::General), Ba2Edition::NextGen),
        PatchOutcome::AlreadyTarget { version: 7 }
    );
}

#[test]
fn plan_reports_already_target() {
    assert_eq!(
        plan(&header(1, Ba2Kind::General), Ba2Edition::OldGen),
        PatchOutcome::AlreadyTarget { version: 1 }
    );
    assert_eq!(
        plan(&header(8, Ba2Kind::Texture), Ba2Edition::NextGen),
        PatchOutcome::AlreadyTarget { version: 8 }
    );
}

#[test]
fn plan_skips_non_fo4_version_or_kind() {
    assert_eq!(
        plan(&header(2, Ba2Kind::General), Ba2Edition::OldGen),
        PatchOutcome::Unsupported {
            version: 2,
            kind: Ba2Kind::General
        }
    );
    let gnmf = Ba2Kind::Other(*b"GNMF");
    assert_eq!(
        plan(&header(1, gnmf), Ba2Edition::OldGen),
        PatchOutcome::Unsupported {
            version: 1,
            kind: gnmf
        }
    );
}

#[test]
fn set_edition_downgrades_and_preserves_the_body() {
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 8, b"GNRL", b"body that must be preserved");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        set_edition(&path, Ba2Edition::OldGen).unwrap(),
        PatchOutcome::Patched { from: 8, to: 1 }
    );
    let patched = std::fs::read(&path).unwrap();
    assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
    let mut restored = patched.clone();
    restored[4..8].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(restored, original);
}

#[test]
fn set_edition_round_trip_is_byte_exact() {
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 8, b"DX10", b"texture body");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        set_edition(&path, Ba2Edition::OldGen).unwrap(),
        PatchOutcome::Patched { from: 8, to: 1 }
    );
    assert_eq!(
        set_edition(&path, Ba2Edition::NextGen).unwrap(),
        PatchOutcome::Patched { from: 1, to: 8 }
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn set_edition_no_ops_a_v7_targeting_next_gen() {
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 7, b"GNRL", b"body");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        set_edition(&path, Ba2Edition::NextGen).unwrap(),
        PatchOutcome::AlreadyTarget { version: 7 }
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn set_edition_skips_a_non_fo4_archive() {
    let (_tmp, root) = temp();
    let path = write_ba2(&root, 2, b"GNRL", b"starfield-ish");
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        set_edition(&path, Ba2Edition::OldGen).unwrap(),
        PatchOutcome::Unsupported {
            version: 2,
            kind: Ba2Kind::General
        }
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
