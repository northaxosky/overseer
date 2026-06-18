use super::archive::ArchiveFormat;
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

    #[error(
        "unsupported archive format: `{extension}` (supported: {})",
        ArchiveFormat::supported_list()
    )]
    UnsupportedFormat { extension: String },

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

    #[error("a mod named `{0}` is already installed")]
    AlreadyInstalled(String),

    #[error("archive contains no installable files")]
    EmptyArchive,

    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),

    #[error("data dir `{0}` not found inside the archive")]
    DataDirNotFound(String),
}

pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> InstallError {
    InstallError::Io {
        path: path.to_owned(),
        source,
    }
}
