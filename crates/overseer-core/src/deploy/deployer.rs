//! Deployment backend trait and types.

use super::{DeployError, DeployManifest, DeployPlan, DeployerKind, ProgressSink, VerifyReport};

/// A mod deployment backend
pub trait Deployer {
    fn kind(&self) -> DeployerKind;

    /// Check whether this deployer can satisfy the plan. Called automatically by [`Deployer::deploy`]
    fn check_supported(&self, plan: &DeployPlan) -> Result<(), DeployError>;

    /// Deploy every file in the plan, returning a manifest describing what was written
    fn deploy(
        &self,
        plan: &DeployPlan,
        progress: &dyn ProgressSink,
    ) -> Result<DeployManifest, DeployError>;

    /// Undeploy every file in the plan
    fn undeploy(
        &self,
        manifest: &DeployManifest,
        progress: &dyn ProgressSink,
    ) -> Result<(), DeployError>;

    /// Check that every file recorded in the manifest is still present
    fn verify(&self, manifest: &DeployManifest) -> VerifyReport;
}
