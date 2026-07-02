//! Fallout 4 whole install edition conversion: applying binary deltas to swap game binaries between gens

use crate::error::IoError;
use crate::fs::size_opt;
use crate::patch::delta::crc32_file;
use camino::Utf8Path;

/// Which edition to convert an install toward
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Downgrade to Old-Gen (1.10.163); the build most F4SE mods target
    ToOldGen,
}

/// A convertible game file, identified by its known-clean *target* fingerprint
#[derive(Debug, Clone, Copy)]
pub struct ConvertItem {
    /// Path relative to the game directory
    pub rel_path: &'static str,
    /// Size in bytes of the clean target file
    pub target_size: u64,
    /// CRC32 of the clean target file
    pub target_crc: u32,
}

/// The AE/NG -> OG Downgrade set: the three core binaries
pub const OLD_GEN_ITEMS: &[ConvertItem] = &[
    ConvertItem {
        rel_path: "Fallout4.exe",
        target_size: 65_503_104,
        target_crc: 0xC605_3902,
    },
    ConvertItem {
        rel_path: "Fallout4Launcher.exe",
        target_size: 4_522_496,
        target_crc: 0x0244_5570,
    },
    ConvertItem {
        rel_path: "steam_api64.dll",
        target_size: 206_760,
        target_crc: 0xBBD9_12FC,
    },
];

/// Where one game file stands relative to its target edition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Already the clean target file; nothing to do
    AlreadyTarget,
    /// Present but not the target; a delta can convert it
    NeedsConversion,
    /// Absent from the game directory
    Missing,
}

/// The manifest of files to convert for `direction`
pub fn items(direction: Direction) -> &'static [ConvertItem] {
    match direction {
        Direction::ToOldGen => OLD_GEN_ITEMS,
    }
}

/// Classify an on-disk `(size, crc)` against an item's target
fn identify(on_disk: Option<(u64, u32)>, item: &ConvertItem) -> ItemState {
    match on_disk {
        None => ItemState::Missing,
        Some((size, crc)) if size == item.target_size && crc == item.target_crc => {
            ItemState::AlreadyTarget
        }
        Some(_) => ItemState::NeedsConversion,
    }
}

/// Classify one item against the file in `game_dir`, hashing only when the size already matches
pub fn classify(game_dir: &Utf8Path, item: &ConvertItem) -> Result<ItemState, IoError> {
    let path = game_dir.join(item.rel_path);
    let Some(size) = size_opt(&path)? else {
        return Ok(ItemState::Missing);
    };
    // A size mismatch can't be the target, skip hashing
    if size != item.target_size {
        return Ok(ItemState::NeedsConversion);
    }
    Ok(identify(Some((size, crc32_file(&path)?)), item))
}

/// Classify every item in `direction`'s manifest against `game_dir`
pub fn plan(
    game_dir: &Utf8Path,
    direction: Direction,
) -> Result<Vec<(&'static ConvertItem, ItemState)>, IoError> {
    items(direction)
        .iter()
        .map(|item| Ok((item, classify(game_dir, item)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp;

    // A tiny synthetic item whose target is the bytes "123456789".
    const ITEM: ConvertItem = ConvertItem {
        rel_path: "check.bin",
        target_size: 9,
        target_crc: 0xCBF4_3926,
    };

    #[test]
    fn identify_maps_the_three_states() {
        assert_eq!(identify(None, &ITEM), ItemState::Missing);
        assert_eq!(
            identify(Some((9, 0xCBF4_3926)), &ITEM),
            ItemState::AlreadyTarget
        );
        // Right size, wrong crc — a different build that happens to share a size.
        assert_eq!(
            identify(Some((9, 0xDEAD_BEEF)), &ITEM),
            ItemState::NeedsConversion
        );
        // Wrong size — can't be the target.
        assert_eq!(
            identify(Some((10, 0xCBF4_3926)), &ITEM),
            ItemState::NeedsConversion
        );
    }

    #[test]
    fn classify_recognizes_the_clean_target() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"123456789").unwrap();
        assert_eq!(classify(&root, &ITEM).unwrap(), ItemState::AlreadyTarget);
    }

    #[test]
    fn classify_flags_a_file_that_differs_by_size_without_hashing() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"a different length").unwrap();
        assert_eq!(classify(&root, &ITEM).unwrap(), ItemState::NeedsConversion);
    }

    #[test]
    fn classify_flags_a_same_size_but_different_file() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"987654321").unwrap(); // 9 bytes, different crc
        assert_eq!(classify(&root, &ITEM).unwrap(), ItemState::NeedsConversion);
    }

    #[test]
    fn classify_reports_a_missing_file() {
        let (_tmp, root) = temp();
        assert_eq!(classify(&root, &ITEM).unwrap(), ItemState::Missing);
    }

    #[test]
    fn plan_over_a_bare_dir_is_all_missing() {
        let (_tmp, root) = temp();
        let states: Vec<_> = plan(&root, Direction::ToOldGen)
            .unwrap()
            .into_iter()
            .map(|(_, state)| state)
            .collect();
        assert_eq!(states, vec![ItemState::Missing; OLD_GEN_ITEMS.len()]);
    }

    #[test]
    fn the_old_gen_manifest_is_the_three_core_binaries() {
        let names: Vec<_> = OLD_GEN_ITEMS.iter().map(|i| i.rel_path).collect();
        assert_eq!(
            names,
            ["Fallout4.exe", "Fallout4Launcher.exe", "steam_api64.dll"]
        );
    }
}
