//! Background operation execution and UI state

mod jobs;
mod progress;
pub(crate) mod protocol;
mod reducer;
pub(crate) mod runner;

pub(crate) use jobs::{
    DeployJob, DoctorJob, InstallJob, PurgeJob, RefreshDownloadsJob, RefreshSavesJob, RemoveJob,
    ReplaceJob, ScanConflictsJob,
};
pub(crate) use protocol::OperationKind;
pub(crate) use runner::{OperationProgress, OperationState};
