//! Tests for the downloads workspace's actions

use crate::app::input::test_helpers::key;
use crate::app::{App, ConflictsStatus, Focus, Session, Workspace};
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
fn confirming_installs_the_mod_and_preserves_location() {
    // A real on-disk instance so the post-install reload (Session::load) works
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    instance.create_profile("Default").expect("profile");
    test_support::write_zip(
        &instance.downloads_dir().join("CoolMod.zip"),
        &[("Textures/a.dds", b"tex"), ("Meshes/b.nif", b"mesh")],
    );

    let mut app = App::sample();
    app.session = Session::load(&instance.root, "Default").expect("session");
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
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Installed CoolMod")),
        "a success notice is shown"
    );
}

#[test]
fn install_download_surfaces_the_fomod_refusal() {
    let (_tmp, instance) = temp_instance();
    let archive = instance.downloads_dir().join("Fancy.zip");
    test_support::write_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );
    let mut app = App::sample();
    app.session.instance = instance;

    app.install_download(&archive);

    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("FOMOD")),
        "the FOMOD refusal is surfaced"
    );
    assert!(
        !app.session.instance.mods_dir().join("Fancy").exists(),
        "a refused FOMOD installs nothing"
    );
}
