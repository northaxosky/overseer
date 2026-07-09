//! The DLC consistency policy: bring Fallout 4's DLC to the cross-storefront consistency revision.
//!
//! Unlike an edition flip, this is a curated consistency revision: masters are upgraded to the
//! correct Steam/Gamepass base and textures are brought to the 2K-downscaled revision. Each file
//! carries an honest provenance `note` instead of an edition label.

use crate::patch::engine::{ConvertItem, GroupSpec, Ownership, TargetSpec};
use crate::patch::fingerprint::ExpectedFingerprint;

/// A DLC file's target identity in the consistency revision, plus its provenance note
pub struct DlcFingerprint {
    pub rel_path: &'static str,
    pub expected: ExpectedFingerprint,
    pub note: &'static str,
}

/// The target identity for `rel` in the consistency revision, if this DLC file is known
pub fn dlc_target(rel: &str) -> Option<TargetSpec> {
    DLC_CONSISTENCY
        .iter()
        .find(|d| d.rel_path.eq_ignore_ascii_case(rel))
        .map(|d| TargetSpec {
            rel_path: d.rel_path,
            expected: d.expected,
        })
}

/// The provenance note for `rel` in the consistency revision, if known
pub fn dlc_note(rel: &str) -> Option<&'static str> {
    DLC_CONSISTENCY
        .iter()
        .find(|d| d.rel_path.eq_ignore_ascii_case(rel))
        .map(|d| d.note)
}

/// Build a single DLC convert item for `rel`, if the file is known
pub fn explicit_item(rel: &str) -> Option<ConvertItem> {
    let target = dlc_target(rel)?;
    let group = DLC_GROUPS
        .iter()
        .find(|g| g.files.iter().any(|f| f.eq_ignore_ascii_case(rel)))?;
    Some(ConvertItem {
        rel_path: target.rel_path,
        target,
        group: group.name,
    })
}

/// The eleven DLC targets that make up the consistency revision
pub static DLC_CONSISTENCY: &[DlcFingerprint] = &[
    DlcFingerprint {
        rel_path: "Data/DLCCoast.esm",
        expected: ExpectedFingerprint {
            size: 74_288_821,
            crc32: 0xFF82_DDC0,
            sha256: Some("19f1da8ed64e1d76e3a8c305fad42df9916b54c7091b24c27c1f19afce931c10"),
        },
        note: "corrected master",
    },
    DlcFingerprint {
        rel_path: "Data/DLCCoast.cdx",
        expected: ExpectedFingerprint {
            size: 6_915_096,
            crc32: 0xCD51_0AA0,
            sha256: Some("e28c5a979907f7c4025810506bb373cfde588c591b8f8fd2972788a0dcd75088"),
        },
        note: "corrected index",
    },
    DlcFingerprint {
        rel_path: "Data/DLCCoast - Geometry.csg",
        expected: ExpectedFingerprint {
            size: 100_906_721,
            crc32: 0x5918_0ED1,
            sha256: Some("bdb5138c8dd06bdad94c81e749f4ec3dcf3563fa440fea87b8d31013a73d1724"),
        },
        note: "corrected geometry",
    },
    DlcFingerprint {
        rel_path: "Data/DLCCoast - Main.ba2",
        expected: ExpectedFingerprint {
            size: 1_441_660_879,
            crc32: 0x99EF_510E,
            sha256: Some("8c845a06340a60ef3a9cce7fa0ccdcb1f7781348960314910bf10b0c6a1caf21"),
        },
        note: "corrected archive",
    },
    DlcFingerprint {
        rel_path: "Data/DLCCoast - Textures.ba2",
        expected: ExpectedFingerprint {
            size: 1_335_934_657,
            crc32: 0xC881_BE47,
            sha256: Some("22a2d93af46339316f5f38cffee64bac53c8a74243e1c2eac7df2653fa5f918c"),
        },
        note: "2K textures",
    },
    DlcFingerprint {
        rel_path: "Data/DLCNukaWorld.esm",
        expected: ExpectedFingerprint {
            size: 64_314_071,
            crc32: 0xD6AF_A81A,
            sha256: Some("1cd241b7d182377ef769a8be2c2255491aab37d0f7fa54c14d5b1291c38a2954"),
        },
        note: "corrected master",
    },
    DlcFingerprint {
        rel_path: "Data/DLCNukaWorld - Textures.ba2",
        expected: ExpectedFingerprint {
            size: 1_794_590_010,
            crc32: 0xB9FD_1CD6,
            sha256: Some("0083e633cfdc9fc34882e41e795f8d16093440197c953d4a68b963fabf62446b"),
        },
        note: "2K textures",
    },
    DlcFingerprint {
        rel_path: "Data/DLCworkshop02.esm",
        expected: ExpectedFingerprint {
            size: 677_786,
            crc32: 0x537C_3844,
            sha256: Some("3182b8e171877b01d6392d796d1f9233a371d70e773eeeadee80baed0f3ec117"),
        },
        note: "corrected master",
    },
    DlcFingerprint {
        rel_path: "Data/DLCworkshop02 - Textures.ba2",
        expected: ExpectedFingerprint {
            size: 114_567_019,
            crc32: 0xEFDC_B228,
            sha256: Some("dffe3043bbc504b0b0940646ae93ec4cd77c4e039556b89269505d3a3552fb7a"),
        },
        note: "2K textures",
    },
    DlcFingerprint {
        rel_path: "Data/DLCworkshop03.esm",
        expected: ExpectedFingerprint {
            size: 2_971_383,
            crc32: 0xD212_20D5,
            sha256: Some("702dc27fc016c991b4b1d449f8a8a40f405a15f0f8be2b0592fe2b61a56c9377"),
        },
        note: "corrected master",
    },
    DlcFingerprint {
        rel_path: "Data/DLCworkshop03 - Textures.ba2",
        expected: ExpectedFingerprint {
            size: 128_081_469,
            crc32: 0x5687_96C1,
            sha256: Some("88c31d35584215f162b6b59d4a2f97f01927eff4e654b6fbcd6b10f9758bf23c"),
        },
        note: "2K textures",
    },
];

/// The four DLC consistency groups, each owned when its master `.esm` is present
pub static DLC_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        name: "DLCCoast",
        ownership: Ownership::Sentinel("Data/DLCCoast.esm"),
        files: &[
            "Data/DLCCoast.esm",
            "Data/DLCCoast.cdx",
            "Data/DLCCoast - Geometry.csg",
            "Data/DLCCoast - Main.ba2",
            "Data/DLCCoast - Textures.ba2",
        ],
    },
    GroupSpec {
        name: "DLCNukaWorld",
        ownership: Ownership::Sentinel("Data/DLCNukaWorld.esm"),
        files: &["Data/DLCNukaWorld.esm", "Data/DLCNukaWorld - Textures.ba2"],
    },
    GroupSpec {
        name: "DLCworkshop02",
        ownership: Ownership::Sentinel("Data/DLCworkshop02.esm"),
        files: &[
            "Data/DLCworkshop02.esm",
            "Data/DLCworkshop02 - Textures.ba2",
        ],
    },
    GroupSpec {
        name: "DLCworkshop03",
        ownership: Ownership::Sentinel("Data/DLCworkshop03.esm"),
        files: &[
            "Data/DLCworkshop03.esm",
            "Data/DLCworkshop03 - Textures.ba2",
        ],
    },
];

#[cfg(test)]
#[path = "tests/dlc.rs"]
mod tests;
