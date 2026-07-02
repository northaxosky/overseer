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
mod tests {
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
}
