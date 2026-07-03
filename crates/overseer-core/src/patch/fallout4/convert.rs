//! Fallout 4 whole-install edition conversion through verified binary deltas.

use super::fingerprint::{
    BinaryFingerprint, CORE_BINARIES, fingerprints_for, known_source, target_fingerprint,
    target_table_complete,
};
use crate::detect::Generation;
use crate::error::{IoError, io_err};
use crate::fs::{copy, fsync, remove_file_opt, rename};
use crate::patch::delta::{DeltaDecoder, DeltaError};
use crate::patch::fingerprint::{FileFingerprint, VerifiedBy, fingerprint_file};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs::OpenOptions;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Copy)]
pub struct ConvertItem {
    pub rel_path: &'static str,
    pub target: &'static BinaryFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    AlreadyTarget,
    NeedsConversion,
    Missing,
}

#[derive(Debug, Clone)]
pub struct ItemPlan {
    pub item: ConvertItem,
    pub state: ItemState,
    pub current: Option<FileFingerprint>,
    pub known_source: Option<&'static BinaryFingerprint>,
}

#[derive(Debug, Clone)]
pub struct ConvertJob {
    pub item: ConvertItem,
    pub delta: Utf8PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Converted,
    AlreadyTarget,
    Missing,
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error(
        "target {target} is incomplete; refusing conversion until all core binary fingerprints are known"
    )]
    IncompleteTarget { target: Generation },
    #[error("delta application failed for `{item}`")]
    Delta {
        item: String,
        #[source]
        source: DeltaError,
    },
    #[error(
        "`{item}` did not reconstruct {expected_label} (size {found_size}, crc {found_crc:08X}, sha256 {found_sha256})"
    )]
    TargetMismatch {
        item: String,
        expected_label: String,
        found_size: u64,
        found_crc: u32,
        found_sha256: String,
    },
    #[error("source `{item}` changed before commit; refusing to swap")]
    SourceChanged { item: String },
    #[error("backup `{backup}` already exists and does not match the current source")]
    BackupConflict { backup: Utf8PathBuf },
    #[error("commit failed for `{item}`; rollback was attempted")]
    CommitFailed {
        item: String,
        #[source]
        source: IoError,
    },
    #[error(transparent)]
    Io(#[from] IoError),
}

#[derive(Debug)]
enum Prepared {
    Ready {
        item: ConvertItem,
        tmp: Utf8PathBuf,
        source: FileFingerprint,
        verified_by: VerifiedBy,
    },
    AlreadyTarget {
        item: ConvertItem,
    },
    Missing {
        item: ConvertItem,
    },
}

pub fn items(target: Generation) -> Vec<ConvertItem> {
    fingerprints_for(target)
        .into_iter()
        .map(|target| ConvertItem {
            rel_path: target.rel_path,
            target,
        })
        .collect()
}

pub fn target_is_complete(target: Generation) -> bool {
    target_table_complete(target)
}

pub fn classify(game_dir: &Utf8Path, item: ConvertItem) -> Result<ItemPlan, IoError> {
    let path = game_dir.join(item.rel_path);
    let current = fingerprint_file(&path)?;
    let known = current
        .as_ref()
        .and_then(|fp| known_source(item.rel_path, fp));
    let state = match &current {
        None => ItemState::Missing,
        Some(fp) if item.target.matches_file(fp) => ItemState::AlreadyTarget,
        Some(_) => ItemState::NeedsConversion,
    };
    Ok(ItemPlan {
        item,
        state,
        current,
        known_source: known,
    })
}

pub fn plan(game_dir: &Utf8Path, target: Generation) -> Result<Vec<ItemPlan>, ConvertError> {
    if !target_table_complete(target) {
        return Err(ConvertError::IncompleteTarget { target });
    }
    items(target)
        .into_iter()
        .map(|item| Ok(classify(game_dir, item)?))
        .collect()
}

pub fn explicit_item(target: Generation, rel_path: &str) -> Option<ConvertItem> {
    target_fingerprint(target, rel_path).map(|target| ConvertItem {
        rel_path: target.rel_path,
        target,
    })
}

fn prepare(
    game_dir: &Utf8Path,
    job: &ConvertJob,
    decoder: &dyn DeltaDecoder,
) -> Result<Prepared, ConvertError> {
    let item = job.item;
    cleanup_leftover_tmp(game_dir, item)?;
    let plan = classify(game_dir, item)?;
    match plan.state {
        ItemState::AlreadyTarget => return Ok(Prepared::AlreadyTarget { item }),
        ItemState::Missing => return Ok(Prepared::Missing { item }),
        ItemState::NeedsConversion => {}
    }
    let source = plan
        .current
        .expect("needs-conversion has a source fingerprint");
    if let Some(source_label) = plan.known_source {
        warn!(
            binary = item.rel_path,
            source = %source_label.label(),
            target = %item.target.label(),
            "prechecked known Fallout 4 binary before applying delta"
        );
    } else {
        warn!(
            binary = item.rel_path,
            "source binary is unknown; target hash remains enforced"
        );
    }
    let real = game_dir.join(item.rel_path);
    let tmp = tmp_path(game_dir, item);
    if let Err(source_err) = decoder.apply(&real, &job.delta, &tmp) {
        let _ = remove_file_opt(&tmp);
        return Err(ConvertError::Delta {
            item: item.rel_path.to_owned(),
            source: source_err,
        });
    }
    let Some(found) = fingerprint_file(&tmp).inspect_err(|_| {
        let _ = remove_file_opt(&tmp);
    })?
    else {
        remove_file_opt(&tmp)?;
        return Err(ConvertError::TargetMismatch {
            item: item.rel_path.to_owned(),
            expected_label: item.target.label(),
            found_size: 0,
            found_crc: 0,
            found_sha256: "missing".to_owned(),
        });
    };
    let Some(verified_by) = item.target.verify_file(&found) else {
        remove_file_opt(&tmp)?;
        return Err(ConvertError::TargetMismatch {
            item: item.rel_path.to_owned(),
            expected_label: item.target.label(),
            found_size: found.size,
            found_crc: found.crc32,
            found_sha256: found.sha256,
        });
    };
    fsync(&tmp)?;
    Ok(Prepared::Ready {
        item,
        tmp,
        source,
        verified_by,
    })
}

fn preflight_writable(game_dir: &Utf8Path, ready: &[PreparedReady]) -> Result<(), ConvertError> {
    for ready in ready {
        let real = game_dir.join(ready.item.rel_path);
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&real)
            .map_err(|e| io_err(&real, e))?;
    }
    Ok(())
}

#[derive(Debug)]
struct PreparedReady {
    item: ConvertItem,
    tmp: Utf8PathBuf,
    source: FileFingerprint,
    verified_by: VerifiedBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupAction {
    Created,
    Reused,
}

trait RenameOp {
    fn rename(&mut self, from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError>;
}

struct FsRename;

impl RenameOp for FsRename {
    fn rename(&mut self, from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError> {
        rename(from, to)
    }
}

fn commit_batch(game_dir: &Utf8Path, ready: &[PreparedReady]) -> Result<(), ConvertError> {
    commit_batch_with_renamer(game_dir, ready, &mut FsRename)
}

fn commit_batch_with_renamer(
    game_dir: &Utf8Path,
    ready: &[PreparedReady],
    renamer: &mut dyn RenameOp,
) -> Result<(), ConvertError> {
    preflight_writable(game_dir, ready)?;
    let mut backups = Vec::new();
    for ready in ready {
        let action = ensure_backup(game_dir, ready)?;
        backups.push((backup_path(game_dir, ready.item), action));
    }
    let mut swapped: Vec<&PreparedReady> = Vec::new();
    for (idx, current) in ready.iter().enumerate() {
        debug_assert!(matches!(
            current.verified_by,
            VerifiedBy::Sha256 | VerifiedBy::Crc32
        ));
        if !source_still_matches(game_dir, current)? {
            rollback(game_dir, &swapped);
            cleanup_remaining_temps(&ready[idx..]);
            cleanup_created_backups(&backups);
            return Err(ConvertError::SourceChanged {
                item: current.item.rel_path.to_owned(),
            });
        }
        let real = game_dir.join(current.item.rel_path);
        match renamer.rename(&current.tmp, &real) {
            Ok(()) => swapped.push(current),
            Err(source) => {
                rollback(game_dir, &swapped);
                cleanup_remaining_temps(&ready[idx..]);
                cleanup_created_backups(&backups);
                return Err(ConvertError::CommitFailed {
                    item: current.item.rel_path.to_owned(),
                    source,
                });
            }
        }
    }
    Ok(())
}

fn cleanup_remaining_temps(ready: &[PreparedReady]) {
    for ready in ready {
        let _ = remove_file_opt(&ready.tmp);
    }
}

fn cleanup_created_backups(backups: &[(Utf8PathBuf, BackupAction)]) {
    for (path, action) in backups {
        if *action == BackupAction::Created {
            let _ = remove_file_opt(path);
        }
    }
}

fn ensure_backup(game_dir: &Utf8Path, ready: &PreparedReady) -> Result<BackupAction, ConvertError> {
    let real = game_dir.join(ready.item.rel_path);
    let bak = backup_path(game_dir, ready.item);
    if let Some(existing) = fingerprint_file(&bak)? {
        if existing.size == ready.source.size
            && existing.crc32 == ready.source.crc32
            && existing.sha256.eq_ignore_ascii_case(&ready.source.sha256)
        {
            return Ok(BackupAction::Reused);
        }
        return Err(ConvertError::BackupConflict { backup: bak });
    }
    copy(&real, &bak)?;
    fsync(&bak)?;
    Ok(BackupAction::Created)
}

fn source_still_matches(game_dir: &Utf8Path, ready: &PreparedReady) -> Result<bool, ConvertError> {
    let real = game_dir.join(ready.item.rel_path);
    let Some(current) = fingerprint_file(&real)? else {
        return Ok(false);
    };
    Ok(current.size == ready.source.size
        && current.crc32 == ready.source.crc32
        && current.sha256.eq_ignore_ascii_case(&ready.source.sha256))
}

fn rollback(game_dir: &Utf8Path, swapped: &[&PreparedReady]) {
    for ready in swapped.iter().rev() {
        let real = game_dir.join(ready.item.rel_path);
        let bak = backup_path(game_dir, ready.item);
        let _ = copy(&bak, &real);
    }
}

fn cleanup_leftover_tmp(game_dir: &Utf8Path, item: ConvertItem) -> Result<(), ConvertError> {
    remove_file_opt(&tmp_path(game_dir, item))?;
    Ok(())
}

fn tmp_path(game_dir: &Utf8Path, item: ConvertItem) -> Utf8PathBuf {
    game_dir.join(format!("{}.overseer-tmp", item.rel_path))
}

fn backup_path(game_dir: &Utf8Path, item: ConvertItem) -> Utf8PathBuf {
    game_dir.join(format!("{}.overseer-bak", item.rel_path))
}

pub fn convert_item(
    game_dir: &Utf8Path,
    item: ConvertItem,
    delta: &Utf8Path,
    decoder: &dyn DeltaDecoder,
) -> Result<Outcome, ConvertError> {
    let job = ConvertJob {
        item,
        delta: delta.to_owned(),
    };
    match prepare(game_dir, &job, decoder)? {
        Prepared::Ready {
            item,
            tmp,
            source,
            verified_by,
        } => {
            let ready = [PreparedReady {
                item,
                tmp,
                source,
                verified_by,
            }];
            commit_batch(game_dir, &ready)?;
            Ok(Outcome::Converted)
        }
        Prepared::AlreadyTarget { .. } => Ok(Outcome::AlreadyTarget),
        Prepared::Missing { .. } => Ok(Outcome::Missing),
    }
}

pub fn convert(
    game_dir: &Utf8Path,
    jobs: &[ConvertJob],
    decoder: &dyn DeltaDecoder,
) -> Result<Vec<(String, Outcome)>, ConvertError> {
    let mut ready = Vec::new();
    let mut outcomes = Vec::new();
    for job in jobs {
        match prepare(game_dir, job, decoder) {
            Ok(Prepared::Ready {
                item,
                tmp,
                source,
                verified_by,
            }) => {
                ready.push(PreparedReady {
                    item,
                    tmp,
                    source,
                    verified_by,
                });
                outcomes.push((item.rel_path.to_owned(), Outcome::Converted));
            }
            Ok(Prepared::AlreadyTarget { item }) => {
                outcomes.push((item.rel_path.to_owned(), Outcome::AlreadyTarget));
            }
            Ok(Prepared::Missing { item }) => {
                outcomes.push((item.rel_path.to_owned(), Outcome::Missing));
            }
            Err(e) => {
                for ready in &ready {
                    let _ = remove_file_opt(&ready.tmp);
                }
                return Err(e);
            }
        }
    }
    commit_batch(game_dir, &ready)?;
    Ok(outcomes)
}

pub fn core_binary_names() -> &'static [&'static str] {
    CORE_BINARIES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::delta::DeltaError;
    use crate::patch::fingerprint::ExpectedFingerprint;
    use crate::test_support::temp;

    static TEST_FP: BinaryFingerprint = BinaryFingerprint {
        generation: Generation::OldGen,
        rel_path: "check.bin",
        build: "test",
        expected: ExpectedFingerprint {
            size: 9,
            crc32: 0xCBF4_3926,
            sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
        },
    };

    fn item() -> ConvertItem {
        ConvertItem {
            rel_path: "check.bin",
            target: &TEST_FP,
        }
    }

    enum FakeDecoder {
        Writes(Vec<u8>),
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

    fn seed_source(bytes: &[u8]) -> (tempfile::TempDir, Utf8PathBuf) {
        let (tmp, root) = temp();
        std::fs::write(root.join("check.bin"), bytes).unwrap();
        (tmp, root)
    }

    #[test]
    fn classify_recognizes_the_clean_target() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"123456789").unwrap();
        assert_eq!(
            classify(&root, item()).unwrap().state,
            ItemState::AlreadyTarget
        );
    }

    #[test]
    fn classify_flags_different_file() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"987654321").unwrap();
        assert_eq!(
            classify(&root, item()).unwrap().state,
            ItemState::NeedsConversion
        );
    }

    #[test]
    fn classify_reports_a_missing_file() {
        let (_tmp, root) = temp();
        assert_eq!(classify(&root, item()).unwrap().state, ItemState::Missing);
    }

    #[test]
    fn plan_refuses_incomplete_ng_target() {
        let (_tmp, root) = temp();
        assert!(matches!(
            plan(&root, Generation::NextGen),
            Err(ConvertError::IncompleteTarget { .. })
        ));
    }

    #[test]
    fn converts_verifies_and_backs_up() {
        let (_tmp, root) = seed_source(b"AE-version");
        let outcome = convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"123456789".to_vec()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Converted);
        assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"123456789");
        assert_eq!(
            std::fs::read(root.join("check.bin.overseer-bak")).unwrap(),
            b"AE-version"
        );
    }

    #[test]
    fn wrong_target_hash_leaves_real_file_untouched() {
        let (_tmp, root) = seed_source(b"AE-version");
        let err = convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"987654321".to_vec()),
        )
        .expect_err("mismatch");
        assert!(matches!(err, ConvertError::TargetMismatch { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version"
        );
        assert!(!root.join("check.bin.overseer-tmp").exists());
        assert!(!root.join("check.bin.overseer-bak").exists());
    }

    #[test]
    fn corrupted_delta_cleans_up() {
        let (_tmp, root) = seed_source(b"AE-version");
        let err = convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Fails,
        )
        .expect_err("decoder failed");
        assert!(matches!(err, ConvertError::Delta { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version"
        );
        assert!(!root.join("check.bin.overseer-tmp").exists());
    }

    #[test]
    fn already_target_is_idempotent() {
        let (_tmp, root) = temp();
        std::fs::write(root.join("check.bin"), b"123456789").unwrap();
        let outcome = convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"corrupted".to_vec()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::AlreadyTarget);
        assert!(!root.join("check.bin.overseer-bak").exists());
    }

    #[test]
    fn backup_conflict_refuses_to_clobber() {
        let (_tmp, root) = seed_source(b"AE-version");
        std::fs::write(root.join("check.bin.overseer-bak"), b"other").unwrap();
        let err = convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"123456789".to_vec()),
        )
        .expect_err("backup conflict");
        assert!(matches!(err, ConvertError::BackupConflict { .. }));
        assert_eq!(
            std::fs::read(root.join("check.bin")).unwrap(),
            b"AE-version"
        );
    }

    struct PerPathDecoder;

    impl DeltaDecoder for PerPathDecoder {
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
    fn bad_delta_in_batch_converts_nothing() {
        static GOOD_FP: BinaryFingerprint = BinaryFingerprint {
            generation: Generation::OldGen,
            rel_path: "good.bin",
            build: "test",
            expected: ExpectedFingerprint {
                size: 9,
                crc32: 0xCBF4_3926,
                sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
            },
        };
        static BAD_FP: BinaryFingerprint = BinaryFingerprint {
            generation: Generation::OldGen,
            rel_path: "bad.bin",
            build: "test",
            expected: ExpectedFingerprint {
                size: 9,
                crc32: 0xCBF4_3926,
                sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
            },
        };
        let (_tmp, root) = temp();
        std::fs::write(root.join("good.bin"), b"OLD-good").unwrap();
        std::fs::write(root.join("bad.bin"), b"OLD-bad").unwrap();
        let jobs = [
            ConvertJob {
                item: ConvertItem {
                    rel_path: "good.bin",
                    target: &GOOD_FP,
                },
                delta: Utf8PathBuf::from("d.vcdiff"),
            },
            ConvertJob {
                item: ConvertItem {
                    rel_path: "bad.bin",
                    target: &BAD_FP,
                },
                delta: Utf8PathBuf::from("d.vcdiff"),
            },
        ];
        let err = convert(&root, &jobs, &PerPathDecoder).expect_err("batch fails");
        assert!(matches!(err, ConvertError::TargetMismatch { .. }));
        assert_eq!(std::fs::read(root.join("good.bin")).unwrap(), b"OLD-good");
        assert_eq!(std::fs::read(root.join("bad.bin")).unwrap(), b"OLD-bad");
    }

    struct FailSecondRename {
        calls: usize,
    }

    impl RenameOp for FailSecondRename {
        fn rename(&mut self, from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError> {
            self.calls += 1;
            if self.calls == 2 {
                return Err(IoError::new(
                    to,
                    std::io::Error::other("injected rename failure"),
                ));
            }
            rename(from, to)
        }
    }

    #[test]
    fn commit_failure_on_second_rename_rolls_back_first_swap() {
        static ONE_FP: BinaryFingerprint = BinaryFingerprint {
            generation: Generation::OldGen,
            rel_path: "one.bin",
            build: "test",
            expected: ExpectedFingerprint {
                size: 9,
                crc32: 0xCBF4_3926,
                sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
            },
        };
        static TWO_FP: BinaryFingerprint = BinaryFingerprint {
            generation: Generation::OldGen,
            rel_path: "two.bin",
            build: "test",
            expected: ExpectedFingerprint {
                size: 9,
                crc32: 0xCBF4_3926,
                sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
            },
        };
        static THREE_FP: BinaryFingerprint = BinaryFingerprint {
            generation: Generation::OldGen,
            rel_path: "three.bin",
            build: "test",
            expected: ExpectedFingerprint {
                size: 9,
                crc32: 0xCBF4_3926,
                sha256: Some("15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225"),
            },
        };
        let (_tmp, root) = temp();
        std::fs::write(root.join("one.bin"), b"original1").unwrap();
        std::fs::write(root.join("two.bin"), b"original2").unwrap();
        std::fs::write(root.join("three.bin"), b"original3").unwrap();
        let jobs = [
            ConvertJob {
                item: ConvertItem {
                    rel_path: "one.bin",
                    target: &ONE_FP,
                },
                delta: Utf8PathBuf::from("d.vcdiff"),
            },
            ConvertJob {
                item: ConvertItem {
                    rel_path: "two.bin",
                    target: &TWO_FP,
                },
                delta: Utf8PathBuf::from("d.vcdiff"),
            },
            ConvertJob {
                item: ConvertItem {
                    rel_path: "three.bin",
                    target: &THREE_FP,
                },
                delta: Utf8PathBuf::from("d.vcdiff"),
            },
        ];
        let mut ready = Vec::new();
        for job in &jobs {
            match prepare(&root, job, &FakeDecoder::Writes(b"123456789".to_vec())).unwrap() {
                Prepared::Ready {
                    item,
                    tmp,
                    source,
                    verified_by,
                } => ready.push(PreparedReady {
                    item,
                    tmp,
                    source,
                    verified_by,
                }),
                _ => panic!("all three jobs should prepare"),
            }
        }
        let mut renamer = FailSecondRename { calls: 0 };
        let err = commit_batch_with_renamer(&root, &ready, &mut renamer).expect_err("commit fails");
        assert!(matches!(err, ConvertError::CommitFailed { item, .. } if item == "two.bin"));
        assert_eq!(std::fs::read(root.join("one.bin")).unwrap(), b"original1");
        assert_eq!(std::fs::read(root.join("two.bin")).unwrap(), b"original2");
        assert_eq!(std::fs::read(root.join("three.bin")).unwrap(), b"original3");
        for name in ["one.bin", "two.bin", "three.bin"] {
            assert!(!root.join(format!("{name}.overseer-tmp")).exists());
            assert!(!root.join(format!("{name}.overseer-bak")).exists());
        }
    }

    #[test]
    fn leftover_temp_is_removed_before_retry() {
        let (_tmp, root) = seed_source(b"AE-version");
        std::fs::write(root.join("check.bin.overseer-tmp"), b"stale").unwrap();
        convert_item(
            &root,
            item(),
            Utf8Path::new("d.vcdiff"),
            &FakeDecoder::Writes(b"123456789".to_vec()),
        )
        .unwrap();
        assert_eq!(std::fs::read(root.join("check.bin")).unwrap(), b"123456789");
    }
}
