//! Deployment backend trait and types.

use super::{
    DeployError, DeployPlan, DeployRecord, DeployerKind, ProgressSink, ReversalReport, VerifyReport,
};
use camino::Utf8PathBuf;

/// A fully resolved thing to run: program, arguments, and directory
#[derive(Debug, Clone)]
pub struct LaunchTarget {
    pub program: Utf8PathBuf,
    pub args: Vec<String>,
    pub working_dir: Utf8PathBuf,
}

/// A mod deployment backend
pub trait Deployer {
    /// Which backend this is (used for journaling and display).
    fn kind(&self) -> DeployerKind;

    /// Check whether this deployer can satisfy the plan
    fn check_supported(&self, plan: &DeployPlan) -> Result<(), DeployError>;

    /// Deploy every entry in the record, backing up any pre-existing files beforehand
    fn deploy(&self, record: &DeployRecord, progress: &dyn ProgressSink)
    -> Result<(), DeployError>;

    /// Reverse the deployment described by `record`, restoring target to its pre-deploy state
    fn undeploy(&self, record: &DeployRecord, progress: &dyn ProgressSink) -> ReversalReport;

    /// Check that every entry recorded is still present on disk
    fn verify(&self, record: &DeployRecord) -> VerifyReport;

    /// Run `target` with the instance's mods visible to it
    fn launch(&self, target: &LaunchTarget) -> Result<(), DeployError>;
}
