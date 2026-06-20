//! File logging for the CLI.
//!
//! Diagnostics go to a daily-rolling log file, never stdout — stdout is the
//! CLI's user-facing output. Setup is best-effort: if a log file cannot be
//! opened we warn once on stderr and run without one rather than failing the
//! command.

use std::io;

use camino::Utf8PathBuf;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Install the file logging subscriber. Safe to call once at startup.
pub fn init() {
    if let Err(e) = try_init() {
        eprintln!("warning: file logging disabled: {e}");
    }
}

fn try_init() -> io::Result<()> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir)?;
    let appender = tracing_appender::rolling::daily(&dir, "overseer.log");

    let filter = EnvFilter::try_from_env("OVERSEER_LOG")
        .unwrap_or_else(|_| EnvFilter::new("warn,overseer=info,overseer_core=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(appender))
        .try_init()
        .map_err(io::Error::other)
}

/// Resolve the log directory: `OVERSEER_LOG_DIR`, else `%LOCALAPPDATA%\Overseer\logs`,
/// else a temp-dir fallback for non-Windows/dev environments.
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
