//! Installed-mod lifecycle operations with deterministic in-process rollback

mod bundle;
mod error;
mod remove;

use camino::Utf8PathBuf;

pub use error::LifecycleError;
pub use remove::remove;

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

#[cfg(test)]
mod failpoint;

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod tests;
