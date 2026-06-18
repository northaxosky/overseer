use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// Errors from installing a mod from an archive
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("io error at `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> InstallError {
    InstallError::Io {
        path: path.to_owned(),
        source,
    }
}
