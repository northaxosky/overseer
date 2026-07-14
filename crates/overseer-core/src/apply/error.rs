//! Errors for the deployment-orchestration layer

use super::outcome::ReversalOutcome;
use crate::deploy::DeployError;
use crate::instance::InstanceError;
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

    /// A reversal could not be fully resolved; the journal is kept
    #[error(
        "`{path}` has an unresolved deployment reversal ({} removed, {} restored, {} captured, {} preserved, {} unresolved); purge again to retry",
        outcome.removed.len(),
        outcome.restored.len(),
        outcome.captured.len(),
        outcome.preserved_conflicts.len(),
        outcome.unresolved.len()
    )]
    RecoveryFailed {
        path: Utf8PathBuf,
        outcome: Box<ReversalOutcome>,
    },

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

/// Attach the offending path to an [`std::io::Error`]
pub(crate) use crate::error::{io_err, non_utf8};
