//! Background setup diagnostics

use super::super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use overseer_core::instance::Instance;
use overseer_diagnostics::diagnose;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DoctorJob;

impl BackgroundJob for DoctorJob {
    const KIND: OperationKind = OperationKind::Doctor;

    /// Reload the captured instance and diagnose its active profile
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::RunningDiagnostics);
        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for diagnostics: {error}"))
        })?;

        let report = diagnose(&instance, &context.profile)
            .map_err(|error| OperationFailure::new(format!("Diagnostics failed: {error}")))?;
        Ok(OperationOutput::Doctor(report))
    }
}

#[cfg(test)]
#[path = "tests/doctor.rs"]
mod tests;
