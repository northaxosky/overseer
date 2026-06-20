//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod lock;
mod state;

pub use error::ApplyError;
pub use lock::InstanceLock;
pub use state::{Deployment, Status};

use crate::deploy::{DeployPlan, DeployRecord, ProgressSink, VerifyReport, deployer_for};
use crate::instance::{Instance, Profile};
use crate::plugins::{self, PluginLoadOrder};
use camino::{Utf8Path, Utf8PathBuf};

/// Deploy a profile's enabled mods into the instance's game `Data/` dir
pub fn deploy_profile(
    instance: &Instance,
    profile_name: &str,
    progress: &dyn ProgressSink,
) -> Result<Deployment, ApplyError> {
    tracing::info!(profile = profile_name, "deploy_profile: start");
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
    let sources = profile.deploy_sources(instance);

    let data_dir = instance.game_dir().join("Data");
    let plan = DeployPlan::from_mods(&data_dir, &sources)?;

    let deployer = deployer_for(instance.config.deployer);
    deployer.check_supported(&plan)?;

    let backup_root = instance.game_dir().join(".overseer-backup");
    guard_no_orphaned_backup(&backup_root)?;
    let record = DeployRecord::from_plan(&plan, backup_root, instance.config.deployer)?;

    let local_dir = resolve_local_dir(instance)?;
    std::fs::create_dir_all(&local_dir).map_err(|e| error::io_err(&local_dir, e))?;

    // Profile bookkeeping doesnt touch anything so its safe
    let order = prepare_load_order(instance, &profile)?;

    // Capture the users original Plugins.txt
    let plugins_txt_backup = plugins::read_plugins_txt(&local_dir)?;

    // First write: journal as InProgress
    let mut deployment = Deployment {
        status: Status::InProgress,
        profile: profile.name,
        record,
        plugins_txt_backup,
    };
    deployment.save(instance)?;

    if let Err(e) = deployer.deploy(&deployment.record, progress) {
        tracing::warn!(error = %e, "deploy_profile: deploy failed; rolling back");
        let _ = reverse_and_finalize(instance, deployment, progress);
        return Err(e.into());
    }

    if let Err(e) = plugins::write_active_plugins(instance.game_dir(), &local_dir, &order.plugins) {
        tracing::warn!(error = %e, "deploy_profile: writing Plugins.txt failed; rolling back");
        let _ = reverse_and_finalize(instance, deployment, progress);
        return Err(e.into());
    }

    // Second Write: InProgress -> Committed flip
    deployment.status = Status::Committed;
    deployment.save(instance)?;
    tracing::info!(
        profile = %deployment.profile,
        files = deployment.record.entries.len(),
        "deploy_profile: committed"
    );
    Ok(deployment)
}

/// Reverses the instance's live deployment: remove the deployed files and clear the state
pub fn purge(instance: &Instance, progress: &dyn ProgressSink) -> Result<(), ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_if_needed(instance, progress)?;

    let deployment = Deployment::load(instance)?;
    tracing::info!(profile = %deployment.profile, "purge: reversing live deployment");
    reverse_and_finalize(instance, deployment, progress)
}

/// A snapshot of an instance's live deployment
pub struct DeploymentStatus {
    pub deployment: Deployment,
    pub verified: VerifyReport,
}

/// Report the instance's live deployment, or `None` if nothing is deployed
pub fn status(instance: &Instance) -> Result<Option<DeploymentStatus>, ApplyError> {
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
                "recovery: interrupted deployment found; reversing"
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

    let local_dir = resolve_local_dir(instance)?;
    let plugins_restored =
        plugins::restore_plugins_txt(&local_dir, deployment.plugins_txt_backup.as_deref());

    if report.is_fully_resolved() && plugins_restored.is_ok() {
        tracing::info!("reversal: fully resolved; clearing journal");
        Deployment::remove(instance)
    } else {
        tracing::warn!("reversal: incomplete; persisting RecoveryFailed");
        let path = Deployment::path(instance);
        let failed = Deployment {
            status: Status::RecoveryFailed,
            ..deployment
        };
        failed.save(instance)?;
        Err(plugins_restored
            .err()
            .map(ApplyError::from)
            .unwrap_or(ApplyError::RecoveryFailed { path }))
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

/// The dir holding the game's real `Plugins.txt`
fn resolve_local_dir(instance: &Instance) -> Result<Utf8PathBuf, ApplyError> {
    if let Some(dir) = &instance.config.local_dir {
        return Ok(dir.clone());
    }

    let base = std::env::var("LOCALAPPDATA").map_err(|_| ApplyError::NoLocalAppData)?;
    Ok(Utf8PathBuf::from(base).join("Fallout4"))
}

// Dont start a deploy when the backup dir survives from a previous run
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
    use super::{ApplyError, Deployment, InstanceLock, Status, deploy_profile, purge, status};
    use crate::deploy::NullSink;
    use crate::instance::{Instance, ModListEntry, Profile};
    use crate::plugins::test_support::write_plugin;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    /// A temp instance whose mods/ and game/ share one volume, so hardlinks succeed.
    fn temp_instance() -> (TempDir, Instance) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        let mut instance = Instance::new(root.join("instance"), root.join("game"));
        // Point Plugins.txt at a temp dir so tests never touch the real %LOCALAPPDATA%.
        instance.config.local_dir = Some(root.join("local"));
        (dir, instance)
    }

    /// Create a mod folder under mods/ with the given relative files and contents.
    fn install_mod(instance: &Instance, name: &str, files: &[(&str, &str)]) {
        for (rel, contents) in files {
            let path = instance.mods_dir().join(name).join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, contents).expect("write file");
        }
    }

    /// Install a mod whose staging dir holds a single valid Fallout 4 plugin.
    fn install_plugin(instance: &Instance, mod_name: &str, plugin: &str) {
        write_plugin(&instance.mods_dir().join(mod_name), plugin, 0, &[]);
    }

    /// Save a profile (highest priority first) so `deploy_profile` can load it from disk.
    fn save_profile(instance: &Instance, name: &str, mods: &[(&str, bool)]) {
        let profile = Profile {
            name: name.to_owned(),
            mods: mods
                .iter()
                .map(|(n, enabled)| ModListEntry {
                    name: (*n).to_owned(),
                    enabled: *enabled,
                    foreign: false,
                })
                .collect(),
        };
        profile.save(instance).expect("save profile");
    }

    /// Absolute path of a file as it would land under the game's Data/ directory.
    fn deployed(instance: &Instance, rel: &str) -> Utf8PathBuf {
        instance.game_dir().join("Data").join(rel)
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
                .any(|e| e.relative.as_str() == "Cool.esp")
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
                .any(|f| f.as_str() == "Cool.esp")
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
    fn an_orphaned_backup_dir_refuses_deploy() {
        let (_tmp, instance) = temp_instance();
        install_plugin(&instance, "CoolMod", "Cool.esp");
        save_profile(&instance, "Default", &[("CoolMod", true)]);

        // A leftover backup dir means a previous run never finished cleaning up.
        let backup_root = instance.game_dir().join(".overseer-backup");
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
        let backup_root = instance.game_dir().join(".overseer-backup");
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
}
