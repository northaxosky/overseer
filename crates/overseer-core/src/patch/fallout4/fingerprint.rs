//! Fallout 4's known core-binary fingerprints, keyed by game edition.

use crate::detect::Generation;
use crate::patch::fingerprint::{ExpectedFingerprint, FileFingerprint, VerifiedBy};

/// The three core binaries an edition swap must convert together
pub const CORE_BINARIES: &[&str] = &["Fallout4.exe", "Fallout4Launcher.exe", "steam_api64.dll"];

/// A known-good identity for one Fallout 4 binary at one edition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryFingerprint {
    pub generation: Generation,
    pub rel_path: &'static str,
    pub build: &'static str,
    pub expected: ExpectedFingerprint,
}

impl BinaryFingerprint {
    /// The edition and build as a display string
    pub fn label(self) -> String {
        format!("{} {}", self.generation.label(), self.build)
    }

    /// The strongest gate this entry can currently enforce
    pub fn verified_by(self) -> VerifiedBy {
        self.expected.verified_by()
    }

    /// Verify `file` against this entry, reporting which hash tier cleared it
    pub fn verify(self, file: &FileFingerprint) -> Option<VerifiedBy> {
        self.expected.verify(file)
    }

    /// Whether `file` matches this entry at any available tier
    pub fn matches(self, file: &FileFingerprint) -> bool {
        self.expected.matches(file)
    }
}

/// The expected fingerprint for `rel` at `generation`, if known
pub fn target_fingerprint(generation: Generation, rel: &str) -> Option<&'static BinaryFingerprint> {
    FINGERPRINTS
        .iter()
        .find(|fp| fp.generation == generation && fp.rel_path.eq_ignore_ascii_case(rel))
}

/// The known edition whose `rel` fingerprint matches `file`, if any
pub fn known_source(rel: &str, file: &FileFingerprint) -> Option<&'static BinaryFingerprint> {
    FINGERPRINTS
        .iter()
        .find(|fp| fp.rel_path.eq_ignore_ascii_case(rel) && fp.matches(file))
}

/// Whether any known fingerprint for `rel` has exactly `size` bytes
pub fn any_known_size(rel: &str, size: u64) -> bool {
    FINGERPRINTS
        .iter()
        .any(|fp| fp.rel_path.eq_ignore_ascii_case(rel) && fp.expected.size == size)
}

/// The verified identity table for every known Fallout 4 core binary
pub static FINGERPRINTS: &[BinaryFingerprint] = &[
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Fallout4.exe",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 65_503_104,
            crc32: 0xC605_3902,
            sha256: Some("55f57947db9e05575122fae1088f0b0247442f11e566b56036caa0ac93329c36"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Fallout4Launcher.exe",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 4_522_496,
            crc32: 0x0244_5570,
            sha256: Some("5e457259dca72c8d1217e2f08a981b630ffd5fe0d30bf28269c8b7898491c6ae"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "steam_api64.dll",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 206_760,
            crc32: 0xBBD9_12FC,
            sha256: Some("81321a5cb72ae3f81243fd0b0d8928a063ca09129ab0878573bd36a28422ec4c"),
        },
    },
    BinaryFingerprint {
        generation: Generation::NextGen,
        rel_path: "Fallout4.exe",
        build: "1.10.984",
        expected: ExpectedFingerprint {
            size: 65_503_104,
            crc32: 0xF95F_9323,
            sha256: Some("23c684ec663b6d5d11a11b3bdf778d79c9e6e8e16ddda95853be31d69a8195b8"),
        },
    },
    BinaryFingerprint {
        generation: Generation::Anniversary,
        rel_path: "Fallout4.exe",
        build: "1.11.221",
        expected: ExpectedFingerprint {
            size: 55_293_864,
            crc32: 0x06FE_A201,
            sha256: Some("428f9996cc4248e26c0f62f9fdd3eaf0e5eb305834b67ee5996538e593218b61"),
        },
    },
    BinaryFingerprint {
        generation: Generation::Anniversary,
        rel_path: "Fallout4Launcher.exe",
        build: "1.11.221",
        expected: ExpectedFingerprint {
            size: 4_533_600,
            crc32: 0xCA61_EDD7,
            sha256: Some("edeee77147b7250261480a0331e99306e6f26e42982ba1d8a9e11585053c8ccd"),
        },
    },
    BinaryFingerprint {
        generation: Generation::Anniversary,
        rel_path: "steam_api64.dll",
        build: "1.11.221",
        expected: ExpectedFingerprint {
            size: 298_384,
            crc32: 0xE36E_7B4D,
            sha256: Some("1db3fd414039d3e5815a5721925dd2e0a3a9f2549603c6cab7c49b84966a1af3"),
        },
    },
];

#[cfg(test)]
#[path = "tests/fingerprint.rs"]
mod tests;
