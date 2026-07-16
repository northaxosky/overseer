//! Background mod replacement

use super::super::protocol::{
    LifecycleState, OperationContext, OperationFailure, OperationKind, OperationOutput,
    OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use crate::app::Session;
use camino::Utf8PathBuf;
use overseer_core::install::{self, InstallError};
use overseer_core::instance::Instance;
use overseer_core::lifecycle::{self, LifecycleError};

#[derive(Debug)]
pub(crate) struct ReplaceJob {
    name: String,
    archive: String,
}

impl ReplaceJob {
    /// Capture the managed mod and archive names for worker execution
    pub(crate) fn new(name: String, archive: String) -> Self {
        Self { name, archive }
    }

    /// Report a committed replacement without reading guarded state
    fn committed_with_residue(
        self,
        path: Utf8PathBuf,
    ) -> Result<OperationOutput, OperationFailure> {
        Ok(OperationOutput::Replace {
            name: self.name,
            state: LifecycleState::CommittedWithResidue(path),
        })
    }
}

impl BackgroundJob for ReplaceJob {
    const KIND: OperationKind = OperationKind::Replace;

    /// Replace through the guarded lifecycle and refresh owned results
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::with_session_recovery(
                format!("Could not load instance for replacement: {error}"),
                context,
            )
        })?;
        reporter.phase(OperationPhase::ExtractingArchive);
        let report = lifecycle::replace(&instance, &self.name, &self.archive).map_err(|error| {
            let message = match error {
                LifecycleError::DeploymentExists { .. } => {
                    format!("Purge the live deployment before replacing {}", self.name)
                }
                LifecycleError::Install(InstallError::Fomod) => {
                    "FOMOD installers aren't supported yet".to_owned()
                }
                error => format!("Replace failed: {error}"),
            };
            OperationFailure::with_session_recovery(message, context)
        })?;
        if let Some(path) = report.residue_warning {
            return self.committed_with_residue(path);
        }
        let committed = format!("Replaced {}", self.name);

        reporter.phase(OperationPhase::ReloadingSession);
        let session =
            Session::load(&context.instance_root, Some(&context.profile)).map_err(|error| {
                OperationFailure::with_session_recovery(
                    format!("{committed}, but reloading failed: {error}"),
                    context,
                )
            })?;

        reporter.phase(OperationPhase::ListingDownloads);
        let downloads = match install::list_downloads(&session.instance) {
            Ok(downloads) => downloads,
            Err(error) => {
                return Err(OperationFailure::with_session(
                    format!("{committed}, but downloads refresh failed: {error}"),
                    session,
                ));
            }
        };

        Ok(OperationOutput::Replace {
            name: self.name,
            state: LifecycleState::Refreshed {
                session: Box::new(session),
                downloads,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/replace.rs"]
mod tests;
