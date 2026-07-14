//! Typed operation messages shared by jobs, the runner, and the reducer

use camino::Utf8PathBuf;
use overseer_core::apply::{self, DeploymentStatus, ReversalOutcome};
use overseer_core::deploy::FileConflict;
use overseer_core::install::DownloadEntry;
use overseer_core::instance::Instance;
use overseer_core::saves::SaveInfo;
use overseer_diagnostics::Report;

use super::super::Session;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationKind {
    Deploy,
    Purge,
    Install,
    ScanConflicts,
    Doctor,
    RefreshSaves,
    RefreshDownloads,
}

impl OperationKind {
    /// Return the user-facing operation label
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Deploy => "Deploy",
            Self::Purge => "Purge",
            Self::Install => "Install",
            Self::ScanConflicts => "Conflicts",
            Self::Doctor => "Doctor",
            Self::RefreshSaves => "Saves refresh",
            Self::RefreshDownloads => "Downloads refresh",
        }
    }

    /// Return the worker-thread name suffix
    pub(super) fn thread_label(self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Purge => "purge",
            Self::Install => "install",
            Self::ScanConflicts => "conflicts",
            Self::Doctor => "doctor",
            Self::RefreshSaves => "saves",
            Self::RefreshDownloads => "downloads",
        }
    }

    /// Report whether this operation mutates instance state
    pub(crate) fn is_mutating(self) -> bool {
        matches!(self, Self::Deploy | Self::Purge | Self::Install)
    }

    /// Report whether this operation refreshes cached data
    pub(super) fn is_refresh(self) -> bool {
        matches!(
            self,
            Self::ScanConflicts | Self::RefreshSaves | Self::RefreshDownloads
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationPhase {
    PlanningDeploy,
    PreparingPurge,
    ExtractingArchive,
    ReloadingSession,
    ScanningConflicts,
    RunningDiagnostics,
    ReadingSaves,
    ListingDownloads,
    Deploying,
    Purging,
    Restoring,
    Finalizing,
}

impl OperationPhase {
    /// Return the user facing phase label
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PlanningDeploy => "Planning deployment",
            Self::PreparingPurge => "Preparing purge",
            Self::ExtractingArchive => "Extracting archive",
            Self::ReloadingSession => "Reloading session",
            Self::ScanningConflicts => "Scanning enabled mod files",
            Self::RunningDiagnostics => "Gathering setup diagnostics",
            Self::ReadingSaves => "Reading and parsing save headers",
            Self::ListingDownloads => "Listing archive metadata",
            Self::Deploying => "Deploying and backing up",
            Self::Purging => "Purging deployment",
            Self::Restoring => "Restoring deployment",
            Self::Finalizing => "Finalizing",
        }
    }

    /// Select the initial phase for an operation
    pub(super) fn initial(kind: OperationKind) -> Self {
        match kind {
            OperationKind::Deploy => Self::PlanningDeploy,
            OperationKind::Purge => Self::PreparingPurge,
            OperationKind::Install => Self::ExtractingArchive,
            OperationKind::ScanConflicts => Self::ScanningConflicts,
            OperationKind::Doctor => Self::RunningDiagnostics,
            OperationKind::RefreshSaves => Self::ReadingSaves,
            OperationKind::RefreshDownloads => Self::ListingDownloads,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationContext {
    pub(crate) instance_root: Utf8PathBuf,
    pub(crate) profile: String,
}

impl OperationContext {
    /// Capture owned identifiers for execution and stale result validation
    pub(super) fn capture(session: &Session) -> Self {
        Self {
            instance_root: session.instance.root.clone(),
            profile: session.profile.name.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum InstallState {
    Refreshed {
        session: Box<Session>,
        downloads: Vec<DownloadEntry>,
    },
    CommittedWithResidue(Utf8PathBuf),
}

#[derive(Debug)]
pub(crate) enum OperationOutput {
    RefreshDownloads(Vec<DownloadEntry>),
    RefreshSaves(Vec<SaveInfo>),
    Doctor(Report),
    ScanConflicts(Vec<FileConflict>),
    Purge {
        status: Option<DeploymentStatus>,
        outcome: ReversalOutcome,
    },
    Deploy {
        status: Option<DeploymentStatus>,
        files: usize,
    },
    Install {
        name: String,
        state: InstallState,
    },
}

impl OperationOutput {
    /// Return the operation kind represented by this output
    pub(super) fn kind(&self) -> OperationKind {
        match self {
            Self::RefreshDownloads(_) => OperationKind::RefreshDownloads,
            Self::RefreshSaves(_) => OperationKind::RefreshSaves,
            Self::Doctor(_) => OperationKind::Doctor,
            Self::ScanConflicts(_) => OperationKind::ScanConflicts,
            Self::Purge { .. } => OperationKind::Purge,
            Self::Deploy { .. } => OperationKind::Deploy,
            Self::Install { .. } => OperationKind::Install,
        }
    }
}

#[derive(Debug)]
pub(crate) struct OperationFailure {
    pub(crate) message: String,
    pub(crate) recovery: Option<OperationRecovery>,
    pub(crate) recovery_error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum OperationRecovery {
    DeploymentStatus(Option<Box<DeploymentStatus>>),
    Session(Box<Session>),
}

impl OperationFailure {
    /// Build a worker failure with a user-facing message
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            recovery: None,
            recovery_error: None,
        }
    }

    /// Preserve the primary failure while probing authoritative deployment status
    pub(super) fn with_deployment_recovery(
        message: impl Into<String>,
        instance: &Instance,
    ) -> Self {
        let message = message.into();

        match apply::status(instance) {
            Ok(status) => Self {
                message,
                recovery: Some(OperationRecovery::DeploymentStatus(status.map(Box::new))),
                recovery_error: None,
            },
            Err(error) => Self {
                message,
                recovery: None,
                recovery_error: Some(format!("deployment status recovery failed: {error}")),
            },
        }
    }

    /// Combine primary and secondary failure details for persistent display
    pub(crate) fn display_message(&self) -> String {
        match &self.recovery_error {
            Some(error) => format!("{}; {error}", self.message),
            None => self.message.clone(),
        }
    }

    /// Preserve the primary failure while reloading authoritative session state
    pub(super) fn with_session_recovery(
        message: impl Into<String>,
        context: &OperationContext,
    ) -> Self {
        let message = message.into();

        match Session::load(&context.instance_root, Some(&context.profile)) {
            Ok(session) => Self {
                message,
                recovery: Some(OperationRecovery::Session(Box::new(session))),
                recovery_error: None,
            },
            Err(error) => Self {
                message,
                recovery: None,
                recovery_error: Some(format!("session recovery failed: {error}")),
            },
        }
    }

    /// Carry an authoritative session loaded before a later refresh failed
    pub(super) fn with_session(message: impl Into<String>, session: Session) -> Self {
        Self {
            message: message.into(),
            recovery: Some(OperationRecovery::Session(Box::new(session))),
            recovery_error: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct WorkerCompletion {
    pub(super) context: OperationContext,
    pub(super) outcome: Result<OperationOutput, OperationFailure>,
}

#[derive(Debug)]
pub(super) enum WorkerEvent {
    Phase(OperationPhase),
    Started(usize),
    Deployed { index: usize, relative: Utf8PathBuf },
    Removed { index: usize, relative: Utf8PathBuf },
    Finished,
    Completion(Box<WorkerCompletion>),
}

#[cfg(test)]
#[path = "tests/protocol.rs"]
mod tests;
