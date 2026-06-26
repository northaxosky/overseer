//! Errors produced by the deployment engine

use super::DeployerKind;
use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors produced by the deployment engine
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

/// Attach the offending path to an [`std::io::Error`].
pub(crate) use crate::error::io_err;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[test]
    fn io_err_attaches_path_and_preserves_source_kind() {
        let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let err = io_err(Utf8Path::new("C:/x/y.dds"), source);
        assert_eq!(err.path, Utf8PathBuf::from("C:/x/y.dds"));
        assert_eq!(err.source.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn missing_staging_display_mentions_mod_and_path() {
        let err = DeployError::MissingStaging {
            mod_name: "CoolMod".to_string(),
            path: Utf8PathBuf::from("C:/mods/CoolMod"),
        };
        let text = err.to_string();
        assert!(text.contains("CoolMod"));
        assert!(text.contains("C:/mods/CoolMod"));
    }

    #[test]
    fn non_utf8_path_display_includes_the_offending_value() {
        let err = DeployError::NonUtf8Path("weird\u{FFFD}name".to_string());
        assert!(err.to_string().contains("weird"));
    }
}
