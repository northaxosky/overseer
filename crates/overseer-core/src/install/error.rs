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

    #[error("unsupported archive format: `{0}` (supported: .7z, .zip")]
    UnsupportedFormat(String),

    #[error("failed to read 7z archive `{path}`")]
    SevenZip {
        path: Utf8PathBuf,
        #[source]
        source: sevenz_rust::Error,
    },

    #[error("failed to read zip archive `{path}`")]
    Zip {
        path: Utf8PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
}

pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> InstallError {
    InstallError::Io {
        path: path.to_owned(),
        source,
    }
}
