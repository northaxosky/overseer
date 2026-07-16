//! Background mod removal

use super::super::protocol::{
    LifecycleState, OperationContext, OperationFailure, OperationKind, OperationOutput,
    OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use crate::app::Session;
use camino::Utf8PathBuf;
use overseer_core::install;
use overseer_core::instance::Instance;
use overseer_core::lifecycle::{self, LifecycleError};

#[derive(Debug)]
pub(crate) struct RemoveJob {
    name: String,
}

impl RemoveJob {
    /// Capture the managed mod name for worker execution
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    /// Report a committed removal without reading guarded state
    fn committed_with_residue(
        self,
        path: Utf8PathBuf,
    ) -> Result<OperationOutput, OperationFailure> {
        Ok(OperationOutput::Remove {
            name: self.name,
            state: LifecycleState::CommittedWithResidue(path),
        })
    }
}

impl BackgroundJob for RemoveJob {
    const KIND: OperationKind = OperationKind::Remove;

    /// Remove through the guarded lifecycle and refresh owned results
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::with_session_recovery(
                format!("Could not load instance for removal: {error}"),
                context,
            )
        })?;
        let report = lifecycle::remove(&instance, &self.name).map_err(|error| {
            let message = match error {
                LifecycleError::DeploymentExists { .. } => {
                    format!("Purge the live deployment before removing {}", self.name)
                }
                error => format!("Remove failed: {error}"),
            };
            OperationFailure::with_session_recovery(message, context)
        })?;
        if let Some(path) = report.residue_warning {
            return self.committed_with_residue(path);
        }
        let committed = format!("Removed {}", self.name);

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

        Ok(OperationOutput::Remove {
            name: self.name,
            state: LifecycleState::Refreshed {
                session: Box::new(session),
                downloads,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/remove.rs"]
mod tests;
