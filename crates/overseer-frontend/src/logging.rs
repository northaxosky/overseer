//! File logging for Overseer's front end

use camino::Utf8PathBuf;
use std::io;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Install the file logging subscriber; call once at startup. Returns an error if it cant start
pub fn init(default_filter: &str) -> io::Result<()> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir)?;
    let appender = tracing_appender::rolling::daily(&dir, "overseer.log");
    let filter =
        EnvFilter::try_from_env("OVERSEER_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(appender))
        .try_init()
        .map_err(io::Error::other)
}

/// Resolve the log directory: `OVERSEER_LOG_DIR`, else `%LOCALAPPDATA%\Overseer\logs`
fn log_dir() -> Utf8PathBuf {
    if let Ok(dir) = std::env::var("OVERSEER_LOG_DIR") {
        return Utf8PathBuf::from(dir);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return Utf8PathBuf::from(local).join("Overseer").join("logs");
    }
    Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .unwrap_or_else(|_| Utf8PathBuf::from("."))
        .join("overseer")
        .join("logs")
}
