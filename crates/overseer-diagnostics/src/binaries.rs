//! Identify the game's core binaries (`Fallout4Launcher.exe`, `steam_api64.dll`) by version/CRC

use camino::Utf8Path;
use overseer_core::detect::{self, ExeVersion};

/// The launcher; classified by CRC32 only, as it carries no usable version resource
const LAUNCHER: &str = "Fallout4Launcher.exe";
/// The Steam API DLL; classified by PE version first, CRC32 as fallback
const STEAM_API: &str = "steam_api64.dll";

/// The core files every managed FO4 install ships, in report order
const BINARIES: [&str; 2] = [LAUNCHER, STEAM_API];

/// Which game generation a single binary belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryEdition {
    /// Old-Gen (`1.10.163`)
    OldGen,
    /// Next-Gen (`1.10.984`)
    NextGen,
    /// Anniversary (`1.11.x`)
    Anniversary,
    /// A build shared by NG and AE that can't be told apart
    NgAe,
    /// A known in-between build nobody cares about
    Obsolete,
}

impl BinaryEdition {
    /// A short label for findings
    pub fn label(self) -> &'static str {
        match self {
            Self::OldGen => "Old-Gen",
            Self::NextGen => "Next-Gen",
            Self::Anniversary => "Anniversary",
            Self::NgAe => "Next-Gen/Anniversary",
            Self::Obsolete => "Obsolete",
        }
    }
}

/// One inspected binary: what it is, and whether it was even there
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinaryScan {
    /// The file name, e.g. `steam_api64.dll`
    pub name: &'static str,
    /// the generation it was classified as, if recognized
    pub edition: Option<BinaryEdition>,
    /// Whether the file exists in the game folder
    pub present: bool,
    /// Whether the file's bytes could be read (false on an IO error while present)
    pub readable: bool,
}

/// Classify a core binary from its name, PE ver, and CRC32
pub fn classify(name: &str, version: Option<ExeVersion>, crc: u32) -> Option<BinaryEdition> {
    match name {
        LAUNCHER => launcher_from_crc(crc),
        STEAM_API => version
            .and_then(steam_api_from_version)
            .or_else(|| steam_api_from_crc(crc)),
        _ => None,
    }
}

/// `Fallout4Launcher.exe` has no usable version resource, so it's keyed purely on CRC32
fn launcher_from_crc(crc: u32) -> Option<BinaryEdition> {
    match crc {
        0x0244_5570 => Some(BinaryEdition::OldGen),
        0xF6A0_6FF5 => Some(BinaryEdition::NextGen),
        0x720B_B9C3 | 0xCA61_EDD7 => Some(BinaryEdition::Anniversary), // 1.11.191, 1.11.221
        0x0E69_6744 | 0xD15C_6A49 | 0x8C52_BE93 | 0x5910_09C9 => Some(BinaryEdition::Obsolete),
        _ => None,
    }
}

/// `steam_api64.dll` carries a reliable file version we can map directly
fn steam_api_from_version(version: ExeVersion) -> Option<BinaryEdition> {
    match (version.major, version.minor, version.patch, version.build) {
        (2, 89, 45, 4) => Some(BinaryEdition::OldGen),
        (7, 40, 51, 27) => Some(BinaryEdition::NgAe),
        _ => None,
    }
}

/// CRC32 fallback for `steam_api64.dll` when the version resource is unreadable
fn steam_api_from_crc(crc: u32) -> Option<BinaryEdition> {
    match crc {
        0xBBD9_12FC => Some(BinaryEdition::OldGen),
        0xE36E_7B4D => Some(BinaryEdition::NgAe),
        _ => None,
    }
}

/// Inspect both core binaries in `game_dir`
pub fn scan(game_dir: &Utf8Path) -> Vec<BinaryScan> {
    BINARIES
        .iter()
        .map(|&name| scan_one(game_dir, name))
        .collect()
}

/// Inspect one binary: existence, PE version, and CRC32
fn scan_one(game_dir: &Utf8Path, name: &'static str) -> BinaryScan {
    let path = game_dir.join(name);
    if !path.exists() {
        return BinaryScan {
            name,
            edition: None,
            present: false,
            readable: false,
        };
    }
    let version = detect::file_version(&path);
    let bytes = std::fs::read(&path);
    let readable = bytes.is_ok();
    let crc = bytes.map(|b| crc32fast::hash(&b)).unwrap_or(0);
    BinaryScan {
        name,
        edition: classify(name, version, crc),
        present: true,
        readable,
    }
}

#[cfg(test)]
#[path = "tests/binaries.rs"]
mod tests;
