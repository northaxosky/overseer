//! The Doctor modal: an on-open diagnostics run shown as a read-only, dismiss-only
//! surface with a selectable findings list and a live detail pane.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use overseer_diagnostics::diagnose;

use crate::app::{App, DoctorReport, Modal, initial_selection};

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

    /// Run setup diagnostics for the active session and show them as a Doctor modal
    pub(super) fn open_doctor(&mut self) {
        match diagnose(&self.session.instance, &self.session.profile.name) {
            Ok(report) => {
                let list = initial_selection(report.findings.len());
                self.modal = Some(Modal::Doctor(DoctorReport { report, list }));
            }
            Err(e) => self.fail(format!("Diagnostics failed: {e}")),
        }
    }
}

#[cfg(test)]
#[path = "tests/doctor.rs"]
mod tests;
