//! Tests for background deployment and purge jobs

use super::*;

use overseer_core::instance::{Instance, Profile};
use overseer_core::test_support::{install_mod, save_profile, temp_instance};

use crate::app::{App, OperationState};

fn deployable_app() -> (tempfile::TempDir, App, Vec<camino::Utf8PathBuf>) {
    let (temp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    install_mod(
        &instance,
        "CoolMod",
        &[("Textures/a.dds", "pixels"), ("Meshes/a.nif", "geometry")],
    );
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let profile = Profile::load(&instance, "Default").expect("load profile");
    let deployed = ["Textures/a.dds", "Meshes/a.nif"]
        .into_iter()
        .map(|relative| instance.config.game_dir.join("Data").join(relative))
        .collect();

    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile = profile;
    app.session.status = None;
    (temp, app, deployed)
}

#[test]
fn deploy_job_reloads_the_instance_and_reports_exact_record_count() {
    let (_temp, mut app, deployed) = deployable_app();

    app.start_operation(DeployJob);
    assert_eq!(app.running_operation_kind(), Some(OperationKind::Deploy));
    app.finish_operation_after_terminal();

    assert!(deployed.iter().all(|path| path.exists()));
    assert!(app.session.status.is_some());
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if completed.succeeded && completed.message == "Deployed 2 files"
    ));
}

#[test]
fn purge_job_removes_the_live_deployment_and_returns_status_only() {
    let (_temp, mut app, deployed) = deployable_app();
    app.start_operation(DeployJob);
    app.finish_operation_after_terminal();

    app.start_operation(PurgeJob);
    assert_eq!(app.running_operation_kind(), Some(OperationKind::Purge));
    app.finish_operation_after_terminal();

    assert!(deployed.iter().all(|path| !path.exists()));
    assert!(app.session.status.is_none());
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if completed.succeeded && completed.message == "Purged the live deployment"
    ));
}

#[test]
fn failed_deploy_recovers_authoritative_status() {
    let (_temp, mut app, _deployed) = deployable_app();
    app.start_operation(DeployJob);
    app.finish_operation_after_terminal();
    app.session.status = None;

    app.start_operation(DeployJob);
    app.finish_operation_after_terminal();

    assert!(
        app.session.status.is_some(),
        "failure recovery restores status"
    );
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if !completed.succeeded && completed.message.starts_with("Deploy failed: ")
    ));
}
