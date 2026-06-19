//! Errors for the deployment-orchestration layer

use crate::deploy::DeployError;
use crate::instance::InstanceError;
use crate::plugins::PluginError;
use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// Something went wrong while applying or reversing a profile's deployment
#[derive(Debug, Error)]
pub enum ApplyError {
    /// An instance may only have one live deployment at a time
    #[error("{path} already has a live deployment; purge it first")]
    AlreadyDeployed { path: Utf8PathBuf },

    /// Tried to purge but nothing is deployed
    #[error("no live deployment found at {path}")]
    NotDeployed { path: Utf8PathBuf },

    /// The deployment state file could not be read or written as JSON
    #[error("deployment state {path}: {source}")]
    State {
        path: Utf8PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// The game's local app-data dir couldn't be located
    #[error("could not locate %LOCALAPPDATA%; set `local_dir` in overseer.toml")]
    NoLocalAppData,

    #[error("{path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Deploy(#[from] DeployError),

    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error(transparent)]
    Plugin(#[from] PluginError),
}

/// Build an [`ApplyError::Io`] tagged with the path that failed
pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> ApplyError {
    ApplyError::Io {
        path: path.to_owned(),
        source,
    }
}
