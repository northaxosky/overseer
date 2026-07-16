//! Tests for typed operation messages

use super::*;

use overseer_core::apply::DeploymentStatus;
use overseer_core::deploy::ConflictSnapshot;
use overseer_core::install::DownloadEntry;
use overseer_core::saves::SaveInfo;
use overseer_diagnostics::Report;

use super::super::runner::{BackgroundJob, OperationReporter, WorkerRequest};
use crate::app::{App, Session};

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
    assert_send::<ConflictSnapshot>();
    assert_send::<Vec<SaveInfo>>();
    assert_send::<Vec<DownloadEntry>>();
    assert_send::<OperationOutput>();
    assert_send::<OperationRecovery>();
    assert_send::<WorkerRequest<TestJob>>();
}

#[test]
fn typed_outputs_map_to_their_operation_kinds() {
    let install_session = Box::new(App::sample().session);
    let cases = [
        (
            OperationOutput::Deploy {
                status: None,
                files: 3,
            },
            OperationKind::Deploy,
        ),
        (
            OperationOutput::Install {
                name: "Mod".to_owned(),
                state: LifecycleState::Refreshed {
                    session: install_session,
                    downloads: Vec::new(),
                },
            },
            OperationKind::Install,
        ),
        (
            OperationOutput::Remove {
                name: "Mod".to_owned(),
                state: LifecycleState::Refreshed {
                    session: Box::new(App::sample().session),
                    downloads: Vec::new(),
                },
            },
            OperationKind::Remove,
        ),
        (
            OperationOutput::Replace {
                name: "Mod".to_owned(),
                state: LifecycleState::CommittedWithResidue(Utf8PathBuf::from("pending")),
            },
            OperationKind::Replace,
        ),
        (
            OperationOutput::Install {
                name: "Residue".to_owned(),
                state: LifecycleState::CommittedWithResidue(Utf8PathBuf::from("pending")),
            },
            OperationKind::Install,
        ),
        (
            OperationOutput::Purge {
                status: None,
                outcome: ReversalOutcome::default(),
            },
            OperationKind::Purge,
        ),
        (
            OperationOutput::ScanConflicts(ConflictSnapshot::from_entries(Vec::new())),
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
        recovery_error: Some("deployment status recovery failed: secondary".to_owned()),
    };

    assert_eq!(
        failure.display_message(),
        "Deploy failed: primary; deployment status recovery failed: secondary"
    );
}

#[test]
fn install_phase_labels_cover_session_reload() {
    assert_eq!(
        OperationPhase::ReloadingSession.label(),
        "Reloading session"
    );
}

#[test]
fn planned_operation_labels_are_complete() {
    let kinds = [
        (OperationKind::Deploy, "Deploy"),
        (OperationKind::Purge, "Purge"),
        (OperationKind::Install, "Install"),
        (OperationKind::Remove, "Remove"),
        (OperationKind::Replace, "Replace"),
        (OperationKind::ScanConflicts, "Conflicts"),
        (OperationKind::Doctor, "Doctor"),
        (OperationKind::RefreshSaves, "Saves refresh"),
        (OperationKind::RefreshDownloads, "Downloads refresh"),
    ];

    for (kind, label) in kinds {
        assert_eq!(kind.label(), label);
    }
}
