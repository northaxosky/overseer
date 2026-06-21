//! NTFS hard-link deployment backend.

use super::error::io_err;
use super::{
    DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, ProgressEvent, ProgressSink,
    ReversalReport, VerifyReport,
};
use camino::{Utf8Path, Utf8PathBuf};
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
        fs::create_dir_all(&record.target_root).map_err(|e| io_err(&record.target_root, e))?;
        progress.on_event(ProgressEvent::Started {
            total: record.entries.len(),
        });

        for (index, entry) in record.entries.iter().enumerate() {
            let dest = record.target_root.join(&entry.relative);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
            }

            // Back up pre existing real files
            if dest.symlink_metadata().is_ok() {
                let backup = record.backup_root.join(&entry.relative);
                if let Some(parent) = backup.parent() {
                    fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
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
                        unresolved.push(io_err(&backup, e));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if entry.preexisting {
                        // Original already restored by an earlier pass
                    } else if let Err(e) = remove_if_present(&dest) {
                        unresolved.push(e);
                    }
                }
                Err(e) => {
                    unresolved.push(io_err(&backup, e));
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
            // Best-effort: the dir may be non-empty (foreign files) or already gone.
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
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
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
                let io = e
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("walk backup root"));
                unresolved.push(io_err(&path, io));
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

    // cntents_first yields children before parents
    for dir in dirs {
        // Best-effort: the dir may be non-empty (foreign files) or already gone.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::{ModSource, NullSink};
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn temp() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let base = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 temp path");
        (dir, base)
    }

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(path, contents).expect("write file");
    }

    /// A one-file plan: stage `rel` in a mod under `base`, targeting `base/Data`.
    fn plan_one(base: &Utf8Path, rel: &str, contents: &str) -> (DeployPlan, Utf8PathBuf) {
        let m = base.join("mods/M");
        write(&m.join(rel), contents);
        let data = base.join("Data");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("M", &m)]).expect("plan");
        (plan, data)
    }

    /// A one-file plan turned into a record, with a sibling backup root.
    fn record_one(base: &Utf8Path, rel: &str, contents: &str) -> (DeployRecord, Utf8PathBuf) {
        let (plan, data) = plan_one(base, rel, contents);
        let record =
            DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
                .expect("record");
        (record, data)
    }

    #[cfg(windows)]
    #[test]
    fn volume_id_extracts_lowercase_drive_letter() {
        assert_eq!(volume_id(Utf8Path::new(r"C:\a\b")).as_deref(), Some("c"));
        assert_eq!(volume_id(Utf8Path::new(r"D:\x")).as_deref(), Some("d"));
    }

    #[test]
    fn volume_id_is_none_for_relative_paths() {
        assert_eq!(volume_id(Utf8Path::new("a/b/c")), None);
    }

    #[cfg(windows)]
    #[test]
    fn same_volume_true_for_same_drive_regardless_of_case() {
        assert!(same_volume(Utf8Path::new(r"c:\a"), Utf8Path::new(r"C:\b")));
    }

    #[cfg(windows)]
    #[test]
    fn same_volume_false_across_drives() {
        assert!(!same_volume(Utf8Path::new(r"C:\a"), Utf8Path::new(r"D:\b")));
    }

    #[test]
    fn same_volume_true_when_undetermined() {
        assert!(same_volume(Utf8Path::new("a/b"), Utf8Path::new("c/d")));
    }

    #[test]
    fn deploy_creates_real_hard_links() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "Textures/x.dds", "original");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        let dest = data.join("Textures/x.dds");
        assert_eq!(fs::read_to_string(&dest).unwrap(), "original");
        // Hard-link proof: editing the source is visible through the deployed name.
        fs::write(&record.entries[0].source, "edited").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "edited");
        assert_eq!(record.target_root, data);
        assert_eq!(record.entries.len(), 1);
    }

    #[test]
    fn deploy_mirrors_nested_directories() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "a/b/c.txt", "deep");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        assert!(data.join("a/b/c.txt").exists());
        assert!(!record.created_dirs.is_empty());
    }

    #[test]
    fn verify_passes_then_reports_missing() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "x.txt", "x");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        assert!(d.verify(&record).is_ok());
        fs::remove_file(data.join("x.txt")).unwrap();
        let report = d.verify(&record);
        assert!(!report.is_ok());
        assert_eq!(report.expected, 1);
        assert_eq!(report.missing.len(), 1);
    }

    #[test]
    fn undeploy_removes_files_and_created_dirs() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "sub/x.txt", "x");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        assert!(data.join("sub/x.txt").exists());
        let report = d.undeploy(&record, &NullSink);
        assert!(report.is_fully_resolved());
        assert!(!data.join("sub/x.txt").exists());
        assert!(!data.join("sub").exists(), "created dir removed");
        assert!(data.exists(), "target root preserved");
    }

    #[test]
    fn undeploy_is_idempotent() {
        let (_tmp, base) = temp();
        let (record, _data) = record_one(&base, "x.txt", "x");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
        // Re-running tolerates already-missing files.
        assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
    }

    #[test]
    fn undeploy_preserves_foreign_files_in_shared_dirs() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "sub/mine.txt", "mine");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        // A file we did not create, inside a directory we did.
        let foreign = data.join("sub/foreign.txt");
        write(&foreign, "not ours");
        let report = d.undeploy(&record, &NullSink);
        assert!(report.is_fully_resolved());
        assert!(!data.join("sub/mine.txt").exists(), "our file removed");
        assert!(foreign.exists(), "foreign file untouched");
        assert!(data.join("sub").exists(), "non-empty dir left in place");
    }

    #[test]
    fn deploy_leaves_staging_sources_intact() {
        let (_tmp, base) = temp();
        let (record, _data) = record_one(&base, "x.txt", "x");
        let src = record.entries[0].source.clone();
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        d.undeploy(&record, &NullSink);
        assert!(src.exists(), "staging source survives deploy+undeploy");
    }

    #[test]
    fn deploy_writes_winning_content_on_conflict() {
        let (_tmp, base) = temp();
        let a = base.join("mods/A");
        let b = base.join("mods/B");
        write(&a.join("f.txt"), "from-a");
        write(&b.join("f.txt"), "from-b");
        let data = base.join("Data");
        let plan =
            DeployPlan::from_mods(&data, &[ModSource::new("A", &a), ModSource::new("B", &b)])
                .expect("plan");
        let record =
            DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
                .expect("record");
        HardlinkDeployer::new()
            .deploy(&record, &NullSink)
            .expect("deploy");
        assert_eq!(fs::read_to_string(data.join("f.txt")).unwrap(), "from-b");
    }

    #[test]
    fn deploy_backs_up_preexisting_file() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "sub/x.txt", "ours");
        // A real, pre-existing file occupies the destination before deploy.
        let dest = data.join("sub/x.txt");
        write(&dest, "preexisting");
        HardlinkDeployer::new()
            .deploy(&record, &NullSink)
            .expect("deploy");
        // The deployed link wins at the destination...
        assert_eq!(fs::read_to_string(&dest).unwrap(), "ours");
        // ...and the original is preserved verbatim under the backup root.
        let backup = record.backup_root.join("sub/x.txt");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "preexisting");
    }

    #[test]
    fn undeploy_restores_a_clobbered_preexisting_file() {
        let (_tmp, base) = temp();
        let (record, data) = record_one(&base, "sub/x.txt", "ours");
        let dest = data.join("sub/x.txt");
        write(&dest, "preexisting");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        assert_eq!(fs::read_to_string(&dest).unwrap(), "ours");
        let report = d.undeploy(&record, &NullSink);
        assert!(report.is_fully_resolved());
        // The user's original file is back, byte for byte...
        assert_eq!(fs::read_to_string(&dest).unwrap(), "preexisting");
        // ...and the backup root is swept clean.
        assert!(
            !record.backup_root.exists(),
            "backup root removed when empty"
        );
    }

    #[test]
    fn undeploy_is_re_runnable_without_deleting_restored_originals() {
        let (_tmp, base) = temp();
        // Two entries: one with a pre-existing original, one we create ourselves.
        let m = base.join("mods/M");
        write(&m.join("kept.txt"), "ours");
        write(&m.join("made.txt"), "made");
        let data = base.join("Data");
        write(&data.join("kept.txt"), "preexisting");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("M", &m)]).expect("plan");
        let record =
            DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)
                .expect("record");
        let d = HardlinkDeployer::new();
        d.deploy(&record, &NullSink).expect("deploy");
        // First reversal restores the original and removes the created file.
        assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
        assert_eq!(
            fs::read_to_string(data.join("kept.txt")).unwrap(),
            "preexisting"
        );
        assert!(!data.join("made.txt").exists());
        // Second reversal (a recovery retry) must be a no-op for the restored
        // original — it must NOT fall through and delete it.
        assert!(d.undeploy(&record, &NullSink).is_fully_resolved());
        assert_eq!(
            fs::read_to_string(data.join("kept.txt")).unwrap(),
            "preexisting",
            "restored original survives a re-run"
        );
        assert!(!data.join("made.txt").exists());
    }

    #[test]
    fn undeploy_reports_a_residual_backup_file() {
        let (_tmp, base) = temp();
        let (record, _data) = record_one(&base, "x.txt", "ours");
        // A stray file under the backup root that no entry will restore.
        write(&record.backup_root.join("stray.txt"), "orphan");
        let report = HardlinkDeployer::new().undeploy(&record, &NullSink);
        assert!(!report.is_fully_resolved());
        assert!(
            report
                .unresolved
                .iter()
                .any(|e| matches!(e, DeployError::ResidualBackup { .. }))
        );
    }
}
