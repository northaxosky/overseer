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

    #[error("no Overseer instance at `{path}` (run `instance init` first)")]
    NotAnInstance { path: Utf8PathBuf },

    #[error("an Overseer instance already exists at `{path}`")]
    AlreadyAnInstance { path: Utf8PathBuf },

    #[error("`{0}` is not a managed mod; only managed mods can be enabled or disabled")]
    NotManaged(String),

    #[error("failed to parse instance config `{path}`")]
    Config {
        path: Utf8PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to serialize instance config `{path}`")]
    ConfigWrite {
        path: Utf8PathBuf,
        #[source]
        source: Box<toml::ser::Error>,
    },
}

pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> InstanceError {
    InstanceError::Io {
        path: path.to_owned(),
        source,
    }
}
