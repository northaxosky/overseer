//! Background archive installation

use super::super::protocol::{
    InstallState, OperationContext, OperationFailure, OperationKind, OperationOutput,
    OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use crate::app::Session;
use overseer_core::install::{self, InstallError};
use overseer_core::instance::Instance;
use overseer_core::lifecycle::{self, LifecycleError};

#[derive(Debug)]
pub(crate) struct InstallJob {
    archive: String,
    name: String,
}

impl InstallJob {
    /// Capture the archive and destination name for worker execution
    pub(crate) fn new(archive: String, name: String) -> Self {
        Self { archive, name }
    }

    /// Report a committed install without reading guarded state
    fn committed_with_residue(
        self,
        path: camino::Utf8PathBuf,
    ) -> Result<OperationOutput, OperationFailure> {
        Ok(OperationOutput::Install {
            name: self.name,
            state: InstallState::CommittedWithResidue(path),
        })
    }
}

impl BackgroundJob for InstallJob {
    const KIND: OperationKind = OperationKind::Install;

    /// Install through the guarded lifecycle and refresh owned results
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::with_session_recovery(
                format!("Could not load instance for install: {error}"),
                context,
            )
        })?;
        reporter.phase(OperationPhase::ExtractingArchive);

        let report = lifecycle::install(&instance, &self.archive, &self.name).map_err(|error| {
            let message = match error {
                LifecycleError::Install(InstallError::Fomod) => {
                    "FOMOD installers aren't supported yet".to_owned()
                }
                error => format!("Install failed: {error}"),
            };

            OperationFailure::with_session_recovery(message, context)
        })?;
        if let Some(path) = report.residue_warning {
            return self.committed_with_residue(path);
        }
        let committed = format!("Installed {}", self.name);

        reporter.phase(OperationPhase::ReloadingSession);
        let session = Session::load(&context.instance_root, &context.profile).map_err(|error| {
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

        Ok(OperationOutput::Install {
            name: self.name,
            state: InstallState::Refreshed {
                session: Box::new(session),
                downloads,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/install.rs"]
mod tests;
