//! Persistent launch marker state.

use super::LaunchMarker;
use crate::apply::ApplyError;
use crate::apply::InstanceLock;
use crate::instance::Instance;
use camino::Utf8PathBuf;

const FILE_NAME: &str = "launch.json";

/// Return the launch marker path for an instance.
pub(super) fn path(instance: &Instance) -> Utf8PathBuf {
    instance.state_dir().join(FILE_NAME)
}

/// Report marker presence without interpreting its contents.
pub(super) fn exists(instance: &Instance) -> Result<bool, ApplyError> {
    let path = path(instance);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::error::io_err(&path, error).into()),
    }
}

/// Write a marker while the caller holds the instance lock.
pub(super) fn write(instance: &Instance, marker: &LaunchMarker) -> Result<(), ApplyError> {
    let path = path(instance);
    crate::fs::ensure_dir(&instance.state_dir())?;
    let contents =
        serde_json::to_vec_pretty(marker).map_err(|source| ApplyError::LaunchMarkerState {
            path: path.clone(),
            source,
        })?;
    crate::fs::write_atomic(&path, &contents)?;
    Ok(())
}

/// Remove a marker while the caller holds the instance lock.
pub(super) fn remove_locked(instance: &Instance) -> Result<bool, ApplyError> {
    let path = path(instance);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::error::io_err(&path, error).into()),
    }
}

/// Clear a launch marker under the instance lock.
pub(super) fn clear(instance: &Instance) -> Result<bool, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    remove_locked(instance)
}

/// Clear the marker only when it still belongs to the expected launcher.
pub(super) fn clear_if(instance: &Instance, expected: &LaunchMarker) -> Result<bool, ApplyError> {
    let _lock = InstanceLock::acquire(instance)?;
    let path = path(instance);
    let contents = match std::fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(crate::error::io_err(&path, error).into()),
    };
    let marker: LaunchMarker =
        serde_json::from_slice(&contents).map_err(|source| ApplyError::LaunchMarkerState {
            path: path.clone(),
            source,
        })?;
    if marker != *expected {
        return Ok(false);
    }
    remove_locked(instance)
}
