//! The Info modal: a read-only, dismiss-only reference list (the help screen).

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::move_in_list;
use crate::app::{App, HELP_ENTRIES, Info, Modal, initial_selection};

impl App {
    /// Keys for an Info modal: scroll the list or dismiss. It has no submit.
    pub(super) fn handle_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_info(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_info(-1),
            // Dismiss-only: Enter does nothing.
            KeyCode::Enter => {}
            _ => {}
        }
    }

    /// Move the open Info modal's selection by `delta`, clamped to its entries
    fn move_in_info(&mut self, delta: isize) {
        if let Some(Modal::Info(info)) = self.modal.as_mut() {
            move_in_list(&mut info.state, info.entries.len(), delta);
        }
    }

    /// Open the keybinding reference as a dismiss-only [`Info`] modal
    pub(super) fn open_help(&mut self) {
        let entries = HELP_ENTRIES
            .iter()
            .map(|(keys, desc)| ((*keys).to_owned(), (*desc).to_owned()))
            .collect();
        self.modal = Some(Modal::Info(Info {
            title: "Help".to_owned(),
            entries,
            state: initial_selection(HELP_ENTRIES.len()),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::test_helpers::key;

    #[test]
    fn help_modal_opens_navigates_and_closes() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('?')));
        let selected = match &app.modal {
            Some(Modal::Info(info)) => {
                assert_eq!(info.title, "Help", "? opens the Help info modal");
                info.state.selected()
            }
            _ => panic!("? opens an Info modal"),
        };
        assert_eq!(selected, Some(0), "opens on the first entry");

        app.handle_key(key(KeyCode::Char('j')));
        match &app.modal {
            Some(Modal::Info(info)) => {
                assert_eq!(info.state.selected(), Some(1), "j scrolls within help");
            }
            _ => panic!("navigation does not close the modal"),
        }

        app.handle_key(key(KeyCode::Esc));
        assert!(app.modal.is_none(), "Esc closes the help modal");
    }

    #[test]
    fn enter_does_not_submit_the_info_modal() {
        let mut app = App::sample();
        app.handle_key(key(KeyCode::Char('?')));
        app.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(app.modal, Some(Modal::Info(_))),
            "Enter is inert: the Info modal is dismiss-only"
        );
    }
}
