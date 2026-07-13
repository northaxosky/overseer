//! Installed-mod lifecycle errors and cleanup warnings

use camino::Utf8PathBuf;
use thiserror::Error;

use crate::apply::ApplyError;
use crate::error::IoError;
use crate::instance::InstanceError;
use crate::lock::LockError;

/// Path blocking cleanup after commit
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupWarning {
    pub blocked_path: Utf8PathBuf,
}

/// Why lifecycle mutation or recovery could not finish safely
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// Another Overseer process holds the instance lock
    #[error("instance is in use by another Overseer process; try again once it finishes")]
    Busy,

    /// Installed mods cannot change during a live deployment
    #[error("cannot change installed mods while `{path}` has a live deployment; purge it first")]
    LiveDeployment { path: Utf8PathBuf },

    /// The lifecycle journal cannot be trusted
    #[error("corrupt lifecycle journal `{path}`: {reason}")]
    CorruptJournal { path: Utf8PathBuf, reason: String },

    /// Recovery found paths it cannot prove belong to the transaction
    #[error("lifecycle recovery conflicts at {paths:?}")]
    RecoveryConflict { paths: Vec<Utf8PathBuf> },

    /// A prior committed operation still needs cleanup
    #[error("lifecycle cleanup remains pending: {0:?}")]
    CleanupPending(CleanupWarning),

    /// Deployment recovery failed under the shared lock
    #[error("deployment recovery failed")]
    Deployment(#[source] Box<ApplyError>),

    /// Instance naming, discovery, or profile persistence failed
    #[error(transparent)]
    Instance(#[from] InstanceError),

    /// A path-aware filesystem operation failed
    #[error(transparent)]
    Io(#[from] IoError),

    #[cfg(test)]
    #[error("simulated lifecycle process crash")]
    TestCrash,
}

impl From<LockError> for LifecycleError {
    fn from(value: LockError) -> Self {
        match value {
            LockError::Busy => Self::Busy,
            LockError::Io(error) => Self::Io(error),
        }
    }
}
