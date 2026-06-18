use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// Errors from reading or writing an instance's on disk state
#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("io error at `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),

    #[error("no mod named `{0}` in this profile")]
    ModNotInList(String),

    #[error("a mod named `{0}` is already in this profile")]
    ModAlreadyInList(String),
}

pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> InstanceError {
    InstanceError::Io {
        path: path.to_owned(),
        source,
    }
}
