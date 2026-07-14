//! Background deployment and purge jobs

use super::super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use overseer_core::apply;
use overseer_core::instance::Instance;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeployJob;

impl BackgroundJob for DeployJob {
    const KIND: OperationKind = OperationKind::Deploy;

    /// Reload the captured instance and deploy its active profile
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::PlanningDeploy);

        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for deploy: {error}"))
        })?;
        let progress = reporter.progress_sink();

        let deployment =
            apply::deploy_profile(&instance, &context.profile, &progress).map_err(|error| {
                OperationFailure::with_deployment_recovery(
                    format!("Deploy failed: {error}"),
                    &instance,
                )
            })?;

        let files = deployment.record.entries.len();
        reporter.phase(OperationPhase::Finalizing);

        let status = apply::status(&instance).map_err(|error| {
            OperationFailure::new(format!(
                "Deploy completed, but status refresh failed: {error}"
            ))
        })?;

        Ok(OperationOutput::Deploy { status, files })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PurgeJob;

impl BackgroundJob for PurgeJob {
    const KIND: OperationKind = OperationKind::Purge;

    /// Reload the captured instance and purge its live deployment
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::PreparingPurge);

        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for purge: {error}"))
        })?;
        let progress = reporter.progress_sink();

        let outcome = apply::purge(&instance, &progress).map_err(|error| {
            OperationFailure::with_deployment_recovery(format!("Purge failed: {error}"), &instance)
        })?;

        reporter.phase(OperationPhase::Finalizing);
        let status = apply::status(&instance).map_err(|error| {
            OperationFailure::new(format!(
                "Purge completed, but status refresh failed: {error}"
            ))
        })?;

        Ok(OperationOutput::Purge { status, outcome })
    }
}

#[cfg(test)]
#[path = "tests/deployment.rs"]
mod tests;
