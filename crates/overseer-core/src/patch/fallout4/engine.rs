//! The shared, crash-safe conversion engine: apply a verified delta and swap it in atomically.
//!
//! The engine is edition-agnostic and label-free. It knows only a file's path and the expected
//! identity it must reconstruct; every policy (core edition flip, DLC consistency revision) wires
//! its own targets, groups and display concerns through [`Policy`].

use crate::error::{IoError, io_err};
use crate::fs::{fsync, remove_file_opt, rename, size_opt};
use crate::patch::delta::{DeltaDecoder, DeltaError};
use crate::patch::fingerprint::{
    ExpectedFingerprint, FileFingerprint, VerifiedBy, fingerprint_file,
};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs::OpenOptions;
use thiserror::Error;
use tracing::warn;

/// The known-good identity a single file must reconstruct, plus where it lives
#[derive(Debug, Clone, Copy)]
pub struct TargetSpec {
    pub rel_path: &'static str,
    pub expected: ExpectedFingerprint,
}

/// How a policy decides whether a group is present and should be converted
pub enum Ownership {
    /// Always converted; every file is required
    Mandatory,
    /// Converted only when this file (a group's master) exists on disk
    Sentinel(&'static str),
}

/// A set of files that must convert together to stay internally consistent
pub struct GroupSpec {
    pub name: &'static str,
    pub ownership: Ownership,
    pub files: &'static [&'static str],
}

impl GroupSpec {
    /// Whether the group is present on disk (mandatory, or its sentinel exists)
    pub fn is_owned(&self, game_dir: &Utf8Path) -> Result<bool, IoError> {
        match self.ownership {
            Ownership::Mandatory => Ok(true),
            Ownership::Sentinel(rel) => Ok(size_opt(&game_dir.join(rel))?.is_some()),
        }
    }
}

/// One convertible file: its path, the identity it should reach, and the group it belongs to
#[derive(Debug, Clone, Copy)]
pub struct ConvertItem {
    pub rel_path: &'static str,
    pub target: TargetSpec,
    pub group: &'static str,
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
    pub known_source: Option<String>,
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
    #[error("delta application failed for `{item}`")]
    Delta {
        item: String,
        #[source]
        source: DeltaError,
    },
    #[error(
        "`{item}` did not reconstruct the expected target (size {found_size}, crc {found_crc:08X}, sha256 {found_sha256})"
    )]
    TargetMismatch {
        item: String,
        found_size: u64,
        found_crc: u32,
        found_sha256: String,
    },
    #[error("source `{item}` changed before commit; refusing to swap")]
    SourceChanged { item: String },
    #[error("backup `{backup}` is from an earlier conversion; revert or remove it first")]
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

/// Everything a policy supplies to the engine: its groups, its target lookup, and display hooks
pub struct Policy<'a> {
    pub groups: &'static [GroupSpec],
    pub target_for: &'a dyn Fn(&str) -> Option<TargetSpec>,
    pub any_known_size: &'a dyn Fn(&str, u64) -> bool,
    pub known_source: &'a dyn Fn(&str, &FileFingerprint) -> Option<String>,
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

/// Whether every file in `group` has a known target under `policy`
fn is_convertible(group: &GroupSpec, policy: &Policy<'_>) -> bool {
    group
        .files
        .iter()
        .all(|rel| (policy.target_for)(rel).is_some())
}

/// Append every fingerprinted convert item in `group` under `policy`
fn push_group_items(group: &GroupSpec, policy: &Policy<'_>, out: &mut Vec<ConvertItem>) {
    for &rel in group.files {
        if let Some(target) = (policy.target_for)(rel) {
            debug_assert_eq!(target.rel_path, rel);
            out.push(ConvertItem {
                rel_path: rel,
                target,
                group: group.name,
            });
        }
    }
}

/// Every convertible, owned item under `policy`
pub fn items(game_dir: &Utf8Path, policy: &Policy<'_>) -> Result<Vec<ConvertItem>, IoError> {
    let mut items = Vec::new();
    for group in policy.groups {
        if !is_convertible(group, policy) || !group.is_owned(game_dir)? {
            continue;
        }
        push_group_items(group, policy, &mut items);
    }
    Ok(items)
}

/// The current state of `item` on disk and its measured fingerprint; policy-free
fn classify_state(
    game_dir: &Utf8Path,
    item: ConvertItem,
) -> Result<(ItemState, Option<FileFingerprint>), IoError> {
    let path = game_dir.join(item.rel_path);
    let current = fingerprint_file(&path)?;
    let state = match &current {
        None => ItemState::Missing,
        Some(fp) if item.target.expected.matches(fp) => ItemState::AlreadyTarget,
        Some(_) => ItemState::NeedsConversion,
    };
    Ok((state, current))
}

/// Classify `item` and attach the policy's known source, if any
pub fn classify(
    game_dir: &Utf8Path,
    item: ConvertItem,
    policy: &Policy<'_>,
) -> Result<ItemPlan, IoError> {
    let (state, current) = classify_state(game_dir, item)?;
    let known_source = current
        .as_ref()
        .and_then(|fp| (policy.known_source)(item.rel_path, fp));
    Ok(ItemPlan {
        item,
        state,
        current,
        known_source,
    })
}

/// Classify for a preview, skipping the hash when size rules out every known fingerprint
fn classify_preview(
    game_dir: &Utf8Path,
    item: ConvertItem,
    policy: &Policy<'_>,
) -> Result<ItemPlan, IoError> {
    let path = game_dir.join(item.rel_path);
    let Some(size) = size_opt(&path)? else {
        return Ok(ItemPlan {
            item,
            state: ItemState::Missing,
            current: None,
            known_source: None,
        });
    };
    if size != item.target.expected.size && !(policy.any_known_size)(item.rel_path, size) {
        return Ok(ItemPlan {
            item,
            state: ItemState::NeedsConversion,
            current: None,
            known_source: None,
        });
    }
    classify(game_dir, item, policy)
}

/// Plan the conversion for every owned, convertible item under `policy`
pub fn plan(game_dir: &Utf8Path, policy: &Policy<'_>) -> Result<Vec<ItemPlan>, ConvertError> {
    let items = items(game_dir, policy)?;
    items
        .into_iter()
        .map(|item| Ok(classify_preview(game_dir, item, policy)?))
        .collect()
}

/// Restore any file left in a crashed mid-swap state, ignoring ownership (a crashed sentinel looks absent)
pub fn recover_install(game_dir: &Utf8Path, policy: &Policy<'_>) -> Result<(), ConvertError> {
    let mut items = Vec::new();
    for group in policy.groups {
        if is_convertible(group, policy) {
            push_group_items(group, policy, &mut items);
        }
    }
    for item in items {
        recover_leftover_backup(game_dir, item)?;
    }
    Ok(())
}

fn prepare(
    game_dir: &Utf8Path,
    job: &ConvertJob,
    decoder: &dyn DeltaDecoder,
) -> Result<Prepared, ConvertError> {
    let item = job.item;
    recover_leftover_backup(game_dir, item)?;
    cleanup_leftover_tmp(game_dir, item)?;
    let (state, current) = classify_state(game_dir, item)?;
    match state {
        ItemState::AlreadyTarget => return Ok(Prepared::AlreadyTarget { item }),
        ItemState::Missing => return Ok(Prepared::Missing { item }),
        ItemState::NeedsConversion => {}
    }
    let source = current.expect("needs-conversion has a source fingerprint");
    warn!(file = item.rel_path, "applying verified delta");
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
            found_size: 0,
            found_crc: 0,
            found_sha256: "missing".to_owned(),
        });
    };
    let Some(verified_by) = item.target.expected.verify(&found) else {
        remove_file_opt(&tmp)?;
        return Err(ConvertError::TargetMismatch {
            item: item.rel_path.to_owned(),
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
    if let Err(err) = preflight_writable(game_dir, ready) {
        cleanup_remaining_temps(ready);
        return Err(err);
    }
    for r in ready {
        if let Err(err) = guard_backup_slot(game_dir, r) {
            cleanup_remaining_temps(ready);
            return Err(err);
        }
    }
    let mut moved: Vec<&PreparedReady> = Vec::new();
    for (idx, current) in ready.iter().enumerate() {
        debug_assert!(matches!(
            current.verified_by,
            VerifiedBy::Sha256 | VerifiedBy::Crc32
        ));
        if !source_still_matches(game_dir, current)? {
            rollback(game_dir, &moved, renamer);
            cleanup_remaining_temps(&ready[idx..]);
            return Err(ConvertError::SourceChanged {
                item: current.item.rel_path.to_owned(),
            });
        }
        if let Err(err) = move_original_aside(game_dir, current, renamer) {
            rollback(game_dir, &moved, renamer);
            cleanup_remaining_temps(&ready[idx..]);
            return Err(err);
        }
        moved.push(current);
        let real = game_dir.join(current.item.rel_path);
        if let Err(source) = renamer.rename(&current.tmp, &real) {
            rollback(game_dir, &moved, renamer);
            cleanup_remaining_temps(&ready[idx..]);
            return Err(ConvertError::CommitFailed {
                item: current.item.rel_path.to_owned(),
                source,
            });
        }
    }
    Ok(())
}

fn cleanup_remaining_temps(ready: &[PreparedReady]) {
    for ready in ready {
        let _ = remove_file_opt(&ready.tmp);
    }
}

/// Refuse up front if the backup slot holds a non-source file; re-converting a converted file must revert first
fn guard_backup_slot(game_dir: &Utf8Path, ready: &PreparedReady) -> Result<(), ConvertError> {
    let bak = backup_path(game_dir, ready.item);
    if let Some(existing) = fingerprint_file(&bak)?
        && !source_matches(&existing, &ready.source)
    {
        return Err(ConvertError::BackupConflict { backup: bak });
    }
    Ok(())
}

/// Move the original aside via atomic rename; a matching pre-existing backup is left in place
fn move_original_aside(
    game_dir: &Utf8Path,
    ready: &PreparedReady,
    renamer: &mut dyn RenameOp,
) -> Result<(), ConvertError> {
    let bak = backup_path(game_dir, ready.item);
    if size_opt(&bak)?.is_some() {
        return Ok(());
    }
    let real = game_dir.join(ready.item.rel_path);
    renamer
        .rename(&real, &bak)
        .map_err(|source| ConvertError::CommitFailed {
            item: ready.item.rel_path.to_owned(),
            source,
        })
}

fn source_still_matches(game_dir: &Utf8Path, ready: &PreparedReady) -> Result<bool, ConvertError> {
    let real = game_dir.join(ready.item.rel_path);
    let Some(current) = fingerprint_file(&real)? else {
        return Ok(false);
    };
    Ok(source_matches(&current, &ready.source))
}

fn source_matches(file: &FileFingerprint, source: &FileFingerprint) -> bool {
    file.size == source.size
        && file.crc32 == source.crc32
        && file.sha256.eq_ignore_ascii_case(&source.sha256)
}

/// Restore every already-moved file from its backup via atomic rename; best-effort
fn rollback(game_dir: &Utf8Path, moved: &[&PreparedReady], renamer: &mut dyn RenameOp) {
    for ready in moved.iter().rev() {
        let real = game_dir.join(ready.item.rel_path);
        let bak = backup_path(game_dir, ready.item);
        let _ = renamer.rename(&bak, &real);
    }
}

/// Recover a crash mid-swap: if the real file is gone but its backup survives, restore it
fn recover_leftover_backup(game_dir: &Utf8Path, item: ConvertItem) -> Result<(), ConvertError> {
    let real = game_dir.join(item.rel_path);
    let bak = backup_path(game_dir, item);
    if size_opt(&real)?.is_none() && size_opt(&bak)?.is_some() {
        rename(&bak, &real)?;
    }
    Ok(())
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

#[cfg(test)]
#[path = "tests/engine.rs"]
mod tests;
