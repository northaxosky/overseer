//! Tests for deployment preparation and current-state detection

use super::*;
use crate::apply::{Deployment, deploy_profile};
use crate::deploy::{DeployerKind, NullSink};
use crate::instance::Profile;
use crate::test_support::{install_mod, install_plugin, save_profile, temp_instance};

fn deployed_fixture() -> (tempfile::TempDir, Instance) {
    let (temp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy fixture");
    (temp, instance)
}

#[test]
fn absent_and_matching_deployments_are_classified() {
    let (_temp, instance) = temp_instance();
    save_profile(&instance, "Default", &[]);
    assert_eq!(
        deployment_state(&instance, "Default").expect("absent state"),
        DeploymentState::Absent
    );

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(
        deployment_state(&instance, "Default").expect("current state"),
        DeploymentState::Current
    );
}

#[test]
fn plugin_reorder_is_stale_even_when_deployed_files_match() {
    let (_temp, instance) = temp_instance();
    install_plugin(&instance, "Plugins", "One.esp");
    install_plugin(&instance, "Plugins", "Two.esp");
    save_profile(&instance, "Default", &[("Plugins", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    std::fs::write(
        instance.profile_dir("Default").join("plugins.txt"),
        "*Two.esp\n*One.esp\n",
    )
    .expect("reorder profile plugins");

    assert_eq!(
        deployment_state(&instance, "Default").expect("state"),
        DeploymentState::Stale
    );
}

#[test]
fn profile_plan_target_backend_and_save_changes_are_stale() {
    let (_temp, mut instance) = deployed_fixture();

    let mut profile = Profile::load_existing(&instance, "Default").expect("load profile");
    profile.disable("CoolMod").expect("disable mod");
    profile.save_modlist(&instance).expect("save mod list");
    assert_eq!(
        deployment_state(&instance, "Default").expect("toggle state"),
        DeploymentState::Stale
    );
    profile.enable("CoolMod").expect("restore mod");
    profile.save_modlist(&instance).expect("save mod list");

    let mut deployment = Deployment::load(&instance).expect("load deployment");
    let target = deployment.record.target_root.clone();
    deployment.record.target_root = instance.root.join("other-game");
    deployment.save(&instance).expect("save changed target");
    assert_eq!(
        deployment_state(&instance, "Default").expect("target state"),
        DeploymentState::Stale
    );
    deployment.record.target_root = target;
    deployment.save(&instance).expect("restore target");

    instance.config.deployer = DeployerKind::ProjFs;
    assert_eq!(
        deployment_state(&instance, "Default").expect("backend state"),
        DeploymentState::Stale
    );
    instance.config.deployer = DeployerKind::HardLink;

    let mut profile = Profile::load_existing(&instance, "Default").expect("load profile");
    profile.local_saves = true;
    profile.save(&instance).expect("save local-saves change");
    assert_eq!(
        deployment_state(&instance, "Default").expect("save state"),
        DeploymentState::Stale
    );
}

#[test]
fn cardinality_changes_in_the_record_are_stale() {
    let (_temp, instance) = deployed_fixture();
    let mut deployment = Deployment::load(&instance).expect("load deployment");
    deployment
        .record
        .entries
        .push(deployment.record.entries[0].clone());
    deployment.save(&instance).expect("save duplicate entry");

    assert_eq!(
        deployment_state(&instance, "Default").expect("state"),
        DeploymentState::Stale
    );
}

#[test]
fn live_plugins_and_save_redirect_edits_are_stale() {
    let (_temp, instance) = temp_instance();
    install_plugin(&instance, "Plugins", "One.esp");
    save_profile(&instance, "Default", &[("Plugins", true)]);
    let mut profile = Profile::load_existing(&instance, "Default").expect("load profile");
    profile.local_saves = true;
    profile.save(&instance).expect("save profile");
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let local = instance.local_dir().expect("local dir");
    std::fs::write(local.join("Plugins.txt"), "*ExternallyEdited.esp\n")
        .expect("edit live plugins");
    assert_eq!(
        deployment_state(&instance, "Default").expect("plugin state"),
        DeploymentState::Stale
    );

    std::fs::write(local.join("Plugins.txt"), "*One.esp\n").expect("restore live plugins");
    let custom_ini = save_paths(&instance, "Default").expect("save paths").0;
    std::fs::write(
        custom_ini,
        "[General]\r\nSLocalSavePath=Saves\\External\\\r\n",
    )
    .expect("edit live save redirect");
    assert_eq!(
        deployment_state(&instance, "Default").expect("redirect state"),
        DeploymentState::Stale
    );
}
