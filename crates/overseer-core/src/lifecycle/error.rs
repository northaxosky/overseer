//! Errors from installed-mod lifecycle operations

use camino::Utf8PathBuf;
use thiserror::Error;

use crate::apply::ApplyError;
use crate::install::InstallError;
use crate::instance::InstanceError;

/// Failure from an installed-mod lifecycle operation
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// Another Overseer process holds the instance lock
    #[error("instance is in use by another Overseer process; try again once it finishes")]
    Busy,

    /// A deployment record occupies the fixed state path
    #[error(
        "cannot modify installed mods while deployment state exists at `{path}`; purge it first"
    )]
    DeploymentExists { path: Utf8PathBuf },

    /// A prior lifecycle bundle requires manual resolution
    #[error(
        "`{path}` contains a pending mod operation; resolve it by hand before any other instance mutation"
    )]
    PendingOperation { path: Utf8PathBuf },

    /// The shared apply lock returned an unrelated failure
    #[error(transparent)]
    Apply(ApplyError),

    /// An archive name is not one safe path basename
    #[error("archive must be one safe Downloads basename: `{name}`")]
    InvalidArchiveName { name: String },

    /// A Downloads archive is missing, a symlink, or not a regular file
    #[error("archive must be a direct regular file in Downloads: `{path}`")]
    ArchiveUnavailable { path: Utf8PathBuf },

    #[error(transparent)]
    Install(#[from] InstallError),

    #[error(transparent)]
    Io(#[from] crate::IoError),

    #[error(transparent)]
    Instance(#[from] InstanceError),

    /// One or more rollback steps failed and the bundle remains for inspection
    #[error("lifecycle rollback is incomplete at `{bundle}`: {issues:?}")]
    RollbackIncomplete {
        bundle: Utf8PathBuf,
        issues: Vec<String>,
    },
}

impl From<ApplyError> for LifecycleError {
    /// Map the shared lock's focused errors into lifecycle errors
    fn from(error: ApplyError) -> Self {
        match error {
            ApplyError::Busy => Self::Busy,
            ApplyError::Io(source) => Self::Io(source),
            other => Self::Apply(other),
        }
    }
}
