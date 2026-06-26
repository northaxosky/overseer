//! Front-end support errors.

use thiserror::Error;

/// Error from [`crate::absolutize`].
#[derive(Debug, Error)]
pub enum AbsolutizeError {
    #[error("could not read the current working directory")]
    Cwd(#[source] std::io::Error),
    #[error("the current working directory is not valid UTF-8")]
    NonUtf8Cwd,
}
