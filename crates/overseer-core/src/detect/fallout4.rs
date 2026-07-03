//! fallout 4 specific edition detection: the exe version table and the `Startup.ba2` down-grade tripwire

use super::ExeVersion;
use crate::deploy::DATA_DIR;
use camino::Utf8Path;

const STARTUP_BA2: &str = "Fallout4 - Startup.ba2";

/// CRC32 of the NG `Startup.ba2` after its BA2 header
const NG_STARTUP_CRC: u32 = 0xA580_8F5F;
const BA2_HEADER_LEN: usize = 12;

/// Which Fallout 4 exe generation is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// `1.10.163.0`, genuine Old-Gen
    OldGen,
    /// `1.10.163.0` exe but NG/AE `Startup.ba2`: down-patched install
    Downgraded,
    /// `1.10.984.x` Next-Gen edition
    NextGen,
    /// The `1.11.x` family (AE)
    Anniversary,
    /// Some rando in-between build nobody should use
    Obsolete,
    /// The exe parsed but its version isn't one we recognize
    Unknown,
    /// The exe is missing, unreadable, or has no version resource
    Undetermined,
}

impl Edition {
    /// The [`Generation`] this edition belongs to, or `None` for obsolete/unknown/undetermined builds.
    pub fn generation(self) -> Option<Generation> {
        match self {
            Edition::OldGen | Edition::Downgraded => Some(Generation::OldGen),
            Edition::NextGen => Some(Generation::NextGen),
            Edition::Anniversary => Some(Generation::Anniversary),
            Edition::Obsolete | Edition::Unknown | Edition::Undetermined => None,
        }
    }
}

/// A Fallout 4 "generation" — the canonical OG / NG / AE vocabulary shared across detection, patching, and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    OldGen,
    NextGen,
    Anniversary,
}

impl Generation {
    /// A short lower-case tag (`og` / `ng` / `ae`), for CLI args and terse output.
    pub fn tag(self) -> &'static str {
        match self {
            Generation::OldGen => "og",
            Generation::NextGen => "ng",
            Generation::Anniversary => "ae",
        }
    }

    /// A label (`Old-Gen` / `Next-Gen` / `Anniversary`).
    pub fn label(self) -> &'static str {
        match self {
            Generation::OldGen => "Old-Gen",
            Generation::NextGen => "Next-Gen",
            Generation::Anniversary => "Anniversary",
        }
    }
}

impl std::fmt::Display for Generation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The runtime generation of `v`, derived from its [`Edition`] so it can't drift from classification.
pub fn runtime_family(v: ExeVersion) -> Option<Generation> {
    edition_by_version(v).generation()
}

/// The runtime family an `f4se_loader.exe` build targets (OG 0.6.x, NG 0.7.2, AE 0.7.7+)
pub fn loader_family(v: ExeVersion) -> Option<Generation> {
    match (v.major, v.minor, v.patch) {
        (0, 6, _) => Some(Generation::OldGen),
        (0, 7, 0..=6) => Some(Generation::NextGen),
        (0, 7, _) => Some(Generation::Anniversary),
        _ => None,
    }
}

/// The address Library filename the engine expects for `v`, under `Data/F4SE/Plugins`
pub fn address_library_name(v: ExeVersion) -> String {
    format!(
        "version-{}-{}-{}-{}.bin",
        v.major, v.minor, v.patch, v.build
    )
}

/// The exe version packed the way F4SE stores it in a plugin's `compatibleVersions`
pub fn packed_runtime(v: ExeVersion) -> u32 {
    (u32::from(v.major) << 24) | (u32::from(v.minor) << 16) | (u32::from(v.patch) << 4)
}

/// How confident we are that the base-game `Startup.ba2` is the NG one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupBa2Signature {
    NextGen,
    Other,
    Missing,
    TooShort,
    Unreadable,
}

/// Known in-between builds no-one should run
const KNOWN_OBSOLETE: &[(u16, u16, u16)] = &[
    (1, 10, 120),
    (1, 10, 130),
    (1, 10, 138),
    (1, 10, 162),
    (1, 10, 980),
    (1, 11, 137),
    (1, 11, 159),
    (1, 11, 169),
];

/// Classify the Fallout 4 [`Edition`] at `game_dir`
pub fn classify_edition(version: Option<ExeVersion>, game_dir: &Utf8Path) -> Edition {
    edition_from(version, startup_signature(game_dir))
}

/// The single version→[`Edition`] table; `edition_from` layers the down-grade tripwire on top.
fn edition_by_version(v: ExeVersion) -> Edition {
    match (v.major, v.minor, v.patch) {
        (1, 10, 163) => Edition::OldGen,
        (1, 10, 984) => Edition::NextGen,
        triple if KNOWN_OBSOLETE.contains(&triple) => Edition::Obsolete,
        (1, 11, _) => Edition::Anniversary,
        _ => Edition::Unknown,
    }
}

/// Map version + the down-grade signal to an [`Edition`]: an OG exe with the NG `Startup.ba2` is `Downgraded`.
fn edition_from(version: Option<ExeVersion>, startup: StartupBa2Signature) -> Edition {
    let Some(v) = version else {
        return Edition::Undetermined;
    };
    match edition_by_version(v) {
        Edition::OldGen if startup == StartupBa2Signature::NextGen => Edition::Downgraded,
        other => other,
    }
}

/// Fingerprint `Data/Fallout4 - Startup.ba2`: CRC32 of the bytes after the BA2 header
fn startup_signature(game_dir: &Utf8Path) -> StartupBa2Signature {
    let path = game_dir.join(DATA_DIR).join(STARTUP_BA2);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StartupBa2Signature::Missing,
        Err(_) => return StartupBa2Signature::Unreadable,
    };
    if bytes.len() <= BA2_HEADER_LEN {
        return StartupBa2Signature::TooShort;
    }
    if crc32fast::hash(&bytes[BA2_HEADER_LEN..]) == NG_STARTUP_CRC {
        StartupBa2Signature::NextGen
    } else {
        StartupBa2Signature::Other
    }
}

// ---------------------------------------------------------------------------; Tests; ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        // 1.10.980 is the obsolete initial NG build (superseded by 984), so it has no supported generation.
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
        // Regression: runtime_family and edition once disagreed on 1.10.980 (NextGen vs Obsolete); they now share one table, so an obsolete build is None here and Obsolete there.
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
        // Missing/short/unreadable Startup.ba2 must not be mistaken for the Next-Gen file.
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
        // CMT's table stops at 1.11.191; 1.11.221 is current per F4SE — keep it in the family.
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
}
