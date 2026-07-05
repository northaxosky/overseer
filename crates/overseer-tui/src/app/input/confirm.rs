//! The Confirm modal: a yes/no gate that runs a [`ConfirmAction`] on accept

use crate::app::{App, ConfirmAction, Modal};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Keys for the Confirm modal: `y`/Enter run the action, `n`/Esc cancel
    pub(super) fn handle_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => self.run_confirmed_action(),
            KeyCode::Char('n') | KeyCode::Esc => self.modal = None,
            _ => {}
        }
    }

    /// Take the confirm modal and run its action
    fn run_confirmed_action(&mut self) {
        let Some(Modal::Confirm(confirm)) = self.modal.take() else {
            return;
        };
        match confirm.action {
            ConfirmAction::InstallDownload(path) => self.install_download(&path),
            ConfirmAction::DeleteSave(path) => self.delete_selected_save(&path),
            ConfirmAction::RemoveExe(name) => self.remove_exe(&name),
        }
    }
}

#[cfg(test)]
#[path = "tests/confirm.rs"]
mod tests;
