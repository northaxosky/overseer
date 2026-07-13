//! Errors surfaced by the mod installer

use super::archive::ArchiveFormat;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors from installing a mod from an archive
#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error(
        "unsupported archive format: `{extension}` (supported: {})",
        ArchiveFormat::supported_list()
    )]
    UnsupportedFormat { extension: String },

    #[error("failed to read 7z archive `{path}`")]
    SevenZip {
        path: Utf8PathBuf,
        #[source]
        source: sevenz_rust2::Error,
    },

    #[error("failed to read zip archive `{path}`")]
    Zip {
        path: Utf8PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("archive contains no installable files")]
    EmptyArchive,

    #[error(
        "archive `{path}` expands to {uncompressed} bytes, despite {compressed}-byte size (decompression bomb?)"
    )]
    TooLarge {
        path: Utf8PathBuf,
        uncompressed: u64,
        compressed: u64,
    },

    #[error("path is not valid UTF-8: `{0}`")]
    NonUtf8Path(String),

    #[error("FOMOD installers aren't supported yet")]
    Fomod,

    #[error("archive content root contains reserved `.overseer-mod.toml` metadata")]
    ReservedMetadata,
}
