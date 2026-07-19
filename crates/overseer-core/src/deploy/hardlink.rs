//! NTFS hard-link deployment backend.

use crate::deploy::deployer::LaunchTarget;

use super::error::io_err;
use super::{
    DeployEntry, DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, PreservedConflict,
    ProgressEvent, ProgressSink, ReversalIssue, ReversalReport, TargetOwnership, VerifyReport,
};
use camino::{Utf8Path, Utf8PathBuf};
use same_file::Handle;
use std::fs;

/// Deploys mods by creating NTFS hard links from each staged file into the game's target directory
#[derive(Debug, Default, Clone)]
pub struct HardlinkDeployer;

impl HardlinkDeployer {
    pub fn new() -> Self {
        Self
    }
}

impl Deployer for HardlinkDeployer {
    fn kind(&self) -> DeployerKind {
        DeployerKind::HardLink
    }

    fn check_supported(&self, plan: &DeployPlan) -> Result<(), DeployError> {
        for file in plan.files() {
            if !same_volume(&plan.target_root, &file.source) {
                return Err(DeployError::CrossVolume {
                    source_path: file.source.clone(),
                    target: plan.target_root.clone(),
                });
            }
        }
        Ok(())
    }

    fn deploy(
        &self,
        record: &DeployRecord,
        progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        crate::fs::ensure_dir(&record.target_root)?;
        progress.on_event(ProgressEvent::Started {
            total: record.entries.len(),
        });

        for (index, entry) in record.entries.iter().enumerate() {
            let dest = record.target_root.join(&entry.relative);
            validate_source(&entry.source)?;
            if let Some(parent) = dest.parent() {
                crate::fs::ensure_dir(parent)?;
            }

            match dest.symlink_metadata() {
                Ok(metadata) => {
                    if !crate::fs::is_regular_file(&metadata) {
                        return Err(DeployError::UnsafeFileType { path: dest });
                    }
                    let backup = record.backup_root.join(&entry.relative);
                    if let Some(parent) = backup.parent() {
                        crate::fs::ensure_dir(parent)?;
                    }
                    fs::rename(&dest, &backup).map_err(|e| io_err(&dest, e))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_err(&dest, error).into()),
            }

            fs::hard_link(&entry.source, &dest).map_err(|e| io_err(&dest, e))?;
            progress.on_event(ProgressEvent::Deployed {
                index,
                relative: entry.relative.as_path(),
            });
        }

        progress.on_event(ProgressEvent::Finished);
        Ok(())
    }

    fn classify(&self, record: &DeployRecord, entry: &DeployEntry) -> TargetOwnership {
        classify_hardlink(record, entry)
    }

    fn undeploy(&self, record: &DeployRecord, progress: &dyn ProgressSink) -> ReversalReport {
        let mut report = ReversalReport::default();

        progress.on_event(ProgressEvent::Started {
            total: record.entries.len(),
        });

        for (index, entry) in record.entries.iter().enumerate() {
            let dest = record.target_root.join(&entry.relative);
            let backup = record.backup_root.join(&entry.relative);
            let ownership = self.classify(record, entry);

            let backup_present = match regular_path_state(&backup) {
                Ok(present) => present,
                Err(issue) => {
                    report.unresolved.push(issue);
                    progress.on_event(ProgressEvent::Removed {
                        index,
                        relative: entry.relative.as_path(),
                    });
                    continue;
                }
            };

            match (ownership, backup_present) {
                (TargetOwnership::OwnedLink, true) => {
                    if let Err(error) = remove_if_present(&dest) {
                        report
                            .unresolved
                            .push(ReversalIssue::new(&dest, error.to_string()));
                    } else {
                        report.removed.push(entry.relative.clone());
                        match fs::rename(&backup, &dest) {
                            Ok(()) => report.restored.push(entry.relative.clone()),
                            Err(error) => report
                                .unresolved
                                .push(ReversalIssue::new(&backup, error.to_string())),
                        }
                    }
                }
                (TargetOwnership::OwnedLink, false) => match remove_if_present(&dest) {
                    Ok(()) => report.removed.push(entry.relative.clone()),
                    Err(error) => report
                        .unresolved
                        .push(ReversalIssue::new(&dest, error.to_string())),
                },
                (TargetOwnership::Foreign, backup_present) => {
                    report.preserved_conflicts.push(PreservedConflict {
                        path: entry.relative.clone(),
                        preserved_at: dest,
                        reason: if backup_present {
                            "foreign destination blocks original backup restoration".to_owned()
                        } else {
                            "foreign destination was preserved".to_owned()
                        },
                        blocking: backup_present,
                    });
                }
                (TargetOwnership::Absent, true) => match fs::rename(&backup, &dest) {
                    Ok(()) => report.restored.push(entry.relative.clone()),
                    Err(error) => report
                        .unresolved
                        .push(ReversalIssue::new(&backup, error.to_string())),
                },
                (TargetOwnership::Absent, false) => {}
                (TargetOwnership::Unknown(error), _) => {
                    report
                        .unresolved
                        .push(ReversalIssue::new(&dest, error.to_string()));
                }
            }

            progress.on_event(ProgressEvent::Removed {
                index,
                relative: entry.relative.as_path(),
            });
        }

        for relative in record.created_dirs.iter().rev() {
            let dir = record.target_root.join(relative);
            match dir.symlink_metadata() {
                Ok(metadata) if crate::fs::is_directory(&metadata) => {
                    if let Err(error) = fs::remove_dir(&dir)
                        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
                    {
                        report
                            .unresolved
                            .push(ReversalIssue::new(&dir, error.to_string()));
                    }
                }
                Ok(_) => report.preserved_conflicts.push(PreservedConflict {
                    path: relative.clone(),
                    preserved_at: dir,
                    reason: "created directory path is now non-regular".to_owned(),
                    blocking: true,
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => report
                    .unresolved
                    .push(ReversalIssue::new(&dir, error.to_string())),
            }
        }
        sweep_backup_root(&record.backup_root, &mut report.unresolved);

        progress.on_event(ProgressEvent::Finished);
        report
    }

    fn verify(&self, record: &DeployRecord) -> VerifyReport {
        let missing = record
            .entries
            .iter()
            .filter(|entry| !record.target_root.join(&entry.relative).exists())
            .map(|entry| entry.relative.clone())
            .collect();

        VerifyReport {
            expected: record.entries.len(),
            missing,
        }
    }

    fn launch(&self, target: &LaunchTarget) -> Result<Box<dyn super::LaunchHandle>, DeployError> {
        super::process::spawn(target)
    }
}

/// Classify one hardlink destination without following reparse points
fn classify_hardlink(record: &DeployRecord, entry: &DeployEntry) -> TargetOwnership {
    let dest = record.target_root.join(&entry.relative);
    let dest_metadata = match dest.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return TargetOwnership::Absent;
        }
        Err(error) => return TargetOwnership::Unknown(io_err(&dest, error).into()),
    };
    if !crate::fs::is_regular_file(&dest_metadata) {
        return TargetOwnership::Foreign;
    }

    let source_metadata = match entry.source.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) => return TargetOwnership::Unknown(io_err(&entry.source, error).into()),
    };
    if !crate::fs::is_regular_file(&source_metadata) {
        return TargetOwnership::Unknown(DeployError::UnsafeFileType {
            path: entry.source.clone(),
        });
    }

    let dest_handle = match Handle::from_path(dest.as_std_path()) {
        Ok(handle) => handle,
        Err(error) => return TargetOwnership::Unknown(io_err(&dest, error).into()),
    };
    let source_handle = match Handle::from_path(entry.source.as_std_path()) {
        Ok(handle) => handle,
        Err(error) => return TargetOwnership::Unknown(io_err(&entry.source, error).into()),
    };

    if dest_handle == source_handle {
        TargetOwnership::OwnedLink
    } else {
        TargetOwnership::Foreign
    }
}

/// Whether two paths live on the same volume
fn same_volume(a: &Utf8Path, b: &Utf8Path) -> bool {
    match (volume_id(a), volume_id(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => true,
    }
}

/// Remove a file if it is there
fn remove_if_present(path: &Utf8Path) -> Result<(), DeployError> {
    crate::fs::remove_file_opt(path).map_err(Into::into)
}

/// Require a staging source to be a normal file before linking through it
fn validate_source(source: &Utf8Path) -> Result<(), DeployError> {
    let metadata = source
        .symlink_metadata()
        .map_err(|error| io_err(source, error))?;
    if !crate::fs::is_regular_file(&metadata) {
        return Err(DeployError::UnsafeFileType {
            path: source.to_owned(),
        });
    }
    Ok(())
}

/// Probe whether a backup is a movable regular file
fn regular_path_state(path: &Utf8Path) -> Result<bool, ReversalIssue> {
    match path.symlink_metadata() {
        Ok(metadata) if crate::fs::is_regular_file(&metadata) => Ok(true),
        Ok(_) => Err(ReversalIssue::new(
            path,
            "backup path is non-regular and was preserved",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ReversalIssue::new(path, error.to_string())),
    }
}

/// Sweep empty backup directories and report every remaining artifact
fn sweep_backup_root(backup_root: &Utf8Path, unresolved: &mut Vec<ReversalIssue>) {
    match backup_root.symlink_metadata() {
        Ok(metadata) if crate::fs::is_directory(&metadata) => {}
        Ok(_) => {
            unresolved.push(ReversalIssue::new(
                backup_root,
                "backup root is non-regular and was preserved",
            ));
            return;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            unresolved.push(ReversalIssue::new(backup_root, error.to_string()));
            return;
        }
    }

    sweep_backup_dir(backup_root, unresolved);
}

/// Recursively sweep one normal backup directory without entering reparse points
fn sweep_backup_dir(dir: &Utf8Path, unresolved: &mut Vec<ReversalIssue>) {
    match dir.symlink_metadata() {
        Ok(metadata) if crate::fs::is_directory(&metadata) => {}
        Ok(_) => {
            unresolved.push(ReversalIssue::new(
                dir,
                "backup directory became non-regular and was preserved",
            ));
            return;
        }
        Err(error) => {
            unresolved.push(ReversalIssue::new(dir, error.to_string()));
            return;
        }
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            unresolved.push(ReversalIssue::new(dir, error.to_string()));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                unresolved.push(ReversalIssue::new(dir, error.to_string()));
                continue;
            }
        };
        let path = match Utf8PathBuf::from_path_buf(entry.path()) {
            Ok(path) => path,
            Err(path) => {
                unresolved.push(ReversalIssue::new(
                    dir,
                    format!("path is not valid UTF-8: `{}`", path.display()),
                ));
                continue;
            }
        };
        match path.symlink_metadata() {
            Ok(metadata) if crate::fs::is_directory(&metadata) => {
                sweep_backup_dir(&path, unresolved);
            }
            Ok(_) => unresolved.push(ReversalIssue::new(
                &path,
                "a backed-up file remains unresolved",
            )),
            Err(error) => unresolved.push(ReversalIssue::new(&path, error.to_string())),
        }
    }

    if let Err(error) = fs::remove_dir(dir)
        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
    {
        unresolved.push(ReversalIssue::new(dir, error.to_string()));
    }
}

fn volume_id(path: &Utf8Path) -> Option<String> {
    use std::path::{Component, Prefix};

    for component in path.as_std_path().components() {
        if let Component::Prefix(prefix) = component {
            return Some(match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    (letter as char).to_ascii_lowercase().to_string()
                }
                other => format!("{other:?}"),
            });
        }
    }
    None
}

#[cfg(test)]
#[path = "tests/hardlink.rs"]
mod tests;
