//! Tests for the Confirm modal

use crate::app::input::test_helpers::key;
use crate::app::{App, Confirm, ConfirmAction, Modal};
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
