//! Installed-mod lifecycle operations with deterministic in-process rollback

mod archive;
mod bundle;
mod error;
mod install;
mod remove;
mod replace;

use camino::Utf8PathBuf;

use crate::apply::{Deployment, InstanceLock};
use crate::instance::{Instance, validate_mod_name};

pub use error::LifecycleError;
pub use install::install;
pub use remove::remove;
pub use replace::{reinstall, replace};

/// Outcome of a completed installed-mod lifecycle operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleReport {
    /// Actual installed mod name
    pub name: String,
    /// Relevant archive basename when the operation uses one
    pub archive: Option<String>,
    /// Pending bundle left behind when success cleanup fails
    pub residue_warning: Option<Utf8PathBuf>,
}

/// Acquire the shared lock and enforce fixed-path lifecycle guards
fn enter(instance: &Instance) -> Result<InstanceLock, LifecycleError> {
    let lock = InstanceLock::acquire(instance)?;
    let pending = bundle::path(instance);
    if bundle::occupied(&pending)? {
        return Err(LifecycleError::PendingOperation { path: pending });
    }
    let deployment = Deployment::path(instance);
    if bundle::occupied(&deployment)? {
        return Err(LifecycleError::DeploymentExists { path: deployment });
    }
    Ok(lock)
}

/// Resolve an installed directory case-insensitively after validating the request
fn find_installed(instance: &Instance, requested: &str) -> Result<Option<String>, LifecycleError> {
    validate_mod_name(requested)?;
    Ok(instance
        .installed_mods()?
        .into_iter()
        .find(|installed| installed.name.eq_ignore_ascii_case(requested))
        .map(|installed| installed.name))
}

/// Clean a successful rollback or report every retained inverse failure
fn finish_rollback(
    bundle: Utf8PathBuf,
    initiating: LifecycleError,
    mut failures: Vec<String>,
) -> LifecycleError {
    if failures.is_empty()
        && let Err(error) = bundle::cleanup(&bundle)
    {
        failures.push(format!("remove rollback bundle `{bundle}`: {error}"));
    }
    if failures.is_empty() {
        return initiating;
    }
    let mut issues = vec![format!("initiating error: {initiating}")];
    issues.extend(failures);
    LifecycleError::RollbackIncomplete { bundle, issues }
}

#[cfg(test)]
mod failpoint;

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod tests;
