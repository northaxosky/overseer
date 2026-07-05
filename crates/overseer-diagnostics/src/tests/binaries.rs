//! Tests for classifying the game's core binaries by version and CRC

use super::*;

fn version(major: u16, minor: u16, patch: u16, build: u16) -> ExeVersion {
    ExeVersion {
        major,
        minor,
        patch,
        build,
    }
}

#[test]
fn launcher_is_classified_by_crc() {
    assert_eq!(
        classify(LAUNCHER, None, 0x0244_5570),
        Some(BinaryEdition::OldGen)
    );
    assert_eq!(
        classify(LAUNCHER, None, 0xF6A0_6FF5),
        Some(BinaryEdition::NextGen)
    );
    assert_eq!(
        classify(LAUNCHER, None, 0x720B_B9C3),
        Some(BinaryEdition::Anniversary)
    );
    assert_eq!(
        classify(LAUNCHER, None, 0xCA61_EDD7),
        Some(BinaryEdition::Anniversary)
    );
}

#[test]
fn every_obsolete_launcher_crc_maps_to_obsolete() {
    for crc in [0x0E69_6744, 0xD15C_6A49, 0x8C52_BE93, 0x5910_09C9] {
        assert_eq!(classify(LAUNCHER, None, crc), Some(BinaryEdition::Obsolete));
    }
}

#[test]
fn the_launcher_ignores_the_version_and_uses_crc() {
    // Even with a version present, the launcher only trusts its CRC32
    assert_eq!(
        classify(LAUNCHER, Some(version(7, 40, 51, 27)), 0x0244_5570),
        Some(BinaryEdition::OldGen)
    );
}

#[test]
fn an_unknown_launcher_crc_is_unrecognised() {
    assert_eq!(classify(LAUNCHER, None, 0xDEAD_BEEF), None);
}

#[test]
fn steam_api_prefers_the_version_table() {
    assert_eq!(
        classify(STEAM_API, Some(version(2, 89, 45, 4)), 0),
        Some(BinaryEdition::OldGen)
    );
    assert_eq!(
        classify(STEAM_API, Some(version(7, 40, 51, 27)), 0),
        Some(BinaryEdition::NgAe)
    );
}

#[test]
fn steam_api_falls_back_to_crc_when_the_version_is_unknown() {
    assert_eq!(
        classify(STEAM_API, None, 0xBBD9_12FC),
        Some(BinaryEdition::OldGen)
    );
    assert_eq!(
        classify(STEAM_API, None, 0xE36E_7B4D),
        Some(BinaryEdition::NgAe)
    );
    // An unrecognised version must not block the CRC fallback
    assert_eq!(
        classify(STEAM_API, Some(version(9, 9, 9, 9)), 0xBBD9_12FC),
        Some(BinaryEdition::OldGen)
    );
}

#[test]
fn an_unrecognised_steam_api_is_none() {
    assert_eq!(
        classify(STEAM_API, Some(version(9, 9, 9, 9)), 0xDEAD_BEEF),
        None
    );
    assert_eq!(classify(STEAM_API, None, 0), None);
}

#[test]
fn an_unknown_file_name_is_never_classified() {
    assert_eq!(classify("kernel32.dll", None, 0x0244_5570), None);
}

#[test]
fn scan_reports_presence_and_recognition_per_binary() {
    let (_guard, dir) = overseer_core::test_support::temp();
    // Only the launcher exists, with bytes that match no known CRC
    std::fs::write(dir.join(LAUNCHER), b"not a real launcher").unwrap();

    let scans = scan(&dir);
    assert_eq!(scans.len(), 2);

    let launcher = scans.iter().find(|s| s.name == LAUNCHER).unwrap();
    assert!(launcher.present);
    assert!(launcher.readable);
    assert_eq!(launcher.edition, None); // present but unrecognised

    let steam = scans.iter().find(|s| s.name == STEAM_API).unwrap();
    assert!(!steam.present);
    assert!(!steam.readable);
    assert_eq!(steam.edition, None);
}
