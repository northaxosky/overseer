//! NTFS hard-link deployment backend.

use crate::deploy::deployer::LaunchTarget;

use super::error::{io_err, walk_io_err};
use super::{
    DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, ProgressEvent, ProgressSink,
    ReversalReport, VerifyReport,
};
use camino::{Utf8Path, Utf8PathBuf};
use same_file::Handle;
use std::fs;
use walkdir::WalkDir;

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
            if let Some(parent) = dest.parent() {
                crate::fs::ensure_dir(parent)?;
            }

            // Back up pre existing real files
            if dest.symlink_metadata().is_ok() {
                let backup = record.backup_root.join(&entry.relative);
                if let Some(parent) = backup.parent() {
                    crate::fs::ensure_dir(parent)?;
                }
                fs::rename(&dest, &backup).map_err(|e| io_err(&dest, e))?;
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

    fn undeploy(&self, record: &DeployRecord, progress: &dyn ProgressSink) -> ReversalReport {
        let mut unresolved = Vec::new();

        progress.on_event(ProgressEvent::Started {
            total: record.entries.len(),
        });

        for (index, entry) in record.entries.iter().enumerate() {
            let dest = record.target_root.join(&entry.relative);
            let backup = record.backup_root.join(&entry.relative);

            // The backup is authoritative
            match backup.symlink_metadata() {
                Ok(_) => {
                    if let Err(e) = remove_if_present(&dest) {
                        unresolved.push(e);
                    } else if let Err(e) = fs::rename(&backup, &dest) {
                        unresolved.push(io_err(&backup, e).into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // no backup for this entry: only remove dest if its our link
                    if is_our_link(&dest, &entry.source)
                        && let Err(e) = remove_if_present(&dest)
                    {
                        unresolved.push(e);
                    }
                }
                Err(e) => {
                    unresolved.push(io_err(&backup, e).into());
                }
            }

            progress.on_event(ProgressEvent::Removed {
                index,
                relative: entry.relative.as_path(),
            });
        }

        // Remove directories we created
        for relative in record.created_dirs.iter().rev() {
            let dir = record.target_root.join(relative);
            // Best-effort: the dir may be non-empty (foreign files) or already gone
            let _ = fs::remove_dir(&dir);
        }

        // Sweep backup root
        sweep_backup_root(&record.backup_root, &mut unresolved);

        progress.on_event(ProgressEvent::Finished);
        ReversalReport { unresolved }
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

    fn launch(&self, target: &LaunchTarget) -> Result<(), DeployError> {
        std::process::Command::new(target.program.as_std_path())
            .current_dir(target.working_dir.as_std_path())
            .args(&target.args)
            .spawn()
            .map_err(|source| DeployError::Launch {
                program: target.program.clone(),
                source,
            })?;
        Ok(())
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

/// Whether `dest` is still the hard link this deployment created
fn is_our_link(dest: &Utf8Path, source: &Utf8Path) -> bool {
    match (
        Handle::from_path(dest.as_std_path()),
        Handle::from_path(source.as_std_path()),
    ) {
        (Ok(d), Ok(s)) => d == s,
        _ => false,
    }
}

/// Sweep the backup root after a reversal
fn sweep_backup_root(backup_root: &Utf8Path, unresolved: &mut Vec<DeployError>) {
    if !backup_root.exists() {
        return;
    }

    let mut dirs: Vec<Utf8PathBuf> = Vec::new();
    for entry in WalkDir::new(backup_root).contents_first(true) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                let path = e
                    .path()
                    .and_then(|p| Utf8Path::from_path(p))
                    .unwrap_or(backup_root)
                    .to_owned();
                unresolved.push(walk_io_err(&path, e).into());
                continue;
            }
        };

        let Some(path) = Utf8Path::from_path(entry.path()) else {
            continue;
        };
        if entry.file_type().is_dir() {
            dirs.push(path.to_owned());
        } else {
            unresolved.push(DeployError::ResidualBackup {
                path: path.to_owned(),
            });
        }
    }

    // contents_first yields children before parents
    for dir in dirs {
        // Best-effort: the dir may be non-empty (foreign files) or already gone
        let _ = fs::remove_dir(&dir);
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
