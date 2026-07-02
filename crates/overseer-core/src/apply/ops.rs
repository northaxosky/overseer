//! Deploy, purge, and status orchestration over an instance's profile.

use super::error::{self, ApplyError};
use super::lock::InstanceLock;
use super::state::{Deployment, SaveRedirect, Status};

use crate::deploy::{
    BACKUP_DIR, DeployError, DeployPlan, DeployRecord, ModSource, NullSink, ProgressSink, ROOT_DIR,
    VerifyReport, deployer_for, strip_data_prefix,
};
use crate::instance::{Instance, Profile};
use crate::plugins::{self, PluginLoadOrder, PluginsRestore};
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

    let mut profile = Profile::load(instance, profile_name)?;
    profile.reconcile(instance)?;
    let mut sources = profile.deploy_sources(instance);

    // Overwrite folder is the highest priority "mod": It wins every conflict
    let overwrite = instance.overwrite_dir();
    std::fs::create_dir_all(&overwrite).map_err(|e| error::io_err(&overwrite, e))?;
    sources.push(ModSource::new("Overwrite", &overwrite));

    let plan = DeployPlan::from_rooted_mods(&instance.config.game_dir, &sources)?;

    let deployer = deployer_for(instance.config.deployer);
    deployer.check_supported(&plan)?;

    let backup_root = instance.config.game_dir.join(BACKUP_DIR);
    guard_no_orphaned_backup(&backup_root)?;
    let record = DeployRecord::from_plan(&plan, backup_root, instance.config.deployer)?;

    let local_dir = instance.local_dir()?;
    std::fs::create_dir_all(&local_dir).map_err(|e| error::io_err(&local_dir, e))?;

    // Profile bookkeeping doesnt touch anything so its safe
    let order = prepare_load_order(instance, &profile)?;
    let local_saves = profile.local_saves;

    // Capture the users original Plugins.txt
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
    // Game-relative paths we deployed, lowercased, so we never capture our own files.
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
            let entry = entry.map_err(|e| {
                let io = e
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("walk capture dir"));
                error::io_err(&dir, io)
            })?;
            if !entry.file_type().is_file() {
                continue;
            }

            let abs = Utf8Path::from_path(entry.path())
                .ok_or_else(|| DeployError::NonUtf8Path(entry.path().display().to_string()))?;
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
        // Under Data/: the staging layout drops the Data/ prefix.
        Some(under_data) if !under_data.as_str().is_empty() => under_data,
        // Outside Data/ (a game-root file): it came from the mod's Root/ folder.
        _ => Utf8Path::new(ROOT_DIR).join(game_relative),
    }
}

/// Move a captured file into the overwrite folder, creating parents
fn capture_move(from: &Utf8Path, to: &Utf8Path) -> Result<(), ApplyError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| error::io_err(parent, e))?;
    }
    if std::fs::rename(from, to).is_err() {
        std::fs::copy(from, to).map_err(|e| error::io_err(to, e))?;
        std::fs::remove_file(from).map_err(|e| error::io_err(from, e))?;
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

/// Rename an installed mod, refusing while any deployment is live.
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

    if let Ok(PluginsRestore::Conflict) = &plugins_restore {
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
        None => Ok(saves::SaveRestore::Restored),
    };
    if let Ok(saves::SaveRestore::Conflict) = &save_restore {
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

/// This profile's `Fallout4Custom.ini` and `Saves/<profile>/` under the instance's My Games (INI) directory.
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

/// Dont start a deploy when the backup dir survives from a previous run
fn guard_no_orphaned_backup(backup_root: &Utf8Path) -> Result<(), ApplyError> {
    if backup_root.exists() {
        return Err(ApplyError::OrphanedBackup {
            path: backup_root.to_owned(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::NullSink;
    use crate::instance::Instance;
    use crate::test_support::{install_mod, install_plugin, save_profile, temp_instance};
    use camino::Utf8PathBuf;

    /// Absolute path of a file as it would land under the game's Data/ directory.
    fn deployed(instance: &Instance, rel: &str) -> Utf8PathBuf {
        instance.config.game_dir.join("Data").join(rel)
    }

    /// Rewrite the on-disk journal's status to mimic a crash at a given stage.
    fn force_status(instance: &Instance, status: Status) {
        let mut deployment = Deployment::load(instance).expect("load journal");
        deployment.status = status;
        deployment.save(instance).expect("save journal");
    }

    #[test]
    fn deploy_hardlinks_enabled_mod_files_into_data() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        let path = deployed(&instance, "Textures/a.dds");
        assert!(path.exists(), "file should be deployed into Data/");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "pixels");
    }

    #[test]
    fn deploy_records_recoverable_state() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        assert_eq!(deployment.profile, "Default");
        assert!(Deployment::exists(&instance));
        assert!(Deployment::path(&instance).exists());
    }

    #[test]
    fn purge_removes_deployed_files_and_state() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Meshes/m.nif", "tris")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        let path = deployed(&instance, "Meshes/m.nif");
        assert!(path.exists());

        purge(&instance, &NullSink).expect("purge");
        assert!(!path.exists(), "purge should remove deployed files");
        assert!(!Deployment::exists(&instance), "purge should clear state");
    }

    #[test]
    fn second_deploy_is_refused() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("first deploy");
        let err =
            deploy_profile(&instance, "Default", &NullSink).expect_err("second deploy must fail");
        assert!(matches!(err, ApplyError::AlreadyDeployed { .. }));
    }

    #[test]
    fn rename_mod_is_refused_while_a_deployment_is_live() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        let err = rename_mod(&instance, "CoolMod", "BetterMod")
            .expect_err("rename must be refused while deployed");
        assert!(matches!(err, ApplyError::DeployedCannotRename { .. }));
        assert!(instance.mods_dir().join("CoolMod").is_dir());
        assert!(!instance.mods_dir().join("BetterMod").exists());
    }

    #[test]
    fn rename_mod_succeeds_when_no_deployment_is_live() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        rename_mod(&instance, "CoolMod", "BetterMod").expect("rename");

        assert!(!instance.mods_dir().join("CoolMod").exists());
        assert!(instance.mods_dir().join("BetterMod").is_dir());
    }

    #[test]
    fn purge_without_deployment_errors() {
        let (_tmp, instance) = temp_instance();
        let err = purge(&instance, &NullSink).expect_err("purge with nothing deployed must fail");
        assert!(matches!(err, ApplyError::NotDeployed { .. }));
    }

    #[test]
    fn deploy_backs_up_and_purge_restores_a_preexisting_data_file() {
        let (_tmp, instance) = temp_instance();
        // A vanilla file already in the game's Data/ that a mod will overwrite.
        let data_file = deployed(&instance, "Textures/conflict.dds");
        std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
        std::fs::write(&data_file, "vanilla").expect("seed vanilla");

        // A mod shipping the same file (non-plugin, so no load-order parsing).
        install_mod(
            &instance,
            "Overwriter",
            &[("Textures/conflict.dds", "modded")],
        );
        save_profile(&instance, "Default", &[("Overwriter", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        // The mod's version wins at the destination.
        assert_eq!(std::fs::read_to_string(&data_file).expect("read"), "modded");

        purge(&instance, &NullSink).expect("purge");
        // The vanilla original is restored byte-for-byte.
        assert_eq!(
            std::fs::read_to_string(&data_file).expect("read"),
            "vanilla"
        );
    }

    #[test]
    fn purge_leaves_a_data_file_replaced_after_deployment() {
        let (_tmp, instance) = temp_instance();
        // A mod ships a non-plugin file we deploy as a hard link.
        install_mod(&instance, "Texturer", &[("Textures/a.dds", "ours")]);
        save_profile(&instance, "Default", &[("Texturer", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        let dest = deployed(&instance, "Textures/a.dds");
        assert_eq!(std::fs::read_to_string(&dest).expect("read"), "ours");

        // A tool rewrites the deployed file in place after deployment, breaking the link.
        std::fs::remove_file(&dest).expect("remove our link");
        std::fs::write(&dest, "tool output").expect("tool writes");

        purge(&instance, &NullSink).expect("purge");
        // Purge must not delete the externally written file as if it were still ours.
        assert!(dest.exists(), "an externally replaced file survives purge");
        assert_eq!(std::fs::read_to_string(&dest).expect("read"), "tool output");
    }

    #[test]
    fn deploy_routes_root_content_to_the_game_root_and_purge_restores() {
        let (_tmp, instance) = temp_instance();
        // A mod with Root/ content (-> game root) and ordinary Data content (-> Data/).
        install_mod(
            &instance,
            "ScriptExtender",
            &[
                ("Root/f4se_loader.exe", "loader"),
                ("Scripts/Quest.pex", "script"),
            ],
        );
        save_profile(&instance, "Default", &[("ScriptExtender", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        let root_exe = instance.config.game_dir.join("f4se_loader.exe");
        let data_file = deployed(&instance, "Scripts/Quest.pex");
        assert_eq!(
            std::fs::read_to_string(&root_exe).expect("root file"),
            "loader"
        );
        assert_eq!(
            std::fs::read_to_string(&data_file).expect("data file"),
            "script"
        );

        purge(&instance, &NullSink).expect("purge");
        assert!(!root_exe.exists(), "root file removed on purge");
        assert!(!data_file.exists(), "data file removed on purge");
    }

    #[test]
    fn deploy_backs_up_and_purge_restores_a_preexisting_root_file() {
        let (_tmp, instance) = temp_instance();
        // A vanilla DLL already sitting next to the game exe that a mod overwrites.
        std::fs::create_dir_all(&instance.config.game_dir).expect("mk game dir");
        let root_dll = instance.config.game_dir.join("dxgi.dll");
        std::fs::write(&root_dll, "vanilla").expect("seed vanilla");

        install_mod(&instance, "ReShade", &[("Root/dxgi.dll", "modded")]);
        save_profile(&instance, "Default", &[("ReShade", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        assert_eq!(std::fs::read_to_string(&root_dll).expect("read"), "modded");

        purge(&instance, &NullSink).expect("purge");
        // The vanilla original next to the exe is restored byte-for-byte.
        assert_eq!(std::fs::read_to_string(&root_dll).expect("read"), "vanilla");
    }

    #[test]
    fn purge_captures_a_generated_file_from_a_mod_created_dir() {
        let (_tmp, instance) = temp_instance();
        // A mod whose file forces creating Data/F4SE/Plugins/.
        install_mod(
            &instance,
            "Buffout",
            &[("F4SE/Plugins/Buffout4.dll", "plugin")],
        );
        save_profile(&instance, "Default", &[("Buffout", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // The game writes a crash log next to the plugin during play.
        let generated = instance
            .config
            .game_dir
            .join("Data/F4SE/Plugins/Buffout4.log");
        std::fs::write(&generated, "crashlog").expect("simulate runtime write");

        purge(&instance, &NullSink).expect("purge");

        // Captured into overwrite/ in staging layout (the Data/ prefix is stripped)...
        let captured = instance.overwrite_dir().join("F4SE/Plugins/Buffout4.log");
        assert_eq!(
            std::fs::read_to_string(&captured).expect("captured file"),
            "crashlog"
        );
        // ...and gone from the game dir, which is left clean.
        assert!(
            !generated.exists(),
            "generated file moved out of the game dir"
        );
        assert!(
            !instance.config.game_dir.join("Data/F4SE").exists(),
            "emptied created dirs are removed"
        );
    }

    #[test]
    fn purge_leaves_generated_files_in_preexisting_dirs() {
        let (_tmp, instance) = temp_instance();
        // Data/ already exists, like a real (vanilla) game install.
        std::fs::create_dir_all(instance.config.game_dir.join("Data")).expect("vanilla Data");
        install_mod(&instance, "Tex", &[("Textures/x.dds", "pix")]);
        save_profile(&instance, "Default", &[("Tex", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // A tool writes a log directly into the pre-existing Data/ root.
        let loose = instance.config.game_dir.join("Data/loose.log");
        std::fs::write(&loose, "log").expect("write");

        purge(&instance, &NullSink).expect("purge");

        // We can't tell it from vanilla (Data/ pre-existed), so it's left in place.
        assert!(
            loose.exists(),
            "generated file in a pre-existing dir is left alone"
        );
        assert!(!instance.overwrite_dir().join("loose.log").exists());
    }

    #[test]
    fn captured_files_redeploy_into_the_game_dir() {
        let (_tmp, instance) = temp_instance();
        install_mod(
            &instance,
            "Buffout",
            &[("F4SE/Plugins/Buffout4.dll", "plugin")],
        );
        save_profile(&instance, "Default", &[("Buffout", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy 1");

        let generated = instance
            .config
            .game_dir
            .join("Data/F4SE/Plugins/Buffout4.log");
        std::fs::write(&generated, "crashlog").expect("write");
        purge(&instance, &NullSink).expect("purge 1");
        assert!(!generated.exists());

        // Re-deploy: the captured file comes back to the same game-dir location.
        deploy_profile(&instance, "Default", &NullSink).expect("deploy 2");
        assert_eq!(
            std::fs::read_to_string(&generated).expect("redeployed"),
            "crashlog"
        );
    }

    #[test]
    fn purge_captures_a_generated_file_in_a_mod_created_root_dir() {
        let (_tmp, instance) = temp_instance();
        // A Root mod that introduces enbseries/ in the game root.
        install_mod(&instance, "ENB", &[("Root/enbseries/enbseries.ini", "cfg")]);
        save_profile(&instance, "Default", &[("ENB", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // ENB writes a cache file into enbseries/ during play.
        let generated = instance.config.game_dir.join("enbseries/cache.bin");
        std::fs::write(&generated, "cache").expect("write");

        purge(&instance, &NullSink).expect("purge");

        // Captured under Root/ so it re-deploys to the game root next time.
        let captured = instance.overwrite_dir().join("Root/enbseries/cache.bin");
        assert_eq!(
            std::fs::read_to_string(&captured).expect("captured"),
            "cache"
        );
        assert!(!generated.exists());
    }

    #[test]
    fn higher_priority_mod_wins_conflicts() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "Winner", &[("shared.txt", "winner")]);
        install_mod(&instance, "Loser", &[("shared.txt", "loser")]);
        // Top of the list = highest priority.
        save_profile(&instance, "Default", &[("Winner", true), ("Loser", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        let path = deployed(&instance, "shared.txt");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "winner");
    }

    #[test]
    fn disabled_mods_are_not_deployed() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "On", "On.esp");
        install_plugin(&instance, "Off", "Off.esp");
        save_profile(&instance, "Default", &[("On", true), ("Off", false)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        assert!(deployed(&instance, "On.esp").exists());
        assert!(
            !deployed(&instance, "Off.esp").exists(),
            "disabled mod must not deploy"
        );
    }

    #[test]
    fn deploy_writes_plugins_txt_and_purge_restores_backup() {
        let (_tmp, instance) = temp_instance();
        let local = instance.config.local_dir.clone().expect("local dir set");
        std::fs::create_dir_all(&local).expect("mk local");
        // An existing Plugins.txt that purge must put back, byte for byte.
        std::fs::write(local.join("Plugins.txt"), b"*Original.esp\n").expect("seed");

        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        assert_eq!(
            deployment.plugins_txt_backup.as_deref(),
            Some(&b"*Original.esp\n"[..]),
            "the original Plugins.txt is captured in the deployment record"
        );

        // The real Plugins.txt now reflects the deployed, active plugin.
        let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
        assert_eq!(txt, "*Cool.esp\n");

        purge(&instance, &NullSink).expect("purge");

        // Purge restores the user's original file exactly.
        assert_eq!(
            std::fs::read(local.join("Plugins.txt")).expect("read"),
            b"*Original.esp\n"
        );
    }

    #[test]
    fn status_is_none_when_nothing_deployed() {
        let (_tmp, instance) = temp_instance();
        assert!(status(&instance).expect("status").is_none());
    }

    #[test]
    fn status_reports_the_live_deployment() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        let report = status(&instance).expect("status").expect("deployed");
        assert_eq!(report.deployment.profile, "Default");
        assert!(report.verified.is_ok(), "all deployed files present");
        assert!(
            report
                .deployment
                .record
                .entries
                .iter()
                .any(|e| e.relative == Utf8Path::new("Data").join("Cool.esp"))
        );
    }

    #[test]
    fn status_detects_a_missing_deployed_file() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // Simulate the game dir being tampered with: delete a deployed file.
        std::fs::remove_file(deployed(&instance, "Cool.esp")).expect("remove");

        let report = status(&instance).expect("status").expect("deployed");
        assert!(!report.verified.is_ok());
        assert!(
            report
                .verified
                .missing
                .iter()
                .any(|f| *f == Utf8Path::new("Data").join("Cool.esp"))
        );
    }

    #[test]
    fn an_interrupted_deployment_is_recovered_so_the_next_deploy_proceeds() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        // Deploy, then forge the journal back to InProgress to mimic a crash that
        // struck after the files landed but before the commit flip.
        deploy_profile(&instance, "Default", &NullSink).expect("first deploy");
        force_status(&instance, Status::InProgress);

        // A non-Committed journal must be reversed on the next entry; without
        // recovery this second deploy would be refused with AlreadyDeployed.
        deploy_profile(&instance, "Default", &NullSink).expect("recovery clears the way");

        assert!(deployed(&instance, "Cool.esp").exists());
        assert_eq!(
            Deployment::load(&instance).expect("load").status,
            Status::Committed
        );
    }

    #[test]
    fn a_held_lock_makes_deploy_busy() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        let _held = InstanceLock::acquire(&instance).expect("hold the lock");
        let err = deploy_profile(&instance, "Default", &NullSink)
            .expect_err("deploy must observe the held lock");
        assert!(matches!(err, ApplyError::Busy));
    }

    #[test]
    fn a_held_lock_makes_purge_busy() {
        let (_tmp, instance) = temp_instance();

        let _held = InstanceLock::acquire(&instance).expect("hold the lock");
        let err = purge(&instance, &NullSink).expect_err("purge must observe the held lock");
        assert!(matches!(err, ApplyError::Busy));
    }

    #[test]
    fn status_recovers_an_interrupted_deployment_and_reports_nothing_live() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        // Deploy, then forge the journal back to InProgress to mimic a crash that
        // struck after the files landed but before the commit flip.
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        force_status(&instance, Status::InProgress);

        // status must reverse the interrupted deployment, not report it as live.
        let live = status(&instance).expect("status");
        assert!(
            live.is_none(),
            "an interrupted deployment is reversed, not reported as live"
        );
        assert!(
            !Deployment::exists(&instance),
            "the journal is cleared once recovery resolves"
        );
        assert!(
            !deployed(&instance, "Cool.esp").exists(),
            "the interrupted deployment's files are removed"
        );
    }

    #[test]
    fn a_held_lock_makes_status_busy() {
        let (_tmp, instance) = temp_instance();

        let _held = InstanceLock::acquire(&instance).expect("hold the lock");
        let err = status(&instance).expect_err("status must observe the held lock");
        assert!(matches!(err, ApplyError::Busy));
    }

    #[test]
    fn purge_keeps_a_plugins_txt_edited_after_deployment() {
        let (_tmp, instance) = temp_instance();
        let local = instance.config.local_dir.clone().expect("local dir set");
        std::fs::create_dir_all(&local).expect("mk local");
        // The user's original list, which an untouched purge would restore.
        std::fs::write(local.join("Plugins.txt"), b"*Original.esp\n").expect("seed original");

        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // A tool or the user rewrites Plugins.txt after deployment.
        std::fs::write(local.join("Plugins.txt"), b"*Edited.esp\n").expect("edit after deploy");

        purge(&instance, &NullSink).expect("purge");

        // The post-deploy edit is preserved, not rolled back to the original.
        assert_eq!(
            std::fs::read(local.join("Plugins.txt")).expect("read"),
            b"*Edited.esp\n"
        );
        assert!(
            !Deployment::exists(&instance),
            "purge still completes and clears the journal on a Plugins.txt conflict"
        );
    }

    #[test]
    fn an_orphaned_backup_dir_refuses_deploy() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        // A leftover backup dir means a previous run never finished cleaning up.
        let backup_root = instance.config.game_dir.join(".overseer-backup");
        std::fs::create_dir_all(&backup_root).expect("plant orphan backup");

        let err = deploy_profile(&instance, "Default", &NullSink)
            .expect_err("deploy must refuse over an orphaned backup");
        assert!(matches!(err, ApplyError::OrphanedBackup { .. }));
    }

    #[test]
    fn a_reversal_that_cannot_finish_keeps_a_recovery_failed_journal() {
        let (_tmp, instance) = temp_instance();
        // A vanilla file gets backed up on deploy, so a backup dir lives alongside
        // the deployment until purge restores it.
        let data_file = deployed(&instance, "conflict.txt");
        std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
        std::fs::write(&data_file, "vanilla").expect("seed vanilla");
        install_mod(&instance, "Overwriter", &[("conflict.txt", "modded")]);
        save_profile(&instance, "Default", &[("Overwriter", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // Plant a stray file no entry will claim, so the sweep at the end of
        // reversal reports it as an unresolved residual backup.
        let backup_root = instance.config.game_dir.join(".overseer-backup");
        std::fs::write(backup_root.join("stray.bin"), b"junk").expect("plant stray");

        let err = purge(&instance, &NullSink).expect_err("purge cannot fully resolve");
        assert!(matches!(err, ApplyError::RecoveryFailed { .. }));

        // The journal survives, flagged so the next entry point knows to retry.
        assert!(Deployment::exists(&instance));
        assert_eq!(
            Deployment::load(&instance).expect("load").status,
            Status::RecoveryFailed
        );
    }

    // --- per-profile saves ---

    /// `Fallout4Custom.ini` under the instance's (temp) My Games dir.
    fn custom_ini(instance: &Instance) -> Utf8PathBuf {
        let stem = instance.config.game.ini_stem();
        instance
            .ini_dir()
            .expect("ini dir")
            .join(format!("{stem}Custom.ini"))
    }

    /// The live `SLocalSavePath` value, if any.
    fn save_path(instance: &Instance) -> Option<String> {
        let text = std::fs::read_to_string(custom_ini(instance)).ok()?;
        crate::ini::Ini::parse(&text)
            .get("General", "SLocalSavePath")
            .map(str::to_owned)
    }

    /// Save `Default` with a single enabled mod and per-profile saves switched on.
    fn deploy_profile_with_local_saves(instance: &Instance) {
        install_mod(instance, "CoolMod", &[("Textures/a.dds", "pix")]);
        save_profile(instance, "Default", &[("CoolMod", true)]);
        let mut profile = Profile::load(instance, "Default").expect("load");
        profile.local_saves = true;
        profile.save(instance).expect("save");
    }

    #[test]
    fn deploy_with_local_saves_redirects_saves_and_purge_restores() {
        let (_tmp, instance) = temp_instance();
        deploy_profile_with_local_saves(&instance);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        assert_eq!(
            save_path(&instance).as_deref(),
            Some("Saves\\Default\\"),
            "redirect written into Fallout4Custom.ini"
        );
        assert!(
            instance
                .ini_dir()
                .unwrap()
                .join("Saves")
                .join("Default")
                .is_dir(),
            "the profile's saves folder is pre-created"
        );

        // Nothing to put back: the user had no prior value.
        let journal = Deployment::load(&instance).expect("journal");
        assert_eq!(
            journal.save_redirect.expect("redirect journalled").original,
            None
        );

        purge(&instance, &NullSink).expect("purge");
        assert_eq!(save_path(&instance), None, "our redirect removed on purge");
    }

    #[test]
    fn deploy_without_local_saves_never_touches_saves() {
        let (_tmp, instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pix")]);
        // save_profile leaves local_saves off.
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        assert!(
            Deployment::load(&instance)
                .expect("journal")
                .save_redirect
                .is_none(),
            "no redirect journalled when the profile opts out"
        );
        assert!(
            !custom_ini(&instance).exists(),
            "the custom INI is never created"
        );
    }

    #[test]
    fn deploy_captures_the_users_prior_save_path_and_purge_restores_it() {
        let (_tmp, instance) = temp_instance();
        deploy_profile_with_local_saves(&instance);

        // The user already had a custom save path.
        let ini = custom_ini(&instance);
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        std::fs::write(&ini, "[General]\r\nSLocalSavePath=Saves\\Mine\\\r\n").unwrap();

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        assert_eq!(
            Deployment::load(&instance)
                .unwrap()
                .save_redirect
                .expect("journalled")
                .original
                .as_deref(),
            Some("Saves\\Mine\\"),
            "the prior value is captured"
        );
        assert_eq!(save_path(&instance).as_deref(), Some("Saves\\Default\\"));

        purge(&instance, &NullSink).expect("purge");
        assert_eq!(
            save_path(&instance).as_deref(),
            Some("Saves\\Mine\\"),
            "the user's original save path is restored on purge"
        );
    }

    #[test]
    fn purge_keeps_a_save_path_changed_after_deployment() {
        let (_tmp, instance) = temp_instance();
        deploy_profile_with_local_saves(&instance);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        // The user re-points their save path while deployed.
        std::fs::write(
            custom_ini(&instance),
            "[General]\r\nSLocalSavePath=Saves\\Manual\\\r\n",
        )
        .unwrap();

        purge(&instance, &NullSink).expect("purge");
        assert_eq!(
            save_path(&instance).as_deref(),
            Some("Saves\\Manual\\"),
            "a value the user changed after deploy is left alone"
        );
        assert!(
            !Deployment::exists(&instance),
            "purge still completes and clears the journal"
        );
    }
}
