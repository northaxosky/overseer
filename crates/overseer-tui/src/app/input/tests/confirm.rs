//! Tests for the Confirm modal

use crate::app::input::test_helpers::key;
use crate::app::{App, Confirm, ConfirmAction, Focus, ModPaneRow, Modal};
use overseer_core::instance::{ModKind, ModListEntry, Profile};
use ratatui::crossterm::event::KeyCode;

fn open_confirm(app: &mut App) {
    app.modal = Some(Modal::Confirm(Confirm {
        message: "Install Mod.zip?".to_owned(),
        action: ConfirmAction::InstallDownload(camino::Utf8PathBuf::from("Mod.zip")),
    }));
}

#[test]
fn n_cancels_the_confirm_without_acting() {
    let mut app = App::sample();
    open_confirm(&mut app);
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.modal.is_none(), "n closes the confirm");
    assert!(app.message.is_none(), "nothing happened");
}

#[test]
fn esc_cancels_the_confirm() {
    let mut app = App::sample();
    open_confirm(&mut app);
    app.handle_key(key(KeyCode::Esc));
    assert!(app.modal.is_none(), "Esc closes the confirm");
}

#[test]
fn enter_accepts_the_confirm_and_runs_its_action() {
    let mut app = App::sample();
    // A RemoveExe confirm whose target is absent: accepting runs the action and reports it
    app.modal = Some(Modal::Confirm(Confirm {
        message: "Remove launch target FO4Edit?".to_owned(),
        action: ConfirmAction::RemoveExe("FO4Edit".to_owned()),
    }));

    app.handle_key(key(KeyCode::Enter));

    assert!(app.modal.is_none(), "Enter accepts and closes the confirm");
    assert!(
        app.message.is_some(),
        "Enter runs the staged action, unlike n/Esc which do nothing"
    );
}

/// A three-row profile `[A, <separator>, B]` seeded on disk under a temp instance
fn app_with_separator() -> (tempfile::TempDir, App) {
    let (tmp, instance) = overseer_core::test_support::temp_instance();
    let mut app = App::sample();
    app.session.instance = instance;
    app.session.profile.mods = vec![
        ModListEntry {
            name: "A".to_owned(),
            enabled: true,
            kind: ModKind::Managed,
        },
        ModListEntry {
            name: "Zone_separator".to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        },
        ModListEntry {
            name: "B".to_owned(),
            enabled: true,
            kind: ModKind::Managed,
        },
    ];
    app.session
        .profile
        .save(&app.session.instance)
        .expect("seed the profile");
    app.mods.reset(&app.session.profile.mods);
    (tmp, app)
}

#[test]
fn x_on_a_separator_confirms_then_removes_and_persists() {
    let (_tmp, mut app) = app_with_separator();
    app.mods.select(Some(1)); // display 1 = the separator (model 1) under reversal

    app.handle_key(key(KeyCode::Char('x')));
    assert!(
        matches!(
            app.modal,
            Some(Modal::Confirm(Confirm {
                action: ConfirmAction::DeleteModSeparator { index: 1 },
                ..
            }))
        ),
        "x on a separator opens a delete confirm"
    );

    app.handle_key(key(KeyCode::Char('y')));

    assert!(app.modal.is_none(), "accepting closes the confirm");
    let names: Vec<&str> = app
        .session
        .profile
        .mods
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["A", "B"], "the divider is gone, members remain");
    let reloaded = Profile::load(&app.session.instance, "Default").expect("reload");
    let reloaded_names: Vec<&str> = reloaded.mods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(reloaded_names, ["A", "B"], "persisted to disk");
}

#[test]
fn del_on_a_separator_also_opens_the_delete_confirm() {
    let (_tmp, mut app) = app_with_separator();
    app.mods.select(Some(1));

    app.handle_key(key(KeyCode::Delete));

    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm {
            action: ConfirmAction::DeleteModSeparator { index: 1 },
            ..
        }))
    ));
}

#[test]
fn x_on_a_managed_mod_notes_and_does_not_delete() {
    let (_tmp, mut app) = app_with_separator();
    app.mods.select(Some(0)); // display 0 = managed "B" (model 2) under reversal

    app.handle_key(key(KeyCode::Char('x')));

    assert!(app.modal.is_none(), "no confirm opens for a managed mod");
    assert!(
        app.message.is_some(),
        "a note explains why nothing happened"
    );
    assert_eq!(app.session.profile.mods.len(), 3, "nothing was removed");
}

#[test]
fn x_when_the_workspace_is_focused_notes_and_does_not_delete() {
    let (_tmp, mut app) = app_with_separator();
    app.mods.select(Some(1)); // a separator is selected in the mods pane
    app.focus = Focus::Workspace;

    app.handle_key(key(KeyCode::Char('x')));

    assert!(
        app.modal.is_none(),
        "focus is on the workspace, so x is inert"
    );
    assert!(app.message.is_some());
    assert_eq!(app.session.profile.mods.len(), 3, "nothing was removed");
}

#[test]
fn a_failed_save_on_delete_rolls_the_separator_back_in_memory() {
    let (_tmp, mut app) = app_with_separator();
    app.mods.select(Some(1));
    app.handle_key(key(KeyCode::Char(' ')));
    // Block the profile dir by planting a file where `profiles/` must be a directory
    let profiles = app.session.instance.profiles_dir();
    std::fs::remove_dir_all(&profiles).ok();
    std::fs::write(&profiles, b"not a directory").expect("plant a blocking file");

    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Char('y')));

    assert!(app.modal.is_none(), "the confirm is consumed");
    let names: Vec<&str> = app
        .session
        .profile
        .mods
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["A", "Zone_separator", "B"],
        "a failed save re-inserts the separator at its index"
    );
    assert!(matches!(
        app.mods.project(&app.session.profile.mods)[1],
        ModPaneRow::Separator {
            collapsed: true,
            ..
        }
    ));
}
