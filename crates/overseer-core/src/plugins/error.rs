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

    #[error("path is not valid UTF-8: `{0}`")]
    NonUtf8Path(String),

    #[error("no plugin named `{0}` in the load order")]
    NotInLoadOrder(String),

    #[error(transparent)]
    GameState(#[from] loadorder::Error),
}

pub(crate) use crate::error::io_err;

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_utf8_path_display_includes_the_offending_value() {
        let err = PluginError::NonUtf8Path("weird\u{FFFD}name".to_string());
        assert!(err.to_string().contains("weird"));
    }
}
