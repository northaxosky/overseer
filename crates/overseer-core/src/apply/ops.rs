//! Deploy, purge, and status orchestration over an instance's profile.

use super::error::{self, ApplyError};
use super::lock::InstanceLock;
use super::state::{Deployment, SaveRedirect, Status};

use crate::deploy::{
    BACKUP_DIR, DeployError, DeployPlan, DeployRecord, ModSource, NullSink, ProgressSink, ROOT_DIR,
    VerifyReport, deployer_for, strip_data_prefix,
};
use crate::fs;
use crate::instance::{Instance, Profile};
use crate::plugins::{self, PluginLoadOrder};
use crate::restore::Restore;
use crate::saves;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeSet;
use walkdir::WalkDir;

/// Deploy a profile's enabled mods into the instance's game directory
pub fn deploy_profile(
    instance: &Instance,
    profile_name: &str,
    progress: &dyn ProgressSink,
) -> Result<Deployment, ApplyError> {
    tracing::info!(profile = profile_name, "deploying profile");
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, progress)?;

    if Deployment::exists(instance) {
        // recover_if_needed only ever leaves a *Committed* journal
        return Err(ApplyError::AlreadyDeployed {
            path: Deployment::path(instance),
        });
    }

    let mut profile = Profile::load_existing(instance, profile_name)?;
    profile.reconcile(instance)?;
    let mut sources = profile.deploy_sources(instance);

    // Overwrite folder is the highest priority "mod": It wins every conflict
    let overwrite = instance.overwrite_dir();
    fs::ensure_dir(&overwrite)?;
    sources.push(ModSource::new("Overwrite", &overwrite));

    let plan = DeployPlan::from_rooted_mods(&instance.config.game_dir, &sources)?;

    let deployer = deployer_for(instance.config.deployer);
    deployer.check_supported(&plan)?;

    let backup_root = instance.config.game_dir.join(BACKUP_DIR);
    guard_no_orphaned_backup(&backup_root)?;
    let record = DeployRecord::from_plan(&plan, backup_root, instance.config.deployer)?;

    let local_dir = instance.local_dir()?;
    fs::ensure_dir(&local_dir)?;

    // Profile bookkeeping doesn't touch anything so it's safe
    let order = prepare_load_order(instance, &profile)?;
    let local_saves = profile.local_saves;

    // Capture the user's original Plugins.txt
    let plugins_txt_backup = plugins::read_plugins_txt(&local_dir)?;

    // First write: journal as InProgress
    let mut deployment = Deployment {
        status: Status::InProgress,
        profile: profile.name,
        record,
        plugins_txt_backup,
        plugins_txt_intended: None,
        save_redirect: None,
    };
    deployment.save(instance)?;

    if let Err(e) = deployer.deploy(&deployment.record, progress) {
        tracing::warn!(error = %e, "deploy failed; rolling back");
        if let Err(rb) = reverse_and_finalize(instance, deployment, progress) {
            tracing::warn!(error = %rb, "rollback after deploy failure was incomplete");
        }
        return Err(e.into());
    }

    if let Err(e) = plugins::write_active_plugins(
        instance.config.game.load_order_id(),
        &instance.config.game_dir,
        &local_dir,
        &order.plugins,
    ) {
        tracing::warn!(error = %e, "writing Plugins.txt failed; rolling back");
        if let Err(rb) = reverse_and_finalize(instance, deployment, progress) {
            tracing::warn!(error = %rb, "rollback after Plugins.txt failure was incomplete");
        }
        return Err(e.into());
    }

    // Capture exactly what we wrote, so a later reversal can tell Plugins.txt apart
    deployment.plugins_txt_intended = plugins::read_plugins_txt(&local_dir)?;

    // Redirect this profile's saves into its own folder, if it opts in
    if local_saves {
        let (custom_ini, saves_dir) = save_paths(instance, &deployment.profile)?;
        match saves::apply_save_redirect(&custom_ini, &saves_dir, &deployment.profile) {
            Ok(original) => deployment.save_redirect = Some(SaveRedirect { original }),
            Err(e) => {
                tracing::warn!(error = %e, "writing save redirect failed; rolling back");
                if let Err(rb) = reverse_and_finalize(instance, deployment, progress) {
                    tracing::warn!(error = %rb, "rollback after save-redirect failure was incomplete");
                }
                return Err(e.into());
            }
        }
    }

    // Second Write: InProgress -> Committed flip
    deployment.status = Status::Committed;
    deployment.save(instance)?;
    tracing::info!(
        profile = %deployment.profile,
        files = deployment.record.entries.len(),
        "deployment committed"
    );
    Ok(deployment)
}

/// Reverses the instance's live deployment
pub fn purge(instance: &Instance, progress: &dyn ProgressSink) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, progress)?;

    let deployment = Deployment::load(instance)?;
    tracing::info!(profile = %deployment.profile, "purging live deployment");
    capture_overwrite(instance, &deployment.record)?;
    reverse_and_finalize(instance, deployment, progress)
}

/// Before tearing down, move files that appeared in directories our deploy created
fn capture_overwrite(instance: &Instance, record: &DeployRecord) -> Result<(), ApplyError> {
    let overwrite = instance.overwrite_dir();
    // Game-relative paths we deployed, lowercased, so we never capture our own files
    let ours: BTreeSet<String> = record
        .entries
        .iter()
        .map(|e| e.relative.as_str().to_lowercase())
        .collect();

    for created in &record.created_dirs {
        let dir = record.target_root.join(created);
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir) {
            let entry = entry.map_err(|e| error::walk_io_err(&dir, e))?;
            if !entry.file_type().is_file() {
                continue;
            }

            let abs = Utf8Path::from_path(entry.path())
                .ok_or_else(|| DeployError::NonUtf8Path(error::non_utf8(entry.path())))?;
            let relative = abs
                .strip_prefix(&record.target_root)
                .expect("walked file is under the target root");
            if ours.contains(&relative.as_str().to_lowercase()) {
                continue; // one of our deployed files; the reversal handles it
            }

            capture_move(abs, &overwrite.join(overwrite_staging_path(relative)))?;
        }
    }
    Ok(())
}

/// Inverse of the deploy mapping: turn a deployed (game-relative) path back into its overwrite *staging* layout
fn overwrite_staging_path(game_relative: &Utf8Path) -> Utf8PathBuf {
    match strip_data_prefix(game_relative) {
        // Under Data/: the staging layout drops the Data/ prefix
        Some(under_data) if !under_data.as_str().is_empty() => under_data,
        // Outside Data/ (a game-root file): it came from the mod's Root/ folder
        _ => Utf8Path::new(ROOT_DIR).join(game_relative),
    }
}

/// Move a captured file into the overwrite folder, creating parents
fn capture_move(from: &Utf8Path, to: &Utf8Path) -> Result<(), ApplyError> {
    if let Some(parent) = to.parent() {
        fs::ensure_dir(parent)?;
    }
    if std::fs::rename(from, to).is_err() {
        copy_then_remove(
            from,
            to,
            |a, b| std::fs::copy(a, b).map(drop),
            |p| std::fs::remove_file(p),
        )?;
    }
    Ok(())
}

/// Cross-volume move fallback: copy, then remove the source; on a failed remove, undo the copy
fn copy_then_remove(
    from: &Utf8Path,
    to: &Utf8Path,
    copy: impl Fn(&Utf8Path, &Utf8Path) -> std::io::Result<()>,
    remove: impl Fn(&Utf8Path) -> std::io::Result<()>,
) -> Result<(), ApplyError> {
    copy(from, to).map_err(|e| error::io_err(to, e))?;
    if let Err(e) = remove(from) {
        let _ = remove(to);
        return Err(error::io_err(from, e).into());
    }
    Ok(())
}

/// A snapshot of an instance's live deployment
#[derive(Debug)]
pub struct DeploymentStatus {
    pub deployment: Deployment,
    pub verified: VerifyReport,
}

/// Report the instance's live deployment, or `None` if nothing is deployed
pub fn status(instance: &Instance) -> Result<Option<DeploymentStatus>, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, &NullSink)?;

    if !Deployment::exists(instance) {
        return Ok(None);
    }
    let deployment = Deployment::load(instance)?;
    let verified = deployer_for(deployment.record.deployer).verify(&deployment.record);
    Ok(Some(DeploymentStatus {
        deployment,
        verified,
    }))
}

/// Rename an installed mod, refusing while any deployment is live
pub fn rename_mod(instance: &Instance, old: &str, new: &str) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, &NullSink)?;

    if Deployment::exists(instance) {
        return Err(ApplyError::DeployedCannotRename {
            path: Deployment::path(instance),
        });
    }

    instance.rename_mod(old, new).map_err(Into::into)
}

/// Rename a profile, refusing only while that profile's own deployment is live
pub fn rename_profile(instance: &mut Instance, old: &str, new: &str) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, &NullSink)?;

    if Deployment::exists(instance)
        && Deployment::load(instance)?
            .profile
            .eq_ignore_ascii_case(old)
    {
        return Err(ApplyError::DeployedCannotRename {
            path: Deployment::path(instance),
        });
    }
    instance.rename_profile(old, new)?;

    // The profile renamed on disk; keep the persisted default pointer in sync
    if instance.config.default_profile.eq_ignore_ascii_case(old) {
        let prev = std::mem::replace(&mut instance.config.default_profile, new.to_owned());
        if let Err(e) = instance.save() {
            instance.config.default_profile = prev;
            return Err(ApplyError::DefaultProfileNotUpdated(e));
        }
    }
    Ok(())
}

/// Lock held recovery used by every mutating entry point
fn recover_if_needed(instance: &Instance, progress: &dyn ProgressSink) -> Result<(), ApplyError> {
    if !Deployment::exists(instance) {
        return Ok(());
    }
    let deployment = Deployment::load(instance)?;
    match deployment.status {
        Status::Committed => Ok(()),
        Status::InProgress | Status::RecoveryFailed => {
            tracing::warn!(
                status = ?deployment.status,
                profile = %deployment.profile,
                "interrupted deployment found; reversing"
            );
            reverse_and_finalize(instance, deployment, progress)
        }
    }
}

/// Shared reversal for purge and recovery: run record driven reversal, restore Plugins.txt, resolve
fn reverse_and_finalize(
    instance: &Instance,
    deployment: Deployment,
    progress: &dyn ProgressSink,
) -> Result<(), ApplyError> {
    let deployer = deployer_for(deployment.record.deployer);
    let report = deployer.undeploy(&deployment.record, progress);

    let local_dir = instance.local_dir()?;
    let plugins_restore = plugins::restore_plugins_txt_if_ours(
        &local_dir,
        deployment.plugins_txt_backup.as_deref(),
        deployment.plugins_txt_intended.as_deref(),
    );

    if let Ok(Restore::Conflict) = &plugins_restore {
        tracing::warn!(
            "Plugins.txt changed since deployment; kept the current file instead of restoring the pre-deployment version"
        );
    }

    // Undo the save redirect, but only if this deployment set one
    let save_restore = match &deployment.save_redirect {
        Some(redirect) => {
            let (custom_ini, _) = save_paths(instance, &deployment.profile)?;
            saves::restore_save_redirect(
                &custom_ini,
                &deployment.profile,
                redirect.original.as_deref(),
            )
        }
        None => Ok(Restore::Restored),
    };
    if let Ok(Restore::Conflict) = &save_restore {
        tracing::warn!(
            "SLocalSavePath changed since deployment; kept the current value instead of restoring"
        );
    }

    if report.is_fully_resolved() && plugins_restore.is_ok() && save_restore.is_ok() {
        tracing::info!("reversal complete; clearing journal");
        Deployment::remove(instance)
    } else {
        tracing::warn!("reversal incomplete; keeping RecoveryFailed journal");
        let path = Deployment::path(instance);
        let failed = Deployment {
            status: Status::RecoveryFailed,
            ..deployment
        };
        failed.save(instance)?;
        let restore_err = plugins_restore
            .err()
            .map(ApplyError::from)
            .or_else(|| save_restore.err().map(ApplyError::from));
        Err(restore_err.unwrap_or(ApplyError::RecoveryFailed { path }))
    }
}

/// Discover the profile's plugins and reconcile and save its load order
fn prepare_load_order(
    instance: &Instance,
    profile: &Profile,
) -> Result<PluginLoadOrder, ApplyError> {
    let discovered = plugins::discover_plugins(instance, profile)?;
    let mut order = PluginLoadOrder::load(instance, &profile.name)?;
    order.reconcile(&discovered);
    order.save(instance)?;
    Ok(order)
}

/// This profile's `Fallout4Custom.ini` and `Saves/<profile>/` under the instance's My Games (INI) directory
fn save_paths(
    instance: &Instance,
    profile: &str,
) -> Result<(Utf8PathBuf, Utf8PathBuf), ApplyError> {
    let ini_dir = instance.ini_dir()?;
    let stem = instance.config.game.ini_stem();
    let custom_ini = ini_dir.join(format!("{stem}Custom.ini"));
    let saves_dir = instance.saves_dir(profile)?;
    Ok((custom_ini, saves_dir))
}

/// Don't start a deploy when the backup dir survives from a previous run
fn guard_no_orphaned_backup(backup_root: &Utf8Path) -> Result<(), ApplyError> {
    if backup_root.exists() {
        return Err(ApplyError::OrphanedBackup {
            path: backup_root.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/ops.rs"]
mod tests;
