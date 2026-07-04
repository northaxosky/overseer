//! Errors for plugin reading and load-order management

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors from reading or managing plugins
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("failed to parse plugin `{path}`")]
    Parse {
        path: Utf8PathBuf,
        #[source]
        source: esplugin::Error,
    },

    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error("no plugin named `{0}` in the load order")]
    NotInLoadOrder(String),

    #[error(transparent)]
    GameState(#[from] loadorder::Error),
}

pub(crate) use crate::error::io_err;
