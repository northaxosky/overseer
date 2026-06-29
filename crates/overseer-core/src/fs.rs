//! Path-aware thin wrappers over `std::fs`. Every error carries the offending path (via
//! [`IoError`]), so these `?`-compose into any domain error; and the repeated
//! NotFound / create-parent / atomic-write idioms collapse here instead of being open-coded.

use crate::error::{IoError, io_err};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use camino::Utf8Path;
use std::io::{ErrorKind, Write};

/// Read a file to a `String`; `Ok(None)` when it doesn't exist. Callers reconstitute their own
/// default (`unwrap_or_default`, an empty parse, or a typed NotFound via `else`).
pub(crate) fn read_to_string_opt(path: &Utf8Path) -> Result<Option<String>, IoError> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

/// Read a file to bytes; `Ok(None)` when it doesn't exist.
pub(crate) fn read_opt(path: &Utf8Path) -> Result<Option<Vec<u8>>, IoError> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, e)),
    }
}

/// `create_dir_all`, binding `path` on error.
pub(crate) fn ensure_dir(path: &Utf8Path) -> Result<(), IoError> {
    std::fs::create_dir_all(path).map_err(|e| io_err(path, e))
}

/// Write bytes, creating parent dirs first; binds `path` on error.
pub(crate) fn write(path: &Utf8Path, contents: impl AsRef<[u8]>) -> Result<(), IoError> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(path, contents).map_err(|e| io_err(path, e))
}

/// Crash-safe write (temp + rename), creating parent dirs first.
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

/// Remove a file; `Ok(())` if it's already gone.
pub(crate) fn remove_file_opt(path: &Utf8Path) -> Result<(), IoError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp;

    #[test]
    fn read_opt_is_none_when_missing_and_some_when_present() {
        let (_t, root) = temp();
        assert_eq!(read_to_string_opt(&root.join("nope.txt")).unwrap(), None);
        write(&root.join("a/b.txt"), "hi").unwrap();
        assert_eq!(
            read_to_string_opt(&root.join("a/b.txt"))
                .unwrap()
                .as_deref(),
            Some("hi")
        );
        assert_eq!(read_opt(&root.join("a/b.txt")).unwrap().unwrap(), b"hi");
    }

    #[test]
    fn write_creates_parents_and_atomic_round_trips() {
        let (_t, root) = temp();
        write_atomic(&root.join("deep/x.bin"), b"data").unwrap();
        assert_eq!(
            read_opt(&root.join("deep/x.bin")).unwrap().unwrap(),
            b"data"
        );
    }

    #[test]
    fn remove_file_opt_is_ok_when_absent() {
        let (_t, root) = temp();
        remove_file_opt(&root.join("ghost")).unwrap();
        write(&root.join("real"), "x").unwrap();
        remove_file_opt(&root.join("real")).unwrap();
        assert!(read_opt(&root.join("real")).unwrap().is_none());
    }
}
