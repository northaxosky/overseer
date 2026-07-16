//! Tests for background mod removal

use super::*;

use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::test_support::{install_plugin, save_profile, temp_instance, write_zip};

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
fn remove_job_reconciles_memory_without_persisting_profile_files() {
    let (_temp, mut app) = initialized_app();
    install_plugin(&app.session.instance, "Removable", "Remove.esp");
    save_profile(&app.session.instance, "Default", &[("Removable", true)]);
    app.session =
        Session::load(&app.session.instance.root, Some("Default")).expect("reload session");
    app.session
        .order
        .save(&app.session.instance)
        .expect("seed plugin list");
    let modlist = app
        .session
        .instance
        .profile_dir("Default")
        .join("modlist.txt");
    let plugins = app
        .session
        .instance
        .profile_dir("Default")
        .join("plugins.txt");
    let modlist_before = std::fs::read(&modlist).expect("read mod list");
    let plugins_before = std::fs::read(&plugins).expect("read plugin list");
    write_zip(
        &app.session.instance.downloads_dir().join("Archive.zip"),
        &[("Textures/a.dds", b"texture")],
    );

    app.start_operation(RemoveJob::new("Removable".to_owned()));
    assert_eq!(app.running_operation_kind(), Some(OperationKind::Remove));
    app.finish_operation_after_terminal();

    assert!(!app.session.instance.mods_dir().join("Removable").exists());
    assert!(app.session.profile.position("Removable").is_none());
    assert!(
        app.session
            .order
            .plugins
            .iter()
            .all(|plugin| plugin.name != "Remove.esp")
    );
    assert!(
        app.downloads
            .entries
            .iter()
            .any(|entry| entry.name == "Archive.zip")
    );
    assert_eq!(
        std::fs::read(&modlist).expect("reread mod list"),
        modlist_before
    );
    assert_eq!(
        std::fs::read(&plugins).expect("reread plugin list"),
        plugins_before
    );
    assert_eq!(completed_message(&app), ("Removed Removable", true));
}

#[test]
fn live_deployment_refusal_recovers_session_state() {
    let (_temp, mut app) = initialized_app();
    install_plugin(&app.session.instance, "Removable", "Remove.esp");
    save_profile(&app.session.instance, "Default", &[("Removable", true)]);
    app.session =
        Session::load(&app.session.instance.root, Some("Default")).expect("reload session");
    overseer_core::apply::deploy_profile(&app.session.instance, "Default", &NullSink)
        .expect("deploy fixture");
    assert!(app.session.status.is_none(), "keep stale UI status");

    app.start_operation(RemoveJob::new("Removable".to_owned()));
    app.finish_operation_after_terminal();

    assert!(
        app.session.status.is_some(),
        "session recovery refreshes status"
    );
    assert!(app.session.instance.mods_dir().join("Removable").exists());
    assert_eq!(
        completed_message(&app),
        ("Purge the live deployment before removing Removable", false)
    );
}

#[test]
fn committed_residue_returns_without_loading_guarded_state() {
    let path = camino::Utf8PathBuf::from(r"state\pending-mod-operation");

    let output = RemoveJob::new("Removable".to_owned())
        .committed_with_residue(path.clone())
        .expect("committed success");

    assert!(matches!(
        output,
        OperationOutput::Remove {
            name,
            state: LifecycleState::CommittedWithResidue(residue),
        } if name == "Removable" && residue == path
    ));
}
