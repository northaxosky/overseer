//! Errors produced by the deployment engine

use super::DeployerKind;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Something went wrong while deploying or purging a mod's files
#[derive(Debug, Error)]
pub enum DeployError {
    #[error("mod `{mod_name}` has no staging directory @ `{path}`")]
    MissingStaging { mod_name: String, path: Utf8PathBuf },

    #[error("path is not valid UTF-8: `{0}`")]
    NonUtf8Path(String),

    #[error("failed to walk staging directory `{path}`")]
    Walk {
        path: Utf8PathBuf,
        #[source]
        source: walkdir::Error,
    },

    #[error(
        "mods and target are on different volumes; hardlink deployment requires the same drive (source: `{source_path}`, target: `{target}`)"
    )]
    CrossVolume {
        source_path: Utf8PathBuf,
        target: Utf8PathBuf,
    },

    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error("a backed-up file remains unresolved at `{path}`")]
    ResidualBackup { path: Utf8PathBuf },

    #[error("the `{deployer}` backend is not implemented")]
    Unsupported { deployer: DeployerKind },

    #[error("failed to launch `{program}`")]
    Launch {
        program: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "mod `{name}` nests a `Data` folder inside `Root` (`{path}`); Root & Data must be separate"
    )]
    RootDataConflict { name: String, path: Utf8PathBuf },
}

/// Attach the offending path to an [`std::io::Error`]
pub(crate) use crate::error::{io_err, non_utf8, walk_io_err};

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;
