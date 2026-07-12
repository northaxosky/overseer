//! Tests for typed operation messages

use super::*;

use overseer_core::install::DownloadEntry;

use super::super::runner::{BackgroundJob, WorkerRequest};
use crate::app::Session;

#[test]
fn runner_boundary_types_are_send() {
    fn assert_send<T: Send>() {}

    assert_send::<Session>();
    assert_send::<Vec<DownloadEntry>>();
    assert_send::<WorkerRequest>();
    assert_send::<Box<dyn BackgroundJob>>();
}
#[test]
fn planned_operation_labels_are_complete() {
    let kinds = [
        (OperationKind::Deploy, "Deploy"),
        (OperationKind::Purge, "Purge"),
        (OperationKind::Install, "Install"),
        (OperationKind::ScanConflicts, "Conflicts"),
        (OperationKind::Doctor, "Doctor"),
        (OperationKind::RefreshSaves, "Saves refresh"),
        (OperationKind::RefreshDownloads, "Downloads refresh"),
    ];

    for (kind, label) in kinds {
        assert_eq!(kind.label(), label);
    }
}
