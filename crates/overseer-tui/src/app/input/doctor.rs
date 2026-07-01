//! The Doctor modal: an on-open diagnostics run shown as a read-only, dismiss-only
//! surface with a selectable findings list and a live detail pane.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use overseer_diagnostics::diagnose;

use super::move_in_list;
use crate::app::{App, DoctorReport, Modal, initial_selection};

impl App {
    /// Keys for a Doctor modal: scroll the findings list or dismiss. It has no submit.
    pub(super) fn handle_doctor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => self.modal = None,
            KeyCode::Down | KeyCode::Char('j') => self.move_in_doctor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_in_doctor(-1),
            // Dismiss-only: Enter does nothing.
            KeyCode::Enter => {}
            _ => {}
        }
    }

    /// Move the open Doctor modal's selection by `delta`, clamped to its findings
    fn move_in_doctor(&mut self, delta: isize) {
        if let Some(Modal::Doctor(doctor)) = self.modal.as_mut() {
            move_in_list(&mut doctor.list, doctor.report.findings.len(), delta);
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
mod tests {
    use super::*;
    use crate::app::input::test_helpers::{key, modal_selection};
    use overseer_diagnostics::{Finding, Report, Severity};

    /// Seed an open Doctor modal with `titles` findings, selecting the first.
    fn open_with(app: &mut App, titles: &[&str]) {
        let report = Report::new(
            titles
                .iter()
                .map(|t| Finding {
                    check: "c",
                    severity: Severity::Warning,
                    title: (*t).to_owned(),
                    detail: None,
                })
                .collect(),
        );
        let list = initial_selection(report.findings.len());
        app.modal = Some(Modal::Doctor(DoctorReport { report, list }));
    }

    #[test]
    fn d_opens_a_doctor_modal_that_ran_diagnostics() {
        use overseer_core::test_support::temp_instance;
        let (_tmp, instance) = temp_instance();
        let mut app = App::sample();
        app.session.instance = instance;

        app.handle_key(key(KeyCode::Char('d')));

        match &app.modal {
            Some(Modal::Doctor(doctor)) => {
                assert!(
                    !doctor.report.findings.is_empty(),
                    "opening runs diagnostics and populates findings"
                );
                assert_eq!(
                    doctor.list.selected(),
                    Some(0),
                    "opens on the first finding"
                );
            }
            _ => panic!("d opens a Doctor modal"),
        }
    }

    #[test]
    fn jk_move_the_doctor_findings_selection() {
        let mut app = App::sample();
        open_with(&mut app, &["a", "b"]);
        assert_eq!(modal_selection(&app), Some(0), "opens on the first finding");
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(modal_selection(&app), Some(1), "j moves down");
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(modal_selection(&app), Some(1), "clamps at the end");
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(modal_selection(&app), Some(0), "k moves up");
    }

    #[test]
    fn esc_and_d_both_close_the_doctor_modal() {
        let mut app = App::sample();
        open_with(&mut app, &["a"]);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.modal.is_none(), "Esc closes the doctor modal");

        open_with(&mut app, &["a"]);
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.modal.is_none(), "d closes the doctor modal");
    }

    #[test]
    fn enter_is_inert_in_the_doctor_modal() {
        let mut app = App::sample();
        open_with(&mut app, &["a"]);
        app.handle_key(key(KeyCode::Enter));
        assert!(
            matches!(app.modal, Some(Modal::Doctor(_))),
            "Enter is inert: the doctor modal is dismiss-only"
        );
    }
}
