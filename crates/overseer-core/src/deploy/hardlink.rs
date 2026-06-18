use std::fs;

use super::error::io_err;
use super::{
    DeployError, DeployManifest, DeployPlan, Deployer, DeployerKind, ProgressEvent, ProgressSink,
    VerifyReport,
};
use camino::{Utf8Path, Utf8PathBuf};

/// Deploys mods by creating NTFS hard links from each staged file into the game's target directory.
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
        plan: &DeployPlan,
        progress: &dyn ProgressSink,
    ) -> Result<DeployManifest, DeployError> {
        self.check_supported(plan)?;
        fs::create_dir_all(&plan.target_root).map_err(|e| io_err(&plan.target_root, e))?;

        progress.on_event(ProgressEvent::Started { total: plan.len() });

        let mut created_dirs: Vec<Utf8PathBuf> = Vec::new();
        let mut deployed: Vec<Utf8PathBuf> = Vec::new();

        for (index, file) in plan.files().iter().enumerate() {
            let dest = plan.target_root.join(&file.relative);
            if let Some(parent) = dest.parent() {
                create_dir_all_tracked(&plan.target_root, parent, &mut created_dirs)?;
            }

            // Replace anything already at the destination
            if dest.symlink_metadata().is_ok() {
                fs::remove_file(&dest).map_err(|e| io_err(&dest, e))?;
            }
            fs::hard_link(&file.source, &dest).map_err(|e| io_err(&dest, e))?;
            deployed.push(file.relative.clone());
            progress.on_event(ProgressEvent::Deployed {
                index,
                relative: file.relative.as_path(),
            });
        }

        progress.on_event(ProgressEvent::Finished);
        Ok(DeployManifest {
            deployer: self.kind(),
            target_root: plan.target_root.clone(),
            files: deployed,
            created_dirs,
        })
    }

    fn undeploy(
        &self,
        manifest: &DeployManifest,
        progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        progress.on_event(ProgressEvent::Started {
            total: manifest.files.len(),
        });

        for (index, relative) in manifest.files.iter().enumerate() {
            let dest = manifest.target_root.join(relative);
            match fs::remove_file(&dest) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_err(&dest, e)),
            }

            progress.on_event(ProgressEvent::Removed {
                index,
                relative: relative.as_path(),
            });
        }

        // Remove directories we created
        for relative in manifest.created_dirs.iter().rev() {
            let dir = manifest.target_root.join(relative);
            let _ = fs::remove_dir(&dir);
        }

        progress.on_event(ProgressEvent::Finished);
        Ok(())
    }

    fn verify(&self, manifest: &DeployManifest) -> VerifyReport {
        let missing = manifest
            .files
            .iter()
            .filter(|relative| !manifest.target_root.join(relative).exists())
            .cloned()
            .collect();

        VerifyReport {
            expected: manifest.files.len(),
            missing,
        }
    }
}

/// Create `dir` and any missing ancestors under `root`, recording newly created directories
fn create_dir_all_tracked(
    root: &Utf8Path,
    dir: &Utf8Path,
    created: &mut Vec<Utf8PathBuf>,
) -> Result<(), DeployError> {
    if dir == root || dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        create_dir_all_tracked(root, parent, created)?;
    }

    fs::create_dir(dir).map_err(|e| io_err(dir, e))?;
    if let Ok(relative) = dir.strip_prefix(root) {
        created.push(relative.to_owned());
    }
    Ok(())
}

/// Whether two paths live on teh same windows volume (drive letter)
fn same_volume(a: &Utf8Path, b: &Utf8Path) -> bool {
    match (volume_id(a), volume_id(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => true,
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
mod tests {
    use super::*;
    use crate::deploy::{ModSource, NullSink};
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

    // --- volume detection (Windows drive-letter semantics) ---

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
        // Relative paths -> volume unknown -> never a false CrossVolume.
        assert!(same_volume(Utf8Path::new("a/b"), Utf8Path::new("c/d")));
    }

    // --- create_dir_all_tracked ---

    #[test]
    fn tracked_dir_creation_records_only_newly_made_dirs() {
        let (_tmp, base) = temp();
        let root = base.join("root");
        fs::create_dir_all(&root).expect("root");
        let mut created = Vec::new();
        create_dir_all_tracked(&root, &root.join("a/b"), &mut created).expect("create");
        assert!(root.join("a/b").is_dir());
        // `a` and `a/b` are new; the pre-existing root is not recorded.
        assert_eq!(created.len(), 2);
    }

    #[test]
    fn tracked_dir_creation_is_noop_for_existing_dir() {
        let (_tmp, base) = temp();
        let root = base.join("root");
        let existing = root.join("a");
        fs::create_dir_all(&existing).expect("pre-create");
        let mut created = Vec::new();
        create_dir_all_tracked(&root, &existing, &mut created).expect("noop");
        assert!(created.is_empty());
    }

    // --- deploy / verify / undeploy ---

    #[test]
    fn deploy_creates_real_hard_links() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "Textures/x.dds", "original");

        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");

        let dest = data.join("Textures/x.dds");
        assert_eq!(fs::read_to_string(&dest).unwrap(), "original");

        // Hard-link proof: editing the source is visible through the deployed name.
        fs::write(&plan.files()[0].source, "edited").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "edited");

        assert_eq!(manifest.deployer, DeployerKind::HardLink);
        assert_eq!(manifest.target_root, data);
        assert_eq!(manifest.files.len(), 1);
    }

    #[test]
    fn deploy_is_idempotent() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "a/b.txt", "v1");
        let d = HardlinkDeployer::new();
        d.deploy(&plan, &NullSink).expect("first deploy");
        // A second deploy must replace, not fail on, the existing destination.
        d.deploy(&plan, &NullSink).expect("second deploy");
        assert_eq!(fs::read_to_string(data.join("a/b.txt")).unwrap(), "v1");
    }

    #[test]
    fn deploy_mirrors_nested_directories() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "a/b/c.txt", "deep");
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");
        assert!(data.join("a/b/c.txt").exists());
        assert!(!manifest.created_dirs.is_empty());
    }

    #[test]
    fn verify_passes_then_reports_missing() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "x.txt", "x");
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");

        assert!(d.verify(&manifest).is_ok());

        fs::remove_file(data.join("x.txt")).unwrap();
        let report = d.verify(&manifest);
        assert!(!report.is_ok());
        assert_eq!(report.expected, 1);
        assert_eq!(report.missing.len(), 1);
    }

    #[test]
    fn undeploy_removes_files_and_created_dirs() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "sub/x.txt", "x");
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");
        assert!(data.join("sub/x.txt").exists());

        d.undeploy(&manifest, &NullSink).expect("undeploy");
        assert!(!data.join("sub/x.txt").exists());
        assert!(!data.join("sub").exists(), "created dir removed");
        assert!(data.exists(), "target root preserved");
    }

    #[test]
    fn undeploy_is_idempotent() {
        let (_tmp, base) = temp();
        let (plan, _data) = plan_one(&base, "x.txt", "x");
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");
        d.undeploy(&manifest, &NullSink).expect("first undeploy");
        // Re-running tolerates already-missing files.
        d.undeploy(&manifest, &NullSink).expect("second undeploy");
    }

    #[test]
    fn undeploy_preserves_foreign_files_in_shared_dirs() {
        let (_tmp, base) = temp();
        let (plan, data) = plan_one(&base, "sub/mine.txt", "mine");
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");

        // A file we did not create, inside a directory we did.
        let foreign = data.join("sub/foreign.txt");
        write(&foreign, "not ours");

        d.undeploy(&manifest, &NullSink).expect("undeploy");
        assert!(!data.join("sub/mine.txt").exists(), "our file removed");
        assert!(foreign.exists(), "foreign file untouched");
        assert!(data.join("sub").exists(), "non-empty dir left in place");
    }

    #[test]
    fn deploy_leaves_staging_sources_intact() {
        let (_tmp, base) = temp();
        let (plan, _data) = plan_one(&base, "x.txt", "x");
        let src = plan.files()[0].source.clone();
        let d = HardlinkDeployer::new();
        let manifest = d.deploy(&plan, &NullSink).expect("deploy");
        d.undeploy(&manifest, &NullSink).expect("undeploy");
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

        HardlinkDeployer::new()
            .deploy(&plan, &NullSink)
            .expect("deploy");
        assert_eq!(fs::read_to_string(data.join("f.txt")).unwrap(), "from-b");
    }

    #[test]
    fn kind_is_hardlink() {
        assert_eq!(HardlinkDeployer::new().kind(), DeployerKind::HardLink);
    }

    #[test]
    fn check_supported_ok_within_one_volume() {
        let (_tmp, base) = temp();
        let (plan, _data) = plan_one(&base, "x.txt", "x");
        assert!(HardlinkDeployer::new().check_supported(&plan).is_ok());
    }
}
