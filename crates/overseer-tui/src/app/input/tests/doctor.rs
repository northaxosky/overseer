//! Tests for the Doctor modal

use super::*;
use crate::app::input::test_helpers::{key, modal_selection};
use crate::app::{DoctorReport, ListCursor, Modal, OperationKind};
use overseer_diagnostics::{Finding, Report, Severity};

/// Seed an open Doctor modal with `titles` findings, selecting the first
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
    let list = ListCursor::first(report.findings.len());
    app.modal = Some(Modal::Doctor(DoctorReport { report, list }));
}

#[test]
fn d_starts_a_worker_without_opening_a_doctor_modal() {
    let mut app = App::sample();

    app.handle_key(key(KeyCode::Char('d')));

    assert_eq!(app.running_operation_kind(), Some(OperationKind::Doctor));
    assert!(app.modal.is_none());
    app.finish_operation_after_terminal();
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
