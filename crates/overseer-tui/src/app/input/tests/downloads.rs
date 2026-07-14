//! Tests for the downloads workspace's actions

use crate::app::input::test_helpers::key;
use crate::app::{App, ConflictsStatus, Focus, OperationKind, OperationState, Session, Workspace};
use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::test_support::{self, temp_instance};
use ratatui::crossterm::event::KeyCode;

#[test]
fn pressing_3_switches_to_downloads_and_lists_archives() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    test_support::write(&instance.downloads_dir().join("Mod.zip"), "fake");
    test_support::write(&instance.downloads_dir().join("notes.txt"), "x");
    let mut app = App::sample();
    app.session.instance = instance;
    *app.downloads.list.state_mut().offset_mut() = 3;

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();

    assert_eq!(app.workspace, Workspace::Downloads, "3 switches workspace");
    assert_eq!(app.focus, Focus::Mods, "switching never moves focus");
    let names: Vec<&str> = app
        .downloads
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["Mod.zip"], "only archives are listed");
    assert_eq!(app.downloads.list.index(), Some(0), "first row selected");
    assert_eq!(
        app.downloads.list.state_mut().offset(),
        3,
        "refresh-on-show preserves scroll state"
    );
}

#[test]
fn enter_on_an_installable_download_opens_a_confirm_without_installing() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    test_support::write(&instance.downloads_dir().join("Mod.zip"), "fake");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Enter));

    match &app.modal {
        Some(crate::app::Modal::Confirm(c)) => {
            assert!(
                c.message.contains("Install Mod.zip"),
                "confirm names the archive"
            );
            assert!(
                c.message.contains("mods/Mod"),
                "confirm names the destination"
            );
        }
        other => panic!("expected a confirm modal, got {other:?}"),
    }
    assert!(
        !app.session.instance.mods_dir().join("Mod").exists(),
        "nothing is installed until the confirm is accepted"
    );
}

#[test]
fn enter_on_an_installed_download_just_notes_it() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    test_support::write(&instance.downloads_dir().join("Mod.zip"), "fake");
    std::fs::create_dir_all(instance.mods_dir().join("Mod")).expect("seed installed mod");
    let mut app = App::sample();
    app.session.instance = instance;

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "an installed row opens no confirm");
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Already installed")),
        "the user is told it's already in"
    );
}

#[test]
fn worker_refuses_when_deployment_goes_live_after_confirmation() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    test_support::install_mod(&instance, "Seed", &[("Textures/seed.dds", "texture")]);
    test_support::save_profile(&instance, "Default", &[("Seed", true)]);
    test_support::write_zip(
        &instance.downloads_dir().join("Blocked.zip"),
        &[("Textures/a.dds", b"texture")],
    );
    let mut app = App::sample();
    app.session = Session::load(&instance.root, Some("Default")).expect("session");

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(crate::app::Modal::Confirm(_))));
    overseer_core::apply::deploy_profile(&instance, "Default", &NullSink)
        .expect("deployment becomes live");

    app.handle_key(key(KeyCode::Char('y')));
    app.finish_operation_after_terminal();

    assert!(!instance.mods_dir().join("Blocked").exists());
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if !completed.succeeded && completed.message.contains("purge it first")
    ));
}

#[test]
fn confirming_starts_the_install_worker_and_preserves_location() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    instance.create_profile("Default").expect("profile");
    test_support::write_zip(
        &instance.downloads_dir().join("CoolMod.zip"),
        &[("Textures/a.dds", b"tex"), ("Meshes/b.nif", b"mesh")],
    );

    let mut app = App::sample();
    app.session = Session::load(&instance.root, Some("Default")).expect("session");
    // A prior ready scan we expect the install to invalidate
    app.conflicts.status = ConflictsStatus::Ready(Vec::new());

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Enter)); // opens the confirm
    assert!(
        matches!(app.modal, Some(crate::app::Modal::Confirm(_))),
        "confirm is open"
    );
    app.handle_key(key(KeyCode::Char('y'))); // accepts it

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::Install),
        "confirmation starts the generic worker"
    );
    assert!(
        app.session.profile.position("CoolMod").is_none(),
        "the worker result is not applied synchronously"
    );
    assert!(
        app.message.is_none(),
        "the durable operation row needs no transient installing notice"
    );
    app.finish_operation_after_terminal();

    assert!(app.modal.is_none(), "the confirm closes after accepting");
    assert!(
        instance
            .mods_dir()
            .join("CoolMod")
            .join("Textures")
            .join("a.dds")
            .exists(),
        "the mod is staged under mods/"
    );
    assert_eq!(app.workspace, Workspace::Downloads, "location preserved");
    assert_eq!(app.focus, Focus::Workspace, "focus preserved");
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "the conflict scan is invalidated"
    );
    let row = app
        .downloads
        .entries
        .iter()
        .find(|e| e.name == "CoolMod.zip")
        .expect("the archive is still listed");
    assert!(row.installed, "the row now reads as installed");
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if completed.succeeded && completed.message == "Installed CoolMod"
    ));

    app.handle_key(key(KeyCode::Char('j')));
    assert!(
        matches!(app.operation, OperationState::Completed(_)),
        "ordinary input does not erase durable success"
    );
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(app.operation, OperationState::Idle),
        "Enter dismisses durable success"
    );
}

#[test]
fn install_download_surfaces_the_fomod_refusal() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    instance.create_profile("Default").expect("profile");
    let archive = instance.downloads_dir().join("Fancy.zip");
    test_support::write_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );
    let mut app = App::sample();
    app.session = Session::load(&instance.root, Some("Default")).expect("session");

    app.install_download(&archive);
    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::Install),
        "the refusal is produced by the worker"
    );
    app.finish_operation_after_terminal();

    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if !completed.succeeded
                && completed.message == "FOMOD installers aren't supported yet"
    ));
    assert!(
        !app.session.instance.mods_dir().join("Fancy").exists(),
        "a refused FOMOD installs nothing"
    );
}
