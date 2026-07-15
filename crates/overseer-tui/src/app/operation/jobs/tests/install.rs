//! Tests for background archive installation

use super::*;

use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::test_support::{install_mod, save_profile, temp_instance, write, write_zip};

use crate::app::{App, OperationKind, OperationState, Session};

fn initialized_app() -> (tempfile::TempDir, App) {
    let (temp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    instance.create_profile("Default").expect("create profile");
    let mut app = App::sample();
    app.session = Session::load(&instance.root, Some("Default")).expect("load session");
    app.mods.reset(&app.session.profile.mods);
    app.plugins
        .reset(&app.session.order.plugins, &app.session.plugin_separators);
    (temp, app)
}

fn completed_message(app: &App) -> (&str, bool) {
    let OperationState::Completed(completed) = &app.operation else {
        panic!("operation completed")
    };
    (&completed.message, completed.succeeded)
}

#[test]
fn install_job_reloads_the_session_and_keeps_the_download_listed() {
    let (_temp, mut app) = initialized_app();
    let archive = app.session.instance.downloads_dir().join("CoolMod.zip");
    write_zip(&archive, &[("Textures/a.dds", b"texture")]);

    app.start_operation(InstallJob::new(
        "CoolMod.zip".to_owned(),
        "CoolMod".to_owned(),
    ));

    assert_eq!(app.running_operation_kind(), Some(OperationKind::Install));
    assert!(
        app.session.profile.position("CoolMod").is_none(),
        "worker output is not accepted synchronously"
    );
    assert!(
        app.operation
            .running()
            .is_some_and(|running| running.view.progress.is_none()),
        "archive extraction stays indeterminate"
    );
    app.finish_operation_after_terminal();

    let installed = app
        .session
        .profile
        .mods
        .iter()
        .find(|entry| entry.name == "CoolMod")
        .expect("installed profile row");
    assert!(!installed.enabled);
    assert_eq!(
        std::fs::read_to_string(
            app.session
                .instance
                .profile_dir("Default")
                .join("modlist.txt")
        )
        .expect("read modlist"),
        ""
    );
    assert!(
        app.downloads
            .entries
            .iter()
            .any(|entry| entry.name == "CoolMod.zip"),
        "download remains listed"
    );
    assert_eq!(completed_message(&app), ("Installed CoolMod", true));
}

#[test]
fn live_deployment_guard_refuses_before_archive_extraction() {
    let (_temp, mut app) = initialized_app();
    install_mod(
        &app.session.instance,
        "Seed",
        &[("Textures/seed.dds", "texture")],
    );
    save_profile(&app.session.instance, "Default", &[("Seed", true)]);
    app.session =
        Session::load(&app.session.instance.root, Some("Default")).expect("reload session");
    overseer_core::apply::deploy_profile(&app.session.instance, "Default", &NullSink)
        .expect("deploy fixture");
    let archive = app.session.instance.downloads_dir().join("Blocked.zip");
    write_zip(&archive, &[("Textures/a.dds", b"texture")]);

    app.start_operation(InstallJob::new(
        "Blocked.zip".to_owned(),
        "Blocked".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(!app.session.instance.mods_dir().join("Blocked").exists());
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.contains("deployment state exists"));
    assert!(message.contains("purge it first"));
}

#[test]
fn fomod_refusal_keeps_the_explicit_user_message() {
    let (_temp, mut app) = initialized_app();
    let archive = app.session.instance.downloads_dir().join("Scripted.zip");
    write_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"texture"),
        ],
    );

    app.start_operation(InstallJob::new(
        "Scripted.zip".to_owned(),
        "Scripted".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert_eq!(
        completed_message(&app),
        ("FOMOD installers aren't supported yet", false)
    );
    assert!(!app.session.instance.mods_dir().join("Scripted").exists());
}

#[test]
fn failed_install_recovers_the_authoritative_session() {
    let (_temp, mut app) = initialized_app();
    install_mod(
        &app.session.instance,
        "ExternallyAdded",
        &[("Textures/external.dds", "texture")],
    );
    let archive = app.session.instance.downloads_dir().join("Broken.zip");
    write(&archive, "not a zip archive");

    app.start_operation(InstallJob::new(
        "Broken.zip".to_owned(),
        "Broken".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(app.session.profile.position("ExternallyAdded").is_some());
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.starts_with("Install failed: "));
    assert!(!message.contains("session recovery failed"));
}

#[test]
fn newly_installed_invalid_plugin_stays_safe_while_disabled() {
    let (_temp, mut app) = initialized_app();
    let archive = app.session.instance.downloads_dir().join("BadPlugin.zip");
    write_zip(&archive, &[("BadPlugin.esp", b"not a TES4 plugin")]);

    app.start_operation(InstallJob::new(
        "BadPlugin.zip".to_owned(),
        "BadPlugin".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(app.session.instance.mods_dir().join("BadPlugin").exists());
    let entry = app
        .session
        .profile
        .mods
        .iter()
        .find(|entry| entry.name == "BadPlugin")
        .expect("installed profile row");
    assert!(!entry.enabled);
    assert_eq!(completed_message(&app), ("Installed BadPlugin", true));
}

#[test]
fn committed_residue_builds_success_without_guarded_refresh_data() {
    let path = camino::Utf8PathBuf::from(r"state\pending-mod-operation");

    let output = InstallJob::new("CoolMod.zip".to_owned(), "CoolMod".to_owned())
        .committed_with_residue(path.clone())
        .expect("committed success");

    assert!(matches!(
        output,
        OperationOutput::Install {
            name,
            state: InstallState::CommittedWithResidue(residue),
        } if name == "CoolMod" && residue == path
    ));
}
