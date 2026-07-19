//! Background deployment preparation and launch job

use super::super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
};
use super::super::runner::{BackgroundJob, OperationReporter};
use overseer_core::apply;
use overseer_core::instance::Instance;
use overseer_core::launch::{self, RedeployToken};

#[derive(Debug)]
pub(crate) struct PrepareLaunchJob {
    tool_key: String,
    tool_name: String,
    consent: Option<RedeployToken>,
}

impl PrepareLaunchJob {
    /// Capture the exact launch request selected by the user
    pub(crate) fn new(tool_key: String, tool_name: String, consent: Option<RedeployToken>) -> Self {
        Self {
            tool_key,
            tool_name,
            consent,
        }
    }
}

impl BackgroundJob for PrepareLaunchJob {
    const KIND: OperationKind = OperationKind::PrepareLaunch;

    /// Ensure the captured profile is current, then spawn the captured tool
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::PreparingLaunch);
        let instance = Instance::load(context.instance_root.clone()).map_err(|error| {
            OperationFailure::new(format!("Could not load instance for launch: {error}"))
        })?;
        let progress = reporter.progress_sink();
        let outcome = launch::prepare_and_launch(
            &instance,
            &context.profile,
            &self.tool_key,
            self.consent,
            &progress,
        )
        .map_err(|error| {
            OperationFailure::with_deployment_recovery(format!("Launch failed: {error}"), &instance)
        })?;

        reporter.phase(OperationPhase::Finalizing);
        let status = apply::status(&instance).ok().flatten();
        Ok(OperationOutput::PrepareLaunch {
            outcome,
            instance,
            tool_key: self.tool_key,
            tool_name: self.tool_name,
            status,
        })
    }
}
