//! Front-end support errors.

use thiserror::Error;

/// Error from [`crate::absolutize`]
#[derive(Debug, Error)]
pub enum AbsolutizeError {
    #[error("could not read the current working directory")]
    Cwd(#[source] std::io::Error),
    #[error("the current working directory is not valid UTF-8")]
    NonUtf8Cwd,
}

/// Error from [`crate::logging::init`]
#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("could not create the log directory `{path}`")]
    CreateDir {
        path: camino::Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not install the logging subscriber")]
    Install(#[source] tracing_subscriber::util::TryInitError),
}
