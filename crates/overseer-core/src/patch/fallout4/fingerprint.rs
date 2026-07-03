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
    pub fn verify_file(self, file: &FileFingerprint) -> Option<VerifiedBy> {
        self.expected.verify(file)
    }

    /// Whether `file` matches this entry at any available tier
    pub fn matches_file(self, file: &FileFingerprint) -> bool {
        self.expected.matches(file)
    }
}

/// The expected fingerprint for `rel_path` at `generation`, if known
pub fn target_fingerprint(
    generation: Generation,
    rel_path: &str,
) -> Option<&'static BinaryFingerprint> {
    FINGERPRINTS
        .iter()
        .find(|fp| fp.generation == generation && fp.rel_path.eq_ignore_ascii_case(rel_path))
}

/// The known edition whose `rel_path` fingerprint matches `file`, if any
pub fn known_source(rel_path: &str, file: &FileFingerprint) -> Option<&'static BinaryFingerprint> {
    FINGERPRINTS
        .iter()
        .find(|fp| fp.rel_path.eq_ignore_ascii_case(rel_path) && fp.matches_file(file))
}

/// Whether any known fingerprint for `rel_path` has exactly `size` bytes
pub fn any_known_size(rel_path: &str, size: u64) -> bool {
    FINGERPRINTS
        .iter()
        .any(|fp| fp.rel_path.eq_ignore_ascii_case(rel_path) && fp.expected.size == size)
}

/// Whether every core binary has a known target fingerprint for `generation`
pub fn target_table_complete(generation: Generation) -> bool {
    CORE_BINARIES
        .iter()
        .all(|name| target_fingerprint(generation, name).is_some())
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
    // Old-Gen DLC targets -- reproduced from the DLC Consistency Patch deltas, SHA1-cross-checked.
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCCoast.esm",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 74_288_821,
            crc32: 0xFF82_DDC0,
            sha256: Some("19f1da8ed64e1d76e3a8c305fad42df9916b54c7091b24c27c1f19afce931c10"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCCoast.cdx",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 6_915_096,
            crc32: 0xCD51_0AA0,
            sha256: Some("e28c5a979907f7c4025810506bb373cfde588c591b8f8fd2972788a0dcd75088"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCCoast - Geometry.csg",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 100_906_721,
            crc32: 0x5918_0ED1,
            sha256: Some("bdb5138c8dd06bdad94c81e749f4ec3dcf3563fa440fea87b8d31013a73d1724"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCCoast - Main.ba2",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 1_441_660_879,
            crc32: 0x99EF_510E,
            sha256: Some("8c845a06340a60ef3a9cce7fa0ccdcb1f7781348960314910bf10b0c6a1caf21"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCCoast - Textures.ba2",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 1_335_934_657,
            crc32: 0xC881_BE47,
            sha256: Some("22a2d93af46339316f5f38cffee64bac53c8a74243e1c2eac7df2653fa5f918c"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCNukaWorld.esm",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 64_314_071,
            crc32: 0xD6AF_A81A,
            sha256: Some("1cd241b7d182377ef769a8be2c2255491aab37d0f7fa54c14d5b1291c38a2954"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCNukaWorld - Textures.ba2",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 1_794_590_010,
            crc32: 0xB9FD_1CD6,
            sha256: Some("0083e633cfdc9fc34882e41e795f8d16093440197c953d4a68b963fabf62446b"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCworkshop02.esm",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 677_786,
            crc32: 0x537C_3844,
            sha256: Some("3182b8e171877b01d6392d796d1f9233a371d70e773eeeadee80baed0f3ec117"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCworkshop02 - Textures.ba2",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 114_567_019,
            crc32: 0xEFDC_B228,
            sha256: Some("dffe3043bbc504b0b0940646ae93ec4cd77c4e039556b89269505d3a3552fb7a"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCworkshop03.esm",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 2_971_383,
            crc32: 0xD212_20D5,
            sha256: Some("702dc27fc016c991b4b1d449f8a8a40f405a15f0f8be2b0592fe2b61a56c9377"),
        },
    },
    BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "Data/DLCworkshop03 - Textures.ba2",
        build: "1.10.163",
        expected: ExpectedFingerprint {
            size: 128_081_469,
            crc32: 0x5687_96C1,
            sha256: Some("88c31d35584215f162b6b59d4a2f97f01927eff4e654b6fbcd6b10f9758bf23c"),
        },
    },
];

#[cfg(test)]
mod tests {
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
}
