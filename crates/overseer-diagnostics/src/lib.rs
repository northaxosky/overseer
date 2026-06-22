//! Setup health checks for an Overseer instance — a clean-room reimplementation of the
//! Collective Modding Toolkit's diagnostics.
//!
//! The model is: gather a [`GameContext`] once, run each [`Check`] against it, and collect
//! the resulting [`Finding`]s into a [`Report`]. Checks are pure functions of the context,
//! so they unit-test without touching the filesystem.

mod checks;
mod context;
mod error;
mod finding;
mod report;

pub use checks::Check;
pub use context::{DataFile, GameContext};
pub use error::DiagnosticError;
pub use finding::{Finding, Severity};
pub use report::Report;

use overseer_core::instance::Instance;

/// Gather the context for a profile and run every registered check
pub fn diagnose(instance: &Instance, profile: &str) -> Result<Report, DiagnosticError> {
    let ctx = GameContext::gather(instance, profile)?;
    let findings = checks::all().iter().flat_map(|c| c.run(&ctx)).collect();
    Ok(Report::new(findings))
}
