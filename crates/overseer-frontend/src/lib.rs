//! Front-end support for Overseer's binaries (CLI, TUI, and later GUI)
//!
//! Backend-neutral concerns every front end needs but that `overseer-core` must
//! not own (it stays UI-agnostic and print-free): file logging now, the
//! role/style descriptor later.

pub mod logging;
pub mod style;

mod error;
pub use error::AbsolutizeError;

use camino::{Utf8Path, Utf8PathBuf};

/// Resolve a possibly-relative path against the current working directory
pub fn absolutize(path: &Utf8Path) -> Result<Utf8PathBuf, AbsolutizeError> {
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    let cwd = std::env::current_dir().map_err(AbsolutizeError::Cwd)?;
    let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|_| AbsolutizeError::NonUtf8Cwd)?;
    Ok(cwd.join(path))
}
