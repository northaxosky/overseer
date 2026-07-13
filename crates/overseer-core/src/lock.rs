//! Per-instance advisory lock guarding instance mutations

use std::fs::{File, OpenOptions, TryLockError};

use thiserror::Error;

use crate::error::IoError;
use crate::fs;
use crate::instance::Instance;

/// Failure to acquire an instance's cross-process lock
#[derive(Debug, Error)]
pub(crate) enum LockError {
    /// Another Overseer process already holds the lock
    #[error("instance is in use by another Overseer process; try again once it finishes")]
    Busy,

    /// Opening or locking the lock file failed
    #[error(transparent)]
    Io(#[from] IoError),
}

/// Exclusive RAII lock on an instance's `state/overseer.lock`
#[derive(Debug)]
pub(crate) struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Try to take the instance lock without blocking
    pub(crate) fn acquire(instance: &Instance) -> Result<Self, LockError> {
        let dir = instance.state_dir();
        fs::ensure_dir(&dir)?;

        let path = dir.join("overseer.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| crate::error::io_err(&path, error))?;

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(LockError::Busy),
            Err(TryLockError::Error(error)) => {
                Err(LockError::Io(crate::error::io_err(&path, error)))
            }
        }
    }
}
