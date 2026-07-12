//! Background Downloads refresh

use overseer_core::install;

use super::super::protocol::{OperationContext, OperationFailure, OperationOutput, OperationPhase};
use super::super::runner::{BackgroundJob, OperationReporter};

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RefreshDownloadsJob;

impl BackgroundJob for RefreshDownloadsJob {
    /// Reload the instance and list its download archives
    fn run(
        self: Box<Self>,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::ListingDownloads);
        let instance = overseer_core::instance::Instance::load(context.instance_root.clone())
            .map_err(|error| {
                OperationFailure::new(format!("Could not load instance for downloads: {error}"))
            })?;
        let downloads = install::list_downloads(&instance)
            .map_err(|error| OperationFailure::new(format!("Could not list downloads: {error}")))?;
        Ok(OperationOutput::RefreshDownloads(downloads))
    }
}
