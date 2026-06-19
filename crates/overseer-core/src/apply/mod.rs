//! Orchestration: turn a profile into a live on disk deployment, and reverse it

mod error;
mod state;

pub use error::ApplyError;
pub use state::Deployment;

use crate::deploy::{DeployPlan, ProgressSink, deployer_for};
use crate::instance::{Instance, Profile};

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

    let deployment = Deployment {
        profile: profile.name,
        manifest,
    };
    deployment.save(instance)?;
    Ok(deployment)
}

/// Reverses the instance's live deployment: remove the deployed files and clear the state
pub fn purge(instance: &Instance, progress: &dyn ProgressSink) -> Result<(), ApplyError> {
    let deployment = Deployment::load(instance)?;
    let deployer = deployer_for(deployment.manifest.deployer);
    deployer.undeploy(&deployment.manifest, progress)?;
    Deployment::remove(instance)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{ApplyError, Deployment, deploy_profile, purge};
    use crate::deploy::NullSink;
    use crate::instance::{Instance, ModListEntry, Profile};
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    /// A temp instance whose mods/ and game/ share one volume, so hardlinks succeed.
    fn temp_instance() -> (TempDir, Instance) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        let instance = Instance::new(root.join("instance"), root.join("game"));
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
        install_mod(&instance, "CoolMod", &[("a.esp", "x")]);
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
        install_mod(&instance, "CoolMod", &[("a.esp", "x")]);
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
        install_mod(&instance, "On", &[("on.esp", "1")]);
        install_mod(&instance, "Off", &[("off.esp", "0")]);
        save_profile(&instance, "Default", &[("On", true), ("Off", false)]);

        deploy_profile(&instance, "Default", &NullSink).expect("deploy");

        assert!(deployed(&instance, "on.esp").exists());
        assert!(
            !deployed(&instance, "off.esp").exists(),
            "disabled mod must not deploy"
        );
    }
}
