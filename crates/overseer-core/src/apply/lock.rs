//! Per-instance advisory lock guarding instance mutating operations

use super::error::{ApplyError, io_err};
use crate::instance::Instance;
use std::fs::{File, OpenOptions, TryLockError};

/// An exclusive RAII advisory lock on an instance's `state/overseer.lock`
#[derive(Debug)]
pub(crate) struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Try to take the instance lock without blocking. Returns [`ApplyError::Busy`]
    pub(crate) fn acquire(instance: &Instance) -> Result<Self, ApplyError> {
        let dir = instance.state_dir();
        std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let path = dir.join("overseer.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io_err(&path, e))?;

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(ApplyError::Busy),
            Err(TryLockError::Error(e)) => Err(io_err(&path, e).into()),
        }
    }
}
