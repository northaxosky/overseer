//! The Fallout 4 conversion manifest: which files convert together, and how ownership is decided

use super::fingerprint::target_fingerprint;
use crate::detect::Generation;
use crate::error::IoError;
use crate::fs::size_opt;
use camino::Utf8Path;

/// How Overseer decides whether a group is present and should be converted
pub enum Ownership {
    /// Always converted; every file is required
    Mandatory,
    /// Converted only when this file (a DLC's master) exists on disk
    Sentinel(&'static str),
}

/// A set of files that must convert together to stay internally consistent
pub struct ConvertGroup {
    pub name: &'static str,
    pub ownership: Ownership,
    pub files: &'static [&'static str],
}

impl ConvertGroup {
    /// Whether the group is present on disk (mandatory, or its sentinel exists)
    pub fn is_owned(&self, game_dir: &Utf8Path) -> Result<bool, IoError> {
        match self.ownership {
            Ownership::Mandatory => Ok(true),
            Ownership::Sentinel(rel) => Ok(size_opt(&game_dir.join(rel))?.is_some()),
        }
    }

    /// Whether every file in the group has a known target fingerprint for `target`
    pub fn is_convertible(&self, target: Generation) -> bool {
        self.files
            .iter()
            .all(|rel| target_fingerprint(target, rel).is_some())
    }
}

/// The group that owns `rel_path` if any
pub fn group_of(rel_path: &str) -> Option<&'static ConvertGroup> {
    GROUPS
        .iter()
        .find(|g| g.files.iter().any(|f| f.eq_ignore_ascii_case(rel_path)))
}

/// Every file Overseer knows how to convert, grouped by consistency set
pub static GROUPS: &[ConvertGroup] = &[
    ConvertGroup {
        name: "core",
        ownership: Ownership::Mandatory,
        files: &["Fallout4.exe", "Fallout4Launcher.exe", "steam_api64.dll"],
    },
    ConvertGroup {
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
    ConvertGroup {
        name: "DLCNukaWorld",
        ownership: Ownership::Sentinel("Data/DLCNukaWorld.esm"),
        files: &["Data/DLCNukaWorld.esm", "Data/DLCNukaWorld - Textures.ba2"],
    },
    ConvertGroup {
        name: "DLCworkshop02",
        ownership: Ownership::Sentinel("Data/DLCworkshop02.esm"),
        files: &[
            "Data/DLCworkshop02.esm",
            "Data/DLCworkshop02 - Textures.ba2",
        ],
    },
    ConvertGroup {
        name: "DLCworkshop03",
        ownership: Ownership::Sentinel("Data/DLCworkshop03.esm"),
        files: &[
            "Data/DLCworkshop03.esm",
            "Data/DLCworkshop03 - Textures.ba2",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp;

    fn group(name: &str) -> &'static ConvertGroup {
        GROUPS.iter().find(|g| g.name == name).unwrap()
    }

    #[test]
    fn core_is_mandatory_and_always_owned() {
        let (_tmp, root) = temp();
        assert!(group("core").is_owned(&root).unwrap());
    }

    #[test]
    fn dlc_is_owned_only_when_its_master_exists() {
        let (_tmp, root) = temp();
        let coast = group("DLCCoast");
        assert!(!coast.is_owned(&root).unwrap());
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::write(root.join("Data/DLCCoast.esm"), b"esm").unwrap();
        assert!(coast.is_owned(&root).unwrap());
    }

    #[test]
    fn core_is_convertible_to_shipped_editions_but_not_incomplete_ng() {
        assert!(group("core").is_convertible(Generation::OldGen));
        assert!(group("core").is_convertible(Generation::Anniversary));
        assert!(!group("core").is_convertible(Generation::NextGen));
    }

    #[test]
    fn dlc_groups_are_convertible_to_old_gen_only() {
        for name in ["DLCCoast", "DLCNukaWorld", "DLCworkshop02", "DLCworkshop03"] {
            assert!(group(name).is_convertible(Generation::OldGen), "{name} OG");
            assert!(
                !group(name).is_convertible(Generation::Anniversary),
                "{name} AE"
            );
            assert!(
                !group(name).is_convertible(Generation::NextGen),
                "{name} NG"
            );
        }
    }

    #[test]
    fn group_of_maps_files_to_their_group() {
        assert_eq!(group_of("Fallout4.exe").unwrap().name, "core");
        assert_eq!(group_of("Data/DLCCoast.esm").unwrap().name, "DLCCoast");
        assert_eq!(
            group_of("Data/DLCCoast - Textures.ba2").unwrap().name,
            "DLCCoast"
        );
        assert!(group_of("Data/Unknown.esm").is_none());
    }
}
