//! Transactional installed-mod rename and recovery

mod error;
mod journal;
mod recovery;
mod rename;

use crate::apply::Deployment;
use crate::deploy::NullSink;
use crate::instance::Instance;
use crate::lock::InstanceLock;
use camino::{Utf8Path, Utf8PathBuf};

pub use error::{CleanupWarning, LifecycleError};
pub(crate) use recovery::recover_locked;

use rename::rename_locked;

const WORK: &str = "lifecycle-work";
const MARKER_PREFIX: &str = ".overseer-lifecycle.";

/// A committed lifecycle result and any deferred cleanup
#[derive(Debug)]
pub struct LifecycleOutcome<T> {
    pub report: T,
    pub cleanup_warning: Option<CleanupWarning>,
}

/// Names changed by a committed rename
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameReport {
    pub old: String,
    pub new: String,
}

/// Rename an installed mod and matching managed profile rows transactionally
pub fn rename_mod(
    instance: &Instance,
    old: &str,
    new: &str,
) -> Result<LifecycleOutcome<RenameReport>, LifecycleError> {
    let _lock = InstanceLock::acquire(instance)?;
    recover_locked(instance)?;

    crate::apply::recover_if_needed_locked(instance, &NullSink)
        .map_err(|error| LifecycleError::Deployment(Box::new(error)))?;

    if Deployment::exists(instance) {
        return Err(LifecycleError::LiveDeployment {
            path: Deployment::path(instance),
        });
    }

    match rename_locked(instance, old, new) {
        Ok(result) => Ok(result),
        #[cfg(test)]
        Err(error) if matches!(error, LifecycleError::TestCrash) => Err(error),
        Err(error) => recover_locked(instance).map_or_else(Err, |_| Err(error)),
    }
}

fn work_path(instance: &Instance) -> Utf8PathBuf {
    instance.state_dir().join(WORK)
}

fn modlist(instance: &Instance, profile: &str) -> Utf8PathBuf {
    instance.profile_dir(profile).join("modlist.txt")
}

fn exists(path: &Utf8Path) -> Result<bool, LifecycleError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::error::io_err(path, error).into()),
    }
}

fn marker_path(root: &Utf8Path, transaction: &str) -> Utf8PathBuf {
    root.join(format!("{MARKER_PREFIX}{transaction}"))
}

fn conflict<T>(path: Utf8PathBuf) -> Result<T, LifecycleError> {
    Err(LifecycleError::RecoveryConflict { paths: vec![path] })
}

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod tests;
