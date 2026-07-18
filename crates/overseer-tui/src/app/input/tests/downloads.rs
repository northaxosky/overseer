//! Tests for the downloads workspace's actions

use crate::app::input::test_helpers::key;
use crate::app::{
    App, ConflictsStatus, Focus, InstallJob, OperationKind, OperationState, Session, Workspace,
};
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
fn enter_on_a_download_opens_the_install_name_prompt() {
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
        Some(crate::app::Modal::Prompt(p)) => {
            assert!(
                matches!(p.kind, crate::app::PromptKind::InstallName { .. }),
                "an install-name prompt opens"
            );
            assert_eq!(p.input, "Mod", "the mod name defaults to the archive stem");
        }
        other => panic!("expected an install-name prompt, got {other:?}"),
    }
    assert!(
        !app.session.instance.mods_dir().join("Mod").exists(),
        "nothing is installed until the prompt is submitted"
    );
}

#[test]
fn worker_refuses_when_deployment_goes_live_after_submitting() {
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
    assert!(matches!(app.modal, Some(crate::app::Modal::Prompt(_))));
    overseer_core::apply::deploy_profile(&instance, "Default", &NullSink)
        .expect("deployment becomes live");

    app.handle_key(key(KeyCode::Enter));
    app.finish_operation_after_terminal();

    assert!(!instance.mods_dir().join("Blocked").exists());
    assert!(matches!(
        app.operation,
        OperationState::Completed(ref completed)
            if !completed.succeeded && completed.message.contains("purge it first")
    ));
}

#[test]
fn submitting_starts_the_install_worker_and_preserves_location() {
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
    app.conflicts.status = ConflictsStatus::Ready(
        overseer_core::deploy::ConflictSnapshot::from_entries(Vec::new()),
    );

    app.handle_key(key(KeyCode::Char('3')));
    app.finish_operation_after_terminal();
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Enter)); // opens the install-name prompt
    assert!(
        matches!(app.modal, Some(crate::app::Modal::Prompt(_))),
        "prompt is open"
    );
    app.handle_key(key(KeyCode::Enter)); // submits it

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::Install),
        "submitting starts the generic worker"
    );
    assert!(
        app.session.profile.item_row("CoolMod").is_none(),
        "the worker result is not applied synchronously"
    );
    assert!(
        app.message.is_none(),
        "the durable operation row needs no transient installing notice"
    );
    app.finish_operation_after_terminal();

    assert!(app.modal.is_none(), "the prompt closes after submitting");
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
    assert!(
        app.downloads
            .entries
            .iter()
            .any(|entry| entry.name == "CoolMod.zip"),
        "the archive is still listed"
    );
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
fn install_surfaces_the_fomod_refusal() {
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

    app.start_operation(InstallJob::new("Fancy.zip".to_owned(), "Fancy".to_owned()));
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
