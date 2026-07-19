//! Concrete background jobs

mod conflicts;
mod deployment;
mod doctor;
mod downloads;
mod install;
mod launch;
mod remove;
mod replace;
mod saves;

pub(crate) use conflicts::ScanConflictsJob;
pub(crate) use deployment::{DeployJob, PurgeJob};
pub(crate) use doctor::DoctorJob;
pub(crate) use downloads::RefreshDownloadsJob;
pub(crate) use install::InstallJob;
pub(crate) use launch::PrepareLaunchJob;
pub(crate) use remove::RemoveJob;
pub(crate) use replace::ReplaceJob;
pub(crate) use saves::RefreshSavesJob;
