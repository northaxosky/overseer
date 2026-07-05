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
