//! The Doctor modal: an on-open diagnostics run shown as a read-only, dismiss-only
//! surface with a selectable findings list and a live detail pane.

use crate::app::{App, DoctorJob};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Keys for a Doctor modal: scroll the findings list or dismiss. It has no submit
    pub(super) fn handle_doctor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_modal_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_modal_list(-1),
            // Dismiss-only: Enter and any other key are inert
            _ => {}
        }
    }

    /// Start setup diagnostics without opening a modal
    pub(super) fn open_doctor(&mut self) {
        self.start_operation(DoctorJob);
    }
}

#[cfg(test)]
#[path = "tests/doctor.rs"]
mod tests;
