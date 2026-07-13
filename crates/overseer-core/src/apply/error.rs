//! Errors for the deployment-orchestration layer

use crate::deploy::DeployError;
use crate::instance::InstanceError;
use crate::lifecycle::LifecycleError;
use crate::lock::LockError;
use crate::plugins::PluginError;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Something went wrong while applying or reversing a profile's deployment
#[derive(Debug, Error)]
pub enum ApplyError {
    /// An instance may only have one live deployment at a time
    #[error("`{path}` already has a live deployment; purge it first")]
    AlreadyDeployed { path: Utf8PathBuf },

    /// Tried to purge but nothing is deployed
    #[error("no live deployment found at `{path}`")]
    NotDeployed { path: Utf8PathBuf },

    /// Refuse to rename a mod while a deployment is live
    #[error("cannot rename mods while `{path}` has a live deployment; purge it first")]
    DeployedCannotRename { path: Utf8PathBuf },

    /// The profile was renamed on disk, but writing the updated default-profile pointer failed
    #[error("renamed the profile, but updating the default profile failed")]
    DefaultProfileNotUpdated(#[source] InstanceError),

    /// The deployment state file could not be read or written as JSON
    #[error("deployment state `{path}`")]
    State {
        path: Utf8PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Another process or command already holds this instance's lock
    #[error("instance is in use by another Overseer process; try again once it finishes")]
    Busy,

    /// Installed-mod lifecycle recovery blocked this operation
    #[error(transparent)]
    Lifecycle(Box<LifecycleError>),

    /// A reversal could not be fully resolved; the journal is kept
    #[error("`{path}` has an unresolved deployment reversal; purge again to retry")]
    RecoveryFailed { path: Utf8PathBuf },

    /// A backup directory survives from a previous run with no journal to reverse it
    #[error(
        "`{path}` holds an orphaned backup from a previous run with no deployment journal; remove it by hand"
    )]
    OrphanedBackup { path: Utf8PathBuf },

    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error(transparent)]
    Deploy(#[from] DeployError),

    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error(transparent)]
    Plugin(#[from] PluginError),
}

impl From<LockError> for ApplyError {
    fn from(value: LockError) -> Self {
        match value {
            LockError::Busy => Self::Busy,
            LockError::Io(error) => Self::Io(error),
        }
    }
}

impl From<LifecycleError> for ApplyError {
    fn from(value: LifecycleError) -> Self {
        match value {
            LifecycleError::Busy => Self::Busy,
            LifecycleError::LiveDeployment { path } => Self::DeployedCannotRename { path },
            LifecycleError::Instance(error) => Self::Instance(error),
            LifecycleError::Io(error) => Self::Io(error),
            LifecycleError::Deployment(error) => *error,
            other => Self::Lifecycle(Box::new(other)),
        }
    }
}

/// Attach the offending path to an [`std::io::Error`]
pub(crate) use crate::error::{io_err, non_utf8, walk_io_err};
