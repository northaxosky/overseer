//! Tests for typed operation messages

use super::*;

use overseer_core::apply::DeploymentStatus;
use overseer_core::deploy::FileConflict;
use overseer_core::install::DownloadEntry;
use overseer_core::saves::SaveInfo;
use overseer_diagnostics::Report;

use super::super::runner::{BackgroundJob, OperationReporter, WorkerRequest};
use crate::app::Session;

struct TestJob;

impl BackgroundJob for TestJob {
    const KIND: OperationKind = OperationKind::RefreshDownloads;

    fn run(
        self,
        _context: &OperationContext,
        _reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        Ok(OperationOutput::RefreshDownloads(Vec::new()))
    }
}

#[test]
fn runner_boundary_types_are_send() {
    fn assert_send<T: Send>() {}

    assert_send::<Session>();
    assert_send::<DeploymentStatus>();
    assert_send::<Report>();
    assert_send::<Vec<FileConflict>>();
    assert_send::<Vec<SaveInfo>>();
    assert_send::<Vec<DownloadEntry>>();
    assert_send::<OperationOutput>();
    assert_send::<OperationRecovery>();
    assert_send::<WorkerRequest<TestJob>>();
}

#[test]
fn typed_outputs_map_to_their_operation_kinds() {
    let cases = [
        (
            OperationOutput::Deploy {
                status: None,
                files: 3,
            },
            OperationKind::Deploy,
        ),
        (OperationOutput::Purge(None), OperationKind::Purge),
        (
            OperationOutput::ScanConflicts(Vec::new()),
            OperationKind::ScanConflicts,
        ),
        (
            OperationOutput::Doctor(Report::new(Vec::new())),
            OperationKind::Doctor,
        ),
        (
            OperationOutput::RefreshSaves(Vec::new()),
            OperationKind::RefreshSaves,
        ),
        (
            OperationOutput::RefreshDownloads(Vec::new()),
            OperationKind::RefreshDownloads,
        ),
    ];

    for (output, expected) in cases {
        assert_eq!(output.kind(), expected);
    }
}

#[test]
fn failure_display_keeps_primary_and_recovery_errors() {
    let failure = OperationFailure {
        message: "Deploy failed: primary".to_owned(),
        recovery: None,
        recovery_error: Some("secondary".to_owned()),
    };

    assert_eq!(
        failure.display_message(),
        "Deploy failed: primary; deployment status recovery failed: secondary"
    );
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
