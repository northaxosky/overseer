//! Background operation execution and UI state

mod jobs;
pub(crate) mod protocol;
mod reducer;
pub(crate) mod runner;

pub(crate) use jobs::{DoctorJob, RefreshDownloadsJob, RefreshSavesJob, ScanConflictsJob};
pub(crate) use protocol::OperationKind;
pub(crate) use runner::OperationState;
