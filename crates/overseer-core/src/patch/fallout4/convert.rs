//! Fallout 4 whole install edition conversion: applying binary deltas to swap game binaries between gens

use crate::detect::Generation;
use crate::error::IoError;
use crate::fs::{copy, fsync, remove_file_opt, rename, size_opt};
use crate::patch::delta::{DeltaDecoder, DeltaError, crc32_file};
use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

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

/// The manifest of files to convert an install *to* `target` (only Old-Gen today; upgrades yield nothing).
pub fn items(target: Generation) -> &'static [ConvertItem] {
    match target {
        Generation::OldGen => OLD_GEN_ITEMS,
        Generation::NextGen | Generation::Anniversary => &[],
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

/// Classify every item in `target`'s manifest against `game_dir`
pub fn plan(
    game_dir: &Utf8Path,
    target: Generation,
) -> Result<Vec<(&'static ConvertItem, ItemState)>, IoError> {
    items(target)
        .iter()
        .map(|item| Ok((item, classify(game_dir, item)?)))
        .collect()
}

/// A failure that stops a conversion
#[derive(Debug, Error)]
pub enum ConvertError {
    /// The delta decoder failed on this file
    #[error("delta application failed for `{item}`")]
    Delta {
        item: String,
        #[source]
        source: DeltaError,
    },
    /// The reconstructed file was not the expected target
    #[error("`{item}` did not reconstruct the target (crc {found:08X}, expected {expected:08X})")]
    TargetMismatch {
        item: String,
        found: u32,
        expected: u32,
    },
    #[error(transparent)]
    Io(#[from] IoError),
}

/// What converting one item did
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The file was downgraded and verified against its target
    Converted,
    /// The file was already the target edition; left untouched
    AlreadyTarget,
    /// The file was absent; nothing to convert
    Missing,
}

/// Initial result for one item: a verified temp to swap, or a no op
enum Prepared {
    /// A verified temp file that must replace the item's live file
    Ready { tmp: Utf8PathBuf },
    /// Already the target edition; nothing to do
    AlreadyTarget,
    /// Absent from the game directory; nothing to convert
    Missing,
}

/// Reconstruct `item` to a temp and verify it against the target, without touching the live file
fn prepare(
    game_dir: &Utf8Path,
    item: &ConvertItem,
    delta: &Utf8Path,
    decoder: &dyn DeltaDecoder,
) -> Result<Prepared, ConvertError> {
    match classify(game_dir, item)? {
        ItemState::AlreadyTarget => return Ok(Prepared::AlreadyTarget),
        ItemState::Missing => return Ok(Prepared::Missing),
        ItemState::NeedsConversion => {}
    }

    let real = game_dir.join(item.rel_path);
    let tmp = game_dir.join(format!("{}.overseer-tmp", item.rel_path));

    // Reconstruct into a temp; the real file is still the untouched source
    if let Err(source) = decoder.apply(&real, delta, &tmp) {
        let _ = remove_file_opt(&tmp);
        return Err(ConvertError::Delta {
            item: item.rel_path.to_owned(),
            source,
        });
    }

    // Verify the candidate is the exact clean target: size and CRC
    let size = size_opt(&tmp).inspect_err(|_| {
        let _ = remove_file_opt(&tmp);
    })?;
    let found = crc32_file(&tmp).inspect_err(|_| {
        let _ = remove_file_opt(&tmp);
    })?;
    if size != Some(item.target_size) || found != item.target_crc {
        remove_file_opt(&tmp)?;
        return Err(ConvertError::TargetMismatch {
            item: item.rel_path.to_owned(),
            found,
            expected: item.target_crc,
        });
    }

    // Flush the reconstructed bytes
    fsync(&tmp)?;
    Ok(Prepared::Ready { tmp })
}

/// Swap a verified temp into place: back up the original by copying, then atomically replace it
fn commit(game_dir: &Utf8Path, item: &ConvertItem, tmp: &Utf8Path) -> Result<(), ConvertError> {
    let real = game_dir.join(item.rel_path);
    let bak = game_dir.join(format!("{}.overseer-bak", item.rel_path));
    copy(&real, &bak)?;
    rename(tmp, &real)?;
    Ok(())
}

/// Convert one `item` in `game_dir` by applying `delta`
pub fn convert_item(
    game_dir: &Utf8Path,
    item: &ConvertItem,
    delta: &Utf8Path,
    decoder: &dyn DeltaDecoder,
) -> Result<Outcome, ConvertError> {
    match prepare(game_dir, item, delta, decoder)? {
        Prepared::Ready { tmp } => {
            commit(game_dir, item, &tmp)?;
            Ok(Outcome::Converted)
        }
        Prepared::AlreadyTarget => Ok(Outcome::AlreadyTarget),
        Prepared::Missing => Ok(Outcome::Missing),
    }
}

/// Apply each resolved `(item, delta)` job as a batch
pub fn convert(
    game_dir: &Utf8Path,
    jobs: &[(&ConvertItem, &Utf8Path)],
    decoder: &dyn DeltaDecoder,
) -> Result<Vec<(&'static str, Outcome)>, ConvertError> {
    let mut ready: Vec<(&ConvertItem, Utf8PathBuf)> = Vec::new();
    let mut outcomes: Vec<(&'static str, Outcome)> = Vec::new();
    for (item, delta) in jobs {
        match prepare(game_dir, item, delta, decoder) {
            Ok(Prepared::Ready { tmp }) => {
                ready.push((item, tmp));
                outcomes.push((item.rel_path, Outcome::Converted));
            }
            Ok(Prepared::AlreadyTarget) => outcomes.push((item.rel_path, Outcome::AlreadyTarget)),
            Ok(Prepared::Missing) => outcomes.push((item.rel_path, Outcome::Missing)),
            Err(e) => {
                for (_, tmp) in &ready {
                    let _ = remove_file_opt(tmp);
                }
                return Err(e);
            }
        }
    }

    // Every required temp is verified; swap them in
    for (item, tmp) in &ready {
        commit(game_dir, item, tmp)?;
    }
    Ok(outcomes)
}

/// Downgrade the three core binaries to Old-Gen, pairing each with its delta; convenience over [`convert`]
pub fn convert_to_old_gen(
    game_dir: &Utf8Path,
    fallout4_exe: &Utf8Path,
    launcher: &Utf8Path,
    steam_api64: &Utf8Path,
    decoder: &dyn DeltaDecoder,
) -> Result<Vec<(&'static str, Outcome)>, ConvertError> {
    let jobs = [
        (&OLD_GEN_ITEMS[0], fallout4_exe),
        (&OLD_GEN_ITEMS[1], launcher),
        (&OLD_GEN_ITEMS[2], steam_api64),
    ];
    convert(game_dir, &jobs, decoder)
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
        let states: Vec<_> = plan(&root, Generation::OldGen)
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

    // A decoder that ignores the delta and writes a scripted result, so convert_item's
    // ordering/verification can be exercised without a real xdelta3.
    enum FakeDecoder {
        /// Write these exact bytes to `dest` (simulates a good or a wrong delta).
        Writes(Vec<u8>),
        /// Fail as if the decoder errored, writing nothing.
        Fails,
    }

    impl DeltaDecoder for FakeDecoder {
        fn apply(
            &self,
            _source: &Utf8Path,
            _delta: &Utf8Path,
            dest: &Utf8Path,
        ) -> Result<(), DeltaError> {
            match self {
                FakeDecoder::Writes(bytes) => {
                    std::fs::write(dest, bytes).unwrap();
                    Ok(())
                }
                FakeDecoder::Fails => Err(DeltaError::Failed {
                    code: Some(1),
                    stderr: "boom".to_owned(),
                }),
            }
        }
    }

    /// Seed `game_dir/check.bin` with `bytes` (a not-yet-target file).
    fn seed_source(bytes: &[u8]) -> (tempfile::TempDir, camino::Utf8PathBuf) {
        let (tmp, root) = temp();
        std::fs::write(root.join("check.bin"), bytes).unwrap();
        (tmp, root)
    }

    #[test]
    fn convert_item_downgrades_verifies_and_backs_up() {
        let (_tmp, root) = seed_source(b"AE-version");
        // A "good delta": the decoder produces the exact target bytes.
        let decoder = FakeDecoder::Writes(b"123456789".to_vec());

        let outcome = convert_item(&root, &ITEM, Utf8Path::new("d.vcdiff"), &decoder).unwrap();

        assert_eq!(outcome, Outcome::Converted);
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"123456789",
            "the real file is now the target"
        );
        assert_eq!(
            std::fs::read(root.join("check.bin.overseer-bak")).unwrap(),
            b"AE-version",
            "the original is backed up"
        );
        assert!(
            !root.join("check.bin.overseer-tmp").exists(),
            "the temp is consumed by the rename"
        );
    }

    #[test]
    fn a_wrong_delta_leaves_the_real_file_untouched() {
        let (_tmp, root) = seed_source(b"AE-version");
        // A "bad delta": right length (9) but wrong bytes, so the CRC won't match.
        let decoder = FakeDecoder::Writes(b"987654321".to_vec());

        let err =
            convert_item(&root, &ITEM, Utf8Path::new("d.vcdiff"), &decoder).expect_err("mismatch");

        assert!(matches!(err, ConvertError::TargetMismatch { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version",
            "a bad delta must never touch the real file"
        );
        assert!(
            !root.join("check.bin.overseer-tmp").exists(),
            "the rejected temp is cleaned up"
        );
        assert!(
            !root.join("check.bin.overseer-bak").exists(),
            "no backup is made when the conversion is rejected"
        );
    }

    #[test]
    fn a_decoder_failure_is_reported_and_cleans_up() {
        let (_tmp, root) = seed_source(b"AE-version");
        let err = convert_item(&root, &ITEM, Utf8Path::new("d.vcdiff"), &FakeDecoder::Fails)
            .expect_err("decoder failed");

        assert!(matches!(err, ConvertError::Delta { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version",
            "the real file is untouched"
        );
        assert!(!root.join("check.bin.overseer-tmp").exists());
    }

    #[test]
    fn convert_item_is_idempotent_on_an_already_target_file() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"123456789").unwrap(); // already the target
        // A decoder that would corrupt if ever called — it must not be.
        let decoder = FakeDecoder::Writes(b"corrupted".to_vec());

        let outcome = convert_item(&root, &ITEM, Utf8Path::new("d.vcdiff"), &decoder).unwrap();

        assert_eq!(outcome, Outcome::AlreadyTarget);
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"123456789",
            "an already-target file is left exactly as-is"
        );
        assert!(!root.join("check.bin.overseer-bak").exists());
    }

    #[test]
    fn convert_item_reports_a_missing_file() {
        let (_tmp, root) = temp();
        let outcome = convert_item(
            &root,
            &ITEM,
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"123456789".to_vec()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Missing);
    }

    #[test]
    fn a_right_crc_but_wrong_size_result_is_rejected() {
        // Guards the CRC32-collision path: the item's CRC matches the output, but the size does
        // not, so the two-factor check must still refuse and leave the real file untouched.
        let (_tmp, root) = seed_source(b"AE-version");
        let item = ConvertItem {
            rel_path: "check.bin",
            target_size: 9_999,      // deliberately not the 9-byte output size
            target_crc: 0xCBF4_3926, // the CRC of "123456789"
        };
        let decoder = FakeDecoder::Writes(b"123456789".to_vec());

        let err = convert_item(&root, &item, Utf8Path::new("d.vcdiff"), &decoder)
            .expect_err("size mismatch");

        assert!(matches!(err, ConvertError::TargetMismatch { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version",
            "a size mismatch must not touch the real file"
        );
        assert!(!root.join("check.bin.overseer-tmp").exists());
        assert!(!root.join("check.bin.overseer-bak").exists());
    }

    // A decoder that writes the target bytes for a "good" dest and garbage for anything else,
    // so a batch can mix a verifiable job with a failing one.
    struct PerDestDecoder;
    impl DeltaDecoder for PerDestDecoder {
        fn apply(
            &self,
            _source: &Utf8Path,
            _delta: &Utf8Path,
            dest: &Utf8Path,
        ) -> Result<(), DeltaError> {
            let bytes: &[u8] = if dest.as_str().contains("good") {
                b"123456789"
            } else {
                b"XXXXXXXXX"
            };
            std::fs::write(dest, bytes).unwrap();
            Ok(())
        }
    }

    #[test]
    fn a_bad_delta_in_the_batch_converts_nothing() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("good.bin"), b"OLD-good").unwrap();
        std::fs::write(root.join("bad.bin"), b"OLD-bad").unwrap();
        let good = ConvertItem {
            rel_path: "good.bin",
            target_size: 9,
            target_crc: 0xCBF4_3926,
        };
        let bad = ConvertItem {
            rel_path: "bad.bin",
            target_size: 9,
            target_crc: 0xCBF4_3926,
        };
        let d = Utf8Path::new("d.vcdiff");
        let jobs = [(&good, d), (&bad, d)];

        let err =
            convert(&root, &jobs, &PerDestDecoder).expect_err("the batch fails on the bad delta");
        assert!(matches!(err, ConvertError::TargetMismatch { .. }));
        // Two-phase: because one delta failed verification, no live file was swapped.
        assert_eq!(
            std::fs::read(root.join("good.bin")).unwrap(),
            b"OLD-good",
            "the good file is untouched"
        );
        assert_eq!(
            std::fs::read(root.join("bad.bin")).unwrap(),
            b"OLD-bad",
            "the bad file is untouched"
        );
        assert!(
            !root.join("good.bin.overseer-bak").exists(),
            "no backup is made when the batch is rejected"
        );
        assert!(
            !root.join("good.bin.overseer-tmp").exists(),
            "staged temps are cleaned up"
        );
        assert!(!root.join("bad.bin.overseer-tmp").exists());
    }

    #[test]
    fn convert_runs_each_job_and_reports_every_outcome() {
        let (_tmp, root) = seed_source(b"AE-version");
        let decoder = FakeDecoder::Writes(b"123456789".to_vec());
        let jobs = [(&ITEM, Utf8Path::new("d.vcdiff"))];

        let outcomes = convert(&root, &jobs, &decoder).unwrap();
        assert_eq!(outcomes, vec![("check.bin", Outcome::Converted)]);
    }
}
