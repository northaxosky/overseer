//! Tests for background mod replacement

use super::*;

use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::test_support::{install_plugin, save_profile, temp_instance, write_zip};

use crate::app::{App, OperationState, Session};

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
fn replace_job_keeps_name_and_reconciles_memory_without_persisting_profile_files() {
    let (_temp, mut app) = initialized_app();
    install_plugin(&app.session.instance, "ReplaceMe", "Old.esp");
    save_profile(&app.session.instance, "Default", &[("ReplaceMe", true)]);
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
        &app.session.instance.downloads_dir().join("Replacement.zip"),
        &[("New.esp", &overseer_core::test_support::tes4_bytes(0, &[]))],
    );

    app.start_operation(ReplaceJob::new(
        "ReplaceMe".to_owned(),
        "Replacement.zip".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(
        app.session
            .instance
            .mods_dir()
            .join("ReplaceMe")
            .join("New.esp")
            .exists()
    );
    assert!(
        !app.session
            .instance
            .mods_dir()
            .join("ReplaceMe")
            .join("Old.esp")
            .exists()
    );
    assert!(app.session.profile.position("ReplaceMe").is_some());
    assert!(
        app.session
            .order
            .plugins
            .iter()
            .all(|plugin| plugin.name != "Old.esp")
    );
    assert_eq!(
        std::fs::read(&modlist).expect("reread mod list"),
        modlist_before
    );
    assert_eq!(
        std::fs::read(&plugins).expect("reread plugin list"),
        plugins_before
    );
    assert_eq!(completed_message(&app), ("Replaced ReplaceMe", true));
}

#[test]
fn missing_archive_fails_without_changing_the_live_mod() {
    let (_temp, mut app) = initialized_app();
    install_plugin(&app.session.instance, "ReplaceMe", "Old.esp");
    save_profile(&app.session.instance, "Default", &[("ReplaceMe", true)]);
    app.session =
        Session::load(&app.session.instance.root, Some("Default")).expect("reload session");
    let archive = app.session.instance.downloads_dir().join("Gone.zip");
    write_zip(&archive, &[("New.esp", b"replacement")]);
    std::fs::remove_file(&archive).expect("remove chosen archive");

    app.start_operation(ReplaceJob::new(
        "ReplaceMe".to_owned(),
        "Gone.zip".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(
        app.session
            .instance
            .mods_dir()
            .join("ReplaceMe")
            .join("Old.esp")
            .exists()
    );
    assert!(
        !completed_message(&app).1,
        "the missing archive is reported as a worker failure"
    );
}

#[test]
fn live_deployment_refusal_uses_the_purge_first_message() {
    let (_temp, mut app) = initialized_app();
    install_plugin(&app.session.instance, "ReplaceMe", "Old.esp");
    save_profile(&app.session.instance, "Default", &[("ReplaceMe", true)]);
    app.session =
        Session::load(&app.session.instance.root, Some("Default")).expect("reload session");
    overseer_core::apply::deploy_profile(&app.session.instance, "Default", &NullSink)
        .expect("deploy fixture");

    app.start_operation(ReplaceJob::new(
        "ReplaceMe".to_owned(),
        "Gone.zip".to_owned(),
    ));
    app.finish_operation_after_terminal();

    assert!(
        app.session.status.is_some(),
        "session recovery refreshes status"
    );
    assert_eq!(
        completed_message(&app),
        (
            "Purge the live deployment before replacing ReplaceMe",
            false
        )
    );
}

#[test]
fn committed_residue_returns_without_loading_guarded_state() {
    let path = camino::Utf8PathBuf::from(r"state\pending-mod-operation");

    let output = ReplaceJob::new("ReplaceMe".to_owned(), "Replacement.zip".to_owned())
        .committed_with_residue(path.clone())
        .expect("committed success");

    assert!(matches!(
        output,
        OperationOutput::Replace {
            name,
            state: LifecycleState::CommittedWithResidue(residue),
        } if name == "ReplaceMe" && residue == path
    ));
}
