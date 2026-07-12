//! Tests for background archive installation

use super::*;

use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::test_support::{install_mod, save_profile, temp_instance, write, write_zip};

use crate::app::{App, ConflictsStatus, OperationKind, OperationState, Session};
use crate::test_support::download_entry;

fn initialized_app() -> (tempfile::TempDir, App) {
    let (temp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    instance.create_profile("Default").expect("create profile");
    let mut app = App::sample();
    app.session = Session::load(&instance.root, "Default").expect("load session");
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
fn install_job_reloads_the_session_and_marks_the_download_installed() {
    let (_temp, mut app) = initialized_app();
    let archive = app.session.instance.downloads_dir().join("CoolMod.zip");
    write_zip(&archive, &[("Textures/a.dds", b"texture")]);

    app.start_operation(InstallJob::new(archive, "CoolMod".to_owned()));

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

    assert!(app.session.profile.position("CoolMod").is_some());
    let entry = app
        .downloads
        .entries
        .iter()
        .find(|entry| entry.name == "CoolMod.zip")
        .expect("download remains listed");
    assert!(entry.installed);
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
    app.session = Session::load(&app.session.instance.root, "Default").expect("reload session");
    overseer_core::apply::deploy_profile(&app.session.instance, "Default", &NullSink)
        .expect("deploy fixture");
    let archive = app.session.instance.downloads_dir().join("Blocked.zip");
    write_zip(&archive, &[("Textures/a.dds", b"texture")]);

    app.start_operation(InstallJob::new(archive, "Blocked".to_owned()));
    app.finish_operation_after_terminal();

    assert!(!app.session.instance.mods_dir().join("Blocked").exists());
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.contains("deployment is live"));
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

    app.start_operation(InstallJob::new(archive, "Scripted".to_owned()));
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

    app.start_operation(InstallJob::new(archive, "Broken".to_owned()));
    app.finish_operation_after_terminal();

    assert!(app.session.profile.position("ExternallyAdded").is_some());
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.starts_with("Install failed: "));
    assert!(!message.contains("session recovery failed"));
}

#[test]
fn reload_failure_reports_install_success_and_secondary_recovery_error() {
    let (_temp, mut app) = initialized_app();
    let archive = app.session.instance.downloads_dir().join("BadPlugin.zip");
    write_zip(&archive, &[("BadPlugin.esp", b"not a TES4 plugin")]);

    app.start_operation(InstallJob::new(archive, "BadPlugin".to_owned()));
    app.finish_operation_after_terminal();

    assert!(app.session.instance.mods_dir().join("BadPlugin").exists());
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.starts_with("Installed BadPlugin, but reloading failed: "));
    assert!(message.contains("session recovery failed: "));
}

#[test]
fn downloads_refresh_failure_applies_the_exact_loaded_session() {
    let (_temp, mut app) = initialized_app();
    let root = app
        .session
        .instance
        .root
        .parent()
        .expect("instance parent")
        .to_owned();
    let archive = root.join("RefreshFailure.zip");
    write_zip(&archive, &[("Textures/a.dds", b"texture")]);
    let downloads_dir = app.session.instance.downloads_dir();
    std::fs::remove_dir(&downloads_dir).expect("remove downloads directory");
    write(&downloads_dir, "blocks directory listing");
    app.downloads.entries = vec![download_entry("Cached.zip", 1, 1, false)];
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());

    app.start_operation(InstallJob::new(archive, "RefreshFailure".to_owned()));
    app.finish_operation_after_terminal();

    assert!(app.session.profile.position("RefreshFailure").is_some());
    assert_eq!(app.downloads.entries[0].name, "Cached.zip");
    assert!(matches!(app.conflicts.status, ConflictsStatus::Stale));
    let (message, succeeded) = completed_message(&app);
    assert!(!succeeded);
    assert!(message.starts_with("Installed RefreshFailure, but downloads refresh failed: "));
    assert!(!message.contains("session recovery failed"));
}
