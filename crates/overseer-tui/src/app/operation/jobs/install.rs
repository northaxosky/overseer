//! Background archive installation

use super::super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use crate::app::Session;
use camino::Utf8PathBuf;
use overseer_core::apply;
use overseer_core::install::{self, InstallError};
use overseer_core::instance::Instance;

#[derive(Debug)]
pub(crate) struct InstallJob {
    archive: Utf8PathBuf,
    name: String,
}

impl InstallJob {
    /// Capture the archive and destination name for worker execution
    pub(crate) fn new(archive: Utf8PathBuf, name: String) -> Self {
        Self { archive, name }
    }
}

impl BackgroundJob for InstallJob {
    const KIND: OperationKind = OperationKind::Install;

    /// Recheck deployment safety, install the archive, and refresh owned results
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
        let status = apply::status(&instance).map_err(|error| {
            OperationFailure::with_session_recovery(
                format!("Could not check deployment status before install: {error}"),
                context,
            )
        })?;

        if status.is_some() {
            return Err(OperationFailure::with_session_recovery(
                "Cannot install while a deployment is live; purge it first",
                context,
            ));
        }
        reporter.phase(OperationPhase::ExtractingArchive);

        install::install(&instance, &self.archive, &self.name).map_err(|error| {
            let message = match error {
                InstallError::Fomod => "FOMOD installers aren't supported yet".to_owned(),
                error => {
                    format!("Install failed: {error}")
                }
            };

            OperationFailure::with_session_recovery(message, context)
        })?;

        reporter.phase(OperationPhase::ReloadingSession);
        let session = Session::load(&context.instance_root, &context.profile).map_err(|error| {
            OperationFailure::with_session_recovery(
                format!("Installed {}, but reloading failed: {error}", self.name),
                context,
            )
        })?;

        reporter.phase(OperationPhase::ListingDownloads);
        let downloads = match install::list_downloads(&session.instance) {
            Ok(downloads) => downloads,
            Err(error) => {
                return Err(OperationFailure::with_session(
                    format!(
                        "Installed {}, but downloads refresh failed: {error}",
                        self.name
                    ),
                    session,
                ));
            }
        };

        Ok(OperationOutput::Install {
            session: Box::new(session),
            name: self.name,
            downloads,
        })
    }
}

#[cfg(test)]
#[path = "tests/install.rs"]
mod tests;
