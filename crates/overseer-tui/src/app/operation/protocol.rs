//! Typed operation messages shared by jobs, the runner, and the reducer

use camino::Utf8PathBuf;
use overseer_core::install::DownloadEntry;
use overseer_core::saves::SaveInfo;

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
    ScanningConflicts,
    RunningDiagnostics,
    ReadingSaves,
    ListingDownloads,
}

impl OperationPhase {
    /// Return the user facing phase label
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PlanningDeploy => "Planning deployment",
            Self::PreparingPurge => "Preparing purge",
            Self::ExtractingArchive => "Extracting archive",
            Self::ScanningConflicts => "Scanning enabled mod files",
            Self::RunningDiagnostics => "Gathering setup diagnostics",
            Self::ReadingSaves => "Reading and parsing save headers",
            Self::ListingDownloads => "Listing archive metadata",
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
pub(crate) enum OperationOutput {
    RefreshDownloads(Vec<DownloadEntry>),
    RefreshSaves(Vec<SaveInfo>),
}

impl OperationOutput {
    /// Return the operation kind represented by this output
    pub(super) fn kind(&self) -> OperationKind {
        match self {
            Self::RefreshDownloads(_) => OperationKind::RefreshDownloads,
            Self::RefreshSaves(_) => OperationKind::RefreshSaves,
        }
    }
}

#[derive(Debug)]
pub(crate) struct OperationFailure {
    pub(crate) message: String,
}

impl OperationFailure {
    /// Build a worker failure with a user-facing message
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
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
    Completion(WorkerCompletion),
}

#[cfg(test)]
#[path = "tests/protocol.rs"]
mod tests;
