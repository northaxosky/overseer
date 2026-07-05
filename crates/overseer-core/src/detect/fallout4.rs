//! fallout 4 specific edition detection: the exe version table and the `Startup.ba2` down-grade tripwire

use super::ExeVersion;
use crate::deploy::DATA_DIR;
use camino::Utf8Path;

const STARTUP_BA2: &str = "Fallout4 - Startup.ba2";

/// CRC32 of the NG `Startup.ba2` after its BA2 header
const NG_STARTUP_CRC: u32 = 0xA580_8F5F;
// CRC start offset (past magic + version + tag); excluding the version field keeps a down-patched NG archive matchable
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
    /// The [`Generation`] this edition belongs to, or `None` for obsolete/unknown/undetermined builds
    pub fn generation(self) -> Option<Generation> {
        match self {
            Edition::OldGen | Edition::Downgraded => Some(Generation::OldGen),
            Edition::NextGen => Some(Generation::NextGen),
            Edition::Anniversary => Some(Generation::Anniversary),
            Edition::Obsolete | Edition::Unknown | Edition::Undetermined => None,
        }
    }
}

/// A Fallout 4 "generation" — the canonical OG / NG / AE vocabulary shared across detection, patching, and diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    OldGen,
    NextGen,
    Anniversary,
}

impl Generation {
    /// A short lower-case tag (`og` / `ng` / `ae`), for CLI args and terse output
    pub fn tag(self) -> &'static str {
        match self {
            Generation::OldGen => "og",
            Generation::NextGen => "ng",
            Generation::Anniversary => "ae",
        }
    }

    /// A label (`Old-Gen` / `Next-Gen` / `Anniversary`)
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

/// The runtime generation of `v`, derived from its [`Edition`] so it can't drift from classification
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

/// How confident we are that the base-game `Startup.ba2` is the NG one
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

/// The single version→[`Edition`] table; `edition_from` layers the down-grade tripwire on top
fn edition_by_version(v: ExeVersion) -> Edition {
    match (v.major, v.minor, v.patch) {
        (1, 10, 163) => Edition::OldGen,
        (1, 10, 984) => Edition::NextGen,
        triple if KNOWN_OBSOLETE.contains(&triple) => Edition::Obsolete,
        (1, 11, _) => Edition::Anniversary,
        _ => Edition::Unknown,
    }
}

/// Map version + the down-grade signal to an [`Edition`]: an OG exe with the NG `Startup.ba2` is `Downgraded`
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

#[cfg(test)]
#[path = "tests/fallout4.rs"]
mod tests;
