use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors from reading or writing an instance's on disk state
#[derive(Debug, Error)]
pub enum InstanceError {
    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error("path is not valid UTF-8: `{0}`")]
    NonUtf8Path(String),

    #[error("no mod named `{0}` in this profile")]
    ModNotInList(String),

    #[error("a mod named `{0}` is already in this profile")]
    ModAlreadyInList(String),

    #[error("invalid mod name `{0}`")]
    InvalidModName(String),

    #[error("no installed mod named `{0}`")]
    ModNotInstalled(String),

    #[error("an installed mod named `{0}` already exists")]
    ModAlreadyInstalled(String),

    #[error("no Overseer instance at `{path}` (run `overseer instance init` first)")]
    NotAnInstance { path: Utf8PathBuf },

    #[error("an Overseer instance already exists at `{path}`")]
    AlreadyAnInstance { path: Utf8PathBuf },

    #[error("`{0}` is not a managed mod; only managed mods can be enabled or disabled")]
    NotManaged(String),

    #[error("invalid separator name: {0}")]
    InvalidSeparatorName(String),

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

    #[error("could not locate %LOCALAPPDATA%; set `local_dir` in overseer.toml")]
    NoLocalAppData,

    #[error("could not locate the Documents folder to find the game's INI directory")]
    NoDocumentsDir,

    #[error("the Documents path is not valid UTF-8: `{0}`")]
    NonUtf8DocumentsPath(std::path::PathBuf),

    #[error("a profile named `{0}` already exists")]
    ProfileExists(String),

    #[error("invalid profile name `{0}`")]
    InvalidProfileName(String),

    #[error("no profile named `{0}`")]
    ProfileNotFound(String),
}

pub(crate) use crate::error::io_err;
