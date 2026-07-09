//! Path-aware thin wrappers over `std::fs`. Every error carries the offending path (via
//! [`IoError`]), so these `?`-compose into any domain error; and the repeated
//! NotFound / create-parent / atomic-write idioms collapse here instead of being open-coded.

use crate::error::{IoError, io_err};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use camino::Utf8Path;
use std::io::{ErrorKind, Read, Write};

/// Read a file to a `String`, returning `Ok(None)` when it doesn't exist so callers choose their default
pub(crate) fn read_to_string_opt(path: &Utf8Path) -> Result<Option<String>, IoError> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

/// Read a file to bytes; `Ok(None)` when it doesn't exist
pub(crate) fn read_opt(path: &Utf8Path) -> Result<Option<Vec<u8>>, IoError> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

/// The size in bytes of a file; `Ok(None)` when it doesn't exist
pub(crate) fn size_opt(path: &Utf8Path) -> Result<Option<u64>, IoError> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(Some(m.len())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

/// `create_dir_all`, binding `path` on error
pub(crate) fn ensure_dir(path: &Utf8Path) -> Result<(), IoError> {
    std::fs::create_dir_all(path).map_err(|e| io_err(path, e))
}

/// Write bytes, creating parent dirs first; binds `path` on error
pub(crate) fn write(path: &Utf8Path, contents: impl AsRef<[u8]>) -> Result<(), IoError> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(path, contents).map_err(|e| io_err(path, e))
}

/// Crash-safe write (temp + rename), creating parent dirs first
pub(crate) fn write_atomic(path: &Utf8Path, contents: &[u8]) -> Result<(), IoError> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    AtomicFile::new(path, OverwriteBehavior::AllowOverwrite)
        .write(|f| f.write_all(contents))
        .map_err(|e| match e {
            atomicwrites::Error::Internal(io) | atomicwrites::Error::User(io) => io_err(path, io),
        })
}

/// Rename `from` to `to` (atomic); binds the source path on error
pub(crate) fn rename(from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError> {
    std::fs::rename(from, to).map_err(|e| io_err(from, e))
}

/// Move a file, falling back to copy + fsync + remove when a rename crosses volumes; creates parent dirs
pub(crate) fn move_file(from: &Utf8Path, to: &Utf8Path) -> Result<(), IoError> {
    if to.exists() {
        return Err(io_err(
            to,
            std::io::Error::new(ErrorKind::AlreadyExists, "move destination already exists"),
        ));
    }
    if let Some(parent) = to.parent() {
        ensure_dir(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| io_err(from, e))?;
    fsync(to)?;
    remove_file_opt(from)
}

/// Flush a file to stable storage before an atomic rename; opens with write access (Windows `FlushFileBuffers` needs it)
pub(crate) fn fsync(path: &Utf8Path) -> Result<(), IoError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| io_err(path, e))?;
    file.sync_all().map_err(|e| io_err(path, e))
}

/// Remove a file; `Ok(())` if it's already gone
pub(crate) fn remove_file_opt(path: &Utf8Path) -> Result<(), IoError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

/// Remove a directory tree; `Ok(())` if it's already gone
pub(crate) fn remove_dir_all_opt(path: &Utf8Path) -> Result<(), IoError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

/// Open a directory for iteration: `Ok(None)` when it doesn't exist
pub(crate) fn read_dir_opt(dir: &Utf8Path) -> Result<Option<std::fs::ReadDir>, IoError> {
    match std::fs::read_dir(dir) {
        Ok(entries) => Ok(Some(entries)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(dir, e)),
    }
}

/// Read `path` in 64 KiB chunks, handing each to `on_chunk`; hash large fiels without buffering
pub(crate) fn read_chunks(path: &Utf8Path, mut on_chunk: impl FnMut(&[u8])) -> Result<(), IoError> {
    let mut file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        on_chunk(&buf[..n]);
    }
    Ok(())
}

/// Move a corrupt file aside to `<path>.bak` so a later write won't clobber it. No-op if absent
pub(crate) fn backup_corrupt(path: &Utf8Path) -> Result<(), IoError> {
    let bak = format!("{path}.bak");
    match std::fs::rename(path, &bak) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

#[cfg(test)]
#[path = "tests/fs.rs"]
mod tests;
