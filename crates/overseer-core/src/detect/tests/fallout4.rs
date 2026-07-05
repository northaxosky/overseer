//! Tests for Fallout 4 edition and runtime detection

use super::*;
use crate::test_support::temp;

fn v(major: u16, minor: u16, patch: u16) -> Option<ExeVersion> {
    Some(ExeVersion {
        major,
        minor,
        patch,
        build: 0,
    })
}

#[test]
fn runtime_family_maps_each_generation() {
    assert_eq!(
        runtime_family(v(1, 10, 163).unwrap()),
        Some(Generation::OldGen)
    );
    // 1.10.980 is the obsolete initial NG build (superseded by 984), so it has no supported generation
    assert_eq!(runtime_family(v(1, 10, 980).unwrap()), None);
    assert_eq!(
        runtime_family(v(1, 10, 984).unwrap()),
        Some(Generation::NextGen)
    );
    assert_eq!(
        runtime_family(v(1, 11, 191).unwrap()),
        Some(Generation::Anniversary)
    );
    assert_eq!(
        runtime_family(v(1, 11, 221).unwrap()),
        Some(Generation::Anniversary)
    );
    assert_eq!(runtime_family(v(1, 10, 120).unwrap()), None);
}

#[test]
fn runtime_family_agrees_with_edition_on_obsolete_builds() {
    // Regression: runtime_family and edition once disagreed on 1.10.980 (NextGen vs Obsolete); they now share one table, so an obsolete build is None here and Obsolete there
    assert_eq!(runtime_family(v(1, 10, 980).unwrap()), None);
    assert_eq!(
        edition_from(v(1, 10, 980), StartupBa2Signature::Other),
        Edition::Obsolete
    );
}

#[test]
fn loader_family_maps_f4se_build_lines() {
    assert_eq!(
        loader_family(v(0, 6, 23).unwrap()),
        Some(Generation::OldGen)
    );
    assert_eq!(
        loader_family(v(0, 7, 2).unwrap()),
        Some(Generation::NextGen)
    );
    assert_eq!(
        loader_family(v(0, 7, 7).unwrap()),
        Some(Generation::Anniversary)
    );
    assert_eq!(loader_family(v(9, 9, 9).unwrap()), None);
}

#[test]
fn address_library_name_uses_the_full_version() {
    assert_eq!(
        address_library_name(ExeVersion {
            major: 1,
            minor: 10,
            patch: 163,
            build: 0
        }),
        "version-1-10-163-0.bin"
    );
}

#[test]
fn no_version_is_undetermined() {
    assert_eq!(
        edition_from(None, StartupBa2Signature::Other),
        Edition::Undetermined
    );
}

#[test]
fn old_gen_with_a_matching_startup_is_genuine() {
    assert_eq!(
        edition_from(v(1, 10, 163), StartupBa2Signature::Other),
        Edition::OldGen
    );
}

#[test]
fn old_gen_with_the_next_gen_startup_is_a_downgrade() {
    assert_eq!(
        edition_from(v(1, 10, 163), StartupBa2Signature::NextGen),
        Edition::Downgraded
    );
}

#[test]
fn an_unconfirmed_startup_does_not_force_a_downgrade() {
    // Missing/short/unreadable Startup.ba2 must not be mistaken for the Next-Gen file
    for s in [
        StartupBa2Signature::Missing,
        StartupBa2Signature::TooShort,
        StartupBa2Signature::Unreadable,
    ] {
        assert_eq!(edition_from(v(1, 10, 163), s), Edition::OldGen);
    }
}

#[test]
fn the_current_builds_map_to_their_editions() {
    assert_eq!(
        edition_from(v(1, 10, 984), StartupBa2Signature::Other),
        Edition::NextGen
    );
    assert_eq!(
        edition_from(v(1, 11, 191), StartupBa2Signature::Other),
        Edition::Anniversary
    );
}

#[test]
fn a_newer_unknown_1_11_build_is_still_anniversary() {
    // CMT's table stops at 1.11.191; 1.11.221 is current per F4SE — keep it in the family
    assert_eq!(
        edition_from(v(1, 11, 221), StartupBa2Signature::Other),
        Edition::Anniversary
    );
}

#[test]
fn known_obsolete_builds_are_obsolete() {
    for (maj, min, pat) in [
        (1, 10, 120),
        (1, 10, 130),
        (1, 10, 138),
        (1, 10, 162),
        (1, 10, 980),
        (1, 11, 137),
        (1, 11, 159),
        (1, 11, 169),
    ] {
        assert_eq!(
            edition_from(v(maj, min, pat), StartupBa2Signature::Other),
            Edition::Obsolete,
            "{maj}.{min}.{pat} should be Obsolete"
        );
    }
}

#[test]
fn an_unrecognised_version_is_unknown() {
    assert_eq!(
        edition_from(v(1, 10, 999), StartupBa2Signature::Other),
        Edition::Unknown
    );
    assert_eq!(
        edition_from(v(1, 12, 0), StartupBa2Signature::Other),
        Edition::Unknown
    );
}

fn write_startup(dir: &Utf8Path, bytes: &[u8]) {
    let data = dir.join("Data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join(STARTUP_BA2), bytes).unwrap();
}

#[test]
fn a_missing_startup_is_missing() {
    let (_t, dir) = temp();
    assert_eq!(startup_signature(&dir), StartupBa2Signature::Missing);
}

#[test]
fn a_tiny_startup_is_too_short() {
    let (_t, dir) = temp();
    write_startup(&dir, b"BTDX\x01"); // fewer than 12 bytes
    assert_eq!(startup_signature(&dir), StartupBa2Signature::TooShort);
}

#[test]
fn an_unrelated_startup_is_other() {
    let (_t, dir) = temp();
    write_startup(&dir, b"not the next-gen archive payload at all");
    assert_eq!(startup_signature(&dir), StartupBa2Signature::Other);
}
