//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod state;

pub use error::ApplyError;
pub use state::Deployment;

use crate::deploy::{DeployPlan, ProgressSink, VerifyReport, deployer_for};
use crate::instance::{Instance, Profile};
use crate::plugins::{self, PluginLoadOrder};
use camino::{Utf8Path, Utf8PathBuf};

/// Deploy a profile's enabled mods into the instance's game `Data/` directory
pub fn deploy_profile(
    instance: &Instance,
    profile_name: &str,
    progress: &dyn ProgressSink,
) -> Result<Deployment, ApplyError> {
    if Deployment::exists(instance) {
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
    let manifest = deployer.deploy(&plan, progress)?;

    let local_dir = resolve_local_dir(instance)?;
    std::fs::create_dir_all(&local_dir).map_err(|e| error::io_err(&local_dir, e))?;
    let plugins_txt_backup = plugins::read_plugins_txt(&local_dir)?;

    if let Err(e) = write_game_plugins(instance, &profile, &local_dir) {
        let _ = deployer.undeploy(&manifest, progress);
        let _ = plugins::restore_plugins_txt(&local_dir, plugins_txt_backup.as_deref());
        return Err(e);
    }

    let deployment = Deployment {
        profile: profile.name,
        manifest,
        plugins_txt_backup,
    };
    deployment.save(instance)?;
    Ok(deployment)
}

/// Reverses the instance's live deployment: remove the deployed files and clear the state
pub fn purge(instance: &Instance, progress: &dyn ProgressSink) -> Result<(), ApplyError> {
    let deployment = Deployment::load(instance)?;
    let deployer = deployer_for(deployment.manifest.deployer);
    deployer.undeploy(&deployment.manifest, progress)?;

    let local_dir = resolve_local_dir(instance)?;
    plugins::restore_plugins_txt(&local_dir, deployment.plugins_txt_backup.as_deref())?;

    Deployment::remove(instance)
}

/// A snapshot of an instance's live deployment & a check
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
    let verified = deployer_for(deployment.manifest.deployer).verify(&deployment.manifest);
    Ok(Some(DeploymentStatus {
        deployment,
        verified,
    }))
}

/// Discover the profile's plugins, reconcile the saved `plugins.txt` and write the real `Plugins.txt`
fn write_game_plugins(
    instance: &Instance,
    profile: &Profile,
    local_dir: &Utf8Path,
) -> Result<(), ApplyError> {
    let discovered = plugins::discover_plugins(instance, profile)?;
    let mut order = PluginLoadOrder::load(instance, &profile.name)?;

    order.reconcile(&discovered);
    order.save(instance)?;

    plugins::write_active_plugins(instance.game_dir(), local_dir, &order.plugins)?;
    Ok(())
}

/// The dir holding the game's real `Plugins.txt`
fn resolve_local_dir(instance: &Instance) -> Result<Utf8PathBuf, ApplyError> {
    if let Some(dir) = &instance.config.local_dir {
        return Ok(dir.clone());
    }
    let base = std::env::var("LOCALAPPDATA").map_err(|_| ApplyError::NoLocalAppData)?;
    Ok(Utf8PathBuf::from(base).join("Fallout4"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{ApplyError, Deployment, deploy_profile, purge, status};
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
                .manifest
                .files
                .iter()
                .any(|f| f.as_str() == "Cool.esp")
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
}
