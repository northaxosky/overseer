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

    /// The deterministic bundle manifest could not be serialized
    #[error("failed to serialize lifecycle manifest `{path}`")]
    Manifest {
        path: Utf8PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// An archive path has no safe supported basename
    #[error("archive path has no safe supported basename: `{path}`")]
    InvalidArchive { path: Utf8PathBuf },

    /// A Downloads import would overwrite an existing entry
    #[error("download destination already exists: `{path}`")]
    DownloadCollision { path: Utf8PathBuf },

    /// A failed import left a partial Downloads entry
    #[error(
        "archive import failed ({copy}) and partial cleanup failed ({cleanup}); retained `{path}`"
    )]
    PartialCopy {
        path: Utf8PathBuf,
        copy: String,
        cleanup: String,
    },

    /// An installed mod has no Overseer provenance
    #[error("missing Overseer provenance for `{name}` at `{path}`; replace the mod explicitly")]
    MissingProvenance { name: String, path: Utf8PathBuf },

    /// An installed mod has invalid Overseer provenance
    #[error(
        "invalid Overseer provenance for `{name}` at `{path}`: {reason}; replace the mod explicitly"
    )]
    InvalidProvenance {
        name: String,
        path: Utf8PathBuf,
        reason: String,
    },

    /// A provenance archive is unavailable from Downloads
    #[error("archive for `{name}` is unavailable at `{path}`; replace the mod explicitly")]
    MissingArchive { name: String, path: Utf8PathBuf },

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
