//! Tests for the saves workspace's actions

use crate::app::input::test_helpers::key;
use crate::app::{App, Focus, Modal, Session, Workspace};
use overseer_core::instance::Instance;
use overseer_core::settings::{SavesSort, SavesSortKey, SortDir};
use overseer_core::test_support::{self, temp_instance};
use ratatui::crossterm::event::KeyCode;

/// A temp instance plus its `App`, with `count` saves seeded for `profile`
fn app_with_saves(profile: &str, count: u32) -> (tempfile::TempDir, App) {
    let (tmp, instance) = temp_instance();
    let dir = instance.saves_dir(profile).expect("saves dir");
    for n in 1..=count {
        test_support::write_fos(
            &dir.join(format!("Save{n}.fos")),
            n,
            "Nora",
            10 + n,
            "Sanctuary",
            "Day 1",
        );
    }
    let mut app = App::sample();
    app.session.instance = instance;
    (tmp, app)
}

#[test]
fn pressing_4_switches_to_saves_and_lists_them() {
    let (_tmp, mut app) = app_with_saves("Default", 1);

    app.handle_key(key(KeyCode::Char('4')));

    assert_eq!(app.workspace, Workspace::Saves, "4 switches workspace");
    assert_eq!(app.focus, Focus::Mods, "switching never moves focus");
    assert_eq!(app.saves.entries.len(), 1, "the profile's save is listed");
    assert_eq!(app.saves.list.selected(), Some(0), "first row selected");
}

#[test]
fn capital_x_on_a_save_opens_a_confirm_without_deleting() {
    let (_tmp, mut app) = app_with_saves("Default", 1);
    let save = app
        .session
        .instance
        .saves_dir("Default")
        .unwrap()
        .join("Save1.fos");

    app.handle_key(key(KeyCode::Char('4')));
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Char('X')));

    match &app.modal {
        Some(Modal::Confirm(c)) => assert!(
            c.message.contains("Delete Save1.fos"),
            "the confirm names the save"
        ),
        other => panic!("expected a confirm modal, got {other:?}"),
    }
    assert!(
        save.exists(),
        "nothing is deleted until the confirm is accepted"
    );
}

#[test]
fn confirming_deletes_the_save_relists_and_clamps_selection() {
    let (_tmp, mut app) = app_with_saves("Default", 2);
    app.handle_key(key(KeyCode::Char('4')));
    app.focus = Focus::Workspace;
    app.saves.list.select(Some(1)); // the second, newest-ordered row

    let doomed = app.saves.entries[1].path.clone();
    app.handle_key(key(KeyCode::Char('X')));
    app.handle_key(key(KeyCode::Char('y')));

    assert!(app.modal.is_none(), "the confirm closes after accepting");
    assert!(!doomed.exists(), "the save file is removed");
    assert_eq!(app.saves.entries.len(), 1, "the list is refreshed");
    assert_eq!(
        app.saves.list.selected(),
        Some(0),
        "the selection clamps into the shorter list"
    );
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Deleted")),
        "a success notice is shown"
    );
}

#[test]
fn deleting_removes_the_script_extender_co_save() {
    let (_tmp, mut app) = app_with_saves("Default", 1);
    let dir = app.session.instance.saves_dir("Default").unwrap();
    let co_save = dir.join("Save1.f4se");
    test_support::write(&co_save, "co-save");

    app.handle_key(key(KeyCode::Char('4')));
    app.focus = Focus::Workspace;
    app.handle_key(key(KeyCode::Char('X')));
    app.handle_key(key(KeyCode::Char('y')));

    assert!(
        !co_save.exists(),
        "the co-save is removed alongside the .fos"
    );
}

#[test]
fn toggling_local_saves_flips_and_persists() {
    let (_tmp, mut app) = app_with_saves("Default", 1);
    app.handle_key(key(KeyCode::Char('4')));
    app.focus = Focus::Workspace;

    let name = app.session.profile.name.clone();
    let before = app.session.profile.local_saves;
    app.handle_key(key(KeyCode::Char('L')));

    assert_eq!(app.session.profile.local_saves, !before, "L flips the flag");
    assert!(
        app.message
            .as_ref()
            .is_some_and(|n| n.text.contains("Local saves")),
        "a status notice is shown"
    );
    // Persisted: reloading from disk reflects the new value
    let reloaded = overseer_core::instance::Profile::load(&app.session.instance, &name).unwrap();
    assert_eq!(
        reloaded.local_saves, !before,
        "the toggle is written to disk"
    );
}

#[test]
fn local_saves_toggle_is_inert_off_the_saves_pane() {
    let (_tmp, mut app) = app_with_saves("Default", 1);
    // Still focused on Mods, not the Saves workspace
    let before = app.session.profile.local_saves;
    app.handle_key(key(KeyCode::Char('L')));
    assert_eq!(
        app.session.profile.local_saves, before,
        "inert unless the Saves pane is focused"
    );
}

#[test]
fn switching_profile_while_on_saves_relists_for_the_new_profile() {
    // A real on-disk instance so the profile switch (Session::load) works
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone()).expect("init");
    instance.create_profile("Default").expect("default profile");
    instance.create_profile("Other").expect("other profile");
    // Only the Other profile has saves on disk
    test_support::write_fos(
        &instance.saves_dir("Other").unwrap().join("Low.fos"),
        1,
        "Nate",
        5,
        "Vault 111",
        "Day 0",
    );
    test_support::write_fos(
        &instance.saves_dir("Other").unwrap().join("High.fos"),
        2,
        "Nate",
        10,
        "Concord",
        "Day 2",
    );

    let mut app = App::sample();
    app.session = Session::load(&instance.root, "Default").expect("session");
    app.settings.saves_sort = SavesSort {
        key: SavesSortKey::Level,
        dir: SortDir::Desc,
    };

    app.handle_key(key(KeyCode::Char('4')));
    assert!(app.saves.entries.is_empty(), "Default has no saves yet");

    // Open the profile picker and switch to Other (sorted second)
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.session.profile.name, "Other", "the profile switched");
    assert_eq!(app.workspace, Workspace::Saves, "still on the Saves pane");
    let names: Vec<&str> = app
        .saves
        .entries
        .iter()
        .map(|e| e.file_name.as_str())
        .collect();
    assert_eq!(
        names,
        ["High.fos", "Low.fos"],
        "the list refreshed and re-applied the saved sort"
    );
}
