//! Background conflict scanning

use super::super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use overseer_core::apply::deploy_sources;
use overseer_core::deploy::ConflictSnapshot;
use overseer_core::instance::{Instance, Profile};

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ScanConflictsJob;

impl BackgroundJob for ScanConflictsJob {
    const KIND: OperationKind = OperationKind::ScanConflicts;

    /// Reload and reconcile the captured profile before scanning every enabled provider
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::ScanningConflicts);

        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for conflicts: {error}"))
        })?;

        let mut profile = Profile::load_existing(&instance, &context.profile).map_err(|error| {
            OperationFailure::new(format!("Could not load profile for conflicts: {error}"))
        })?;

        profile.reconcile(&instance).map_err(|error| {
            OperationFailure::new(format!(
                "Could not reconcile profile for conflicts: {error}"
            ))
        })?;

        let snapshot = ConflictSnapshot::build(&deploy_sources(&instance, &profile))
            .map_err(|error| OperationFailure::new(format!("Could not scan conflicts: {error}")))?;
        Ok(OperationOutput::ScanConflicts(snapshot))
    }
}

#[cfg(test)]
#[path = "tests/conflicts.rs"]
mod tests;
