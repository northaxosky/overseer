//! Fixed pending-work directory and collision-safe tree moves

use std::io::ErrorKind;

use camino::Utf8Path;

use crate::error::io_err;
use crate::fs;

/// Create the previously probed empty bundle path
pub(super) fn create(path: &Utf8Path) -> Result<(), crate::IoError> {
    std::fs::create_dir(path).map_err(|source| crate::error::io_err(path, source))
}

/// Recursively remove completed pending work
pub(super) fn cleanup(path: &Utf8Path) -> Result<(), crate::IoError> {
    #[cfg(test)]
    super::failpoint::check(super::failpoint::Point::Cleanup, path)?;

    fs::remove_dir_all_opt(path)
}

/// Probe a fixed path so only `NotFound` counts as absent
pub(super) fn occupied(path: &Utf8Path) -> Result<bool, crate::IoError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_err(path, error)),
    }
}

/// Rename a tree only when the destination has no filesystem occupant
pub(super) fn rename_tree(from: &Utf8Path, to: &Utf8Path) -> Result<(), crate::IoError> {
    #[cfg(test)]
    super::failpoint::check(super::failpoint::Point::Rename, from)?;

    if occupied(to)? {
        return Err(io_err(
            to,
            std::io::Error::new(
                ErrorKind::AlreadyExists,
                "rename destination already exists",
            ),
        ));
    }

    std::fs::rename(from, to).map_err(|error| io_err(from, error))
}
