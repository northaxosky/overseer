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

    #[error("io error at `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no plugin named `{0}` in the load order")]
    NotInLoadOrder(String),

    #[error("writing the game load order: {0}")]
    GameState(#[from] loadorder::Error),
}
