use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use super::{
    io_err, same_volume, DeployError, DeployManifest, DeployPlan, Deployer, DeployerKind,
    ProgressEvent, ProgressSink, VerifyReport,
};

/// Deploys mods by creating NTFS hard links from each staged file into the game's
/// target directory. The files physically appear in the target with no data copied,
/// so every process (the game, F4SE, xEdit, LOOT) sees them without any special launch.
///
/// Constraints inherent to hard links: the staged files must be on the same volume as
/// the target, and only files (not directories) can be linked, so the directory tree
/// is mirrored on disk.
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
        // The target root itself is never recorded as "created" — we must not remove
        // the game's Data directory on undeploy.
        fs::create_dir_all(&plan.target_root).map_err(|e| io_err(&plan.target_root, e))?;

        progress.on_event(ProgressEvent::Started { total: plan.len() });

        let mut created_dirs: Vec<Utf8PathBuf> = Vec::new();
        let mut deployed: Vec<Utf8PathBuf> = Vec::new();

        for (index, file) in plan.files().iter().enumerate() {
            let dest = plan.target_root.join(&file.relative);
            if let Some(parent) = dest.parent() {
                create_dir_all_tracked(&plan.target_root, parent, &mut created_dirs)?;
            }
            // Replace anything already at the destination, so re-deploying is idempotent.
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
            backend: self.kind(),
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

        // Remove directories we created, deepest first. Ignore "not empty" and
        // "not found": a non-empty directory holds files we don't own.
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

/// Create `dir` and any missing ancestors under `root`, recording newly created
/// directories (relative to `root`, shallowest first) so they can be removed on
/// undeploy.
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
