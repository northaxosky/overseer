//! Background Saves refresh

use overseer_core::instance::Instance;
use overseer_core::saves;

use super::super::protocol::{OperationContext, OperationFailure, OperationOutput, OperationPhase};
use super::super::runner::{BackgroundJob, OperationReporter};

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RefreshSavesJob;

impl BackgroundJob for RefreshSavesJob {
    /// Reload the instance and parse the profile's save headers
    fn run(
        self: Box<Self>,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::ReadingSaves);

        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for saves: {error}"))
        })?;

        let dir = instance
            .saves_dir(&context.profile)
            .map_err(|error| OperationFailure::new(format!("Could not locate saves: {error}")))?;

        let entries = saves::list_saves(&dir, instance.config.game)
            .map_err(|error| OperationFailure::new(format!("Could not list saves: {error}")))?;

        Ok(OperationOutput::RefreshSaves(entries))
    }
}

#[cfg(test)]
#[path = "tests/saves.rs"]
mod tests;
