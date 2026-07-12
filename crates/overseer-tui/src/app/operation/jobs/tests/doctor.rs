//! Tests for background setup diagnostics

use super::*;

use overseer_core::test_support::{self, TestbedSpec};

use crate::app::{App, Modal, OperationKind, OperationState};

#[test]
fn doctor_job_returns_a_report_through_the_worker_boundary() {
    let (_temp, root) = test_support::temp();
    let instance = test_support::build_testbed(&root, &TestbedSpec::new());
    instance.save().expect("persist instance");
    let mut app = App::sample();
    app.session.instance = instance;

    app.start_operation(DoctorJob);

    assert_eq!(app.running_operation_kind(), Some(OperationKind::Doctor));
    assert!(app.modal.is_none(), "the job does not open a modal itself");
    app.finish_operation_after_terminal();

    assert!(
        matches!(app.modal, Some(Modal::Doctor(_))),
        "{:?}",
        app.operation
    );
    assert!(matches!(app.operation, OperationState::Idle));
}
