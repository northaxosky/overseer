//! The Info modal: a read-only, dismiss-only reference list (the help screen).

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, HELP_ENTRIES, Info, ListCursor, Modal};

impl App {
    /// Keys for an Info modal: scroll the list or dismiss. It has no submit
    pub(super) fn handle_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_modal_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_modal_list(-1),
            // Dismiss-only: Enter and any other key are inert
            _ => {}
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
            state: ListCursor::first(HELP_ENTRIES.len()),
        }));
    }
}

#[cfg(test)]
#[path = "tests/info.rs"]
mod tests;
