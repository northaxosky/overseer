//! Shared error building blocks reused across the crate's domain error types.
//!
//! Every module used to repeat the same `Io { path, source }` variant, its
//! `"io error at ..."` message, and a three-line `io_err` constructor. This
//! centralizes that one shape so each domain error simply wraps [`IoError`].

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// An [`std::io::Error`] tagged with its path, ready for domain errors to wrap via `#[from]`
#[derive(Debug, Error)]
#[error("io error at `{path}`")]
pub struct IoError {
    /// The path the failing operation was working on
    pub path: Utf8PathBuf,
    /// The underlying OS error
    #[source]
    pub source: std::io::Error,
}

impl IoError {
    /// Tag `source` with the `path` it failed on
    pub(crate) fn new(path: &Utf8Path, source: std::io::Error) -> Self {
        Self {
            path: path.to_owned(),
            source,
        }
    }
}

/// Tag an [`std::io::Error`] with its path as an [`IoError`] for domain errors to convert via `?`
pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> IoError {
    IoError::new(path, source)
}

/// Render a non-UTF-8 path as the lossy string our `NonUtf8Path`-style variants carry
pub(crate) fn non_utf8(path: &std::path::Path) -> String {
    path.display().to_string()
}
