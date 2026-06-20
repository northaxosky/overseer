//! User-space virtual file system deployment backend - not yet implemented

use super::{
    DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, ProgressSink, ReversalReport,
    VerifyReport,
};

/// Stub for the user space virtual file system backend - not yet implemented
#[derive(Debug, Default, Clone)]
pub struct UsvfsDeployer;

impl UsvfsDeployer {
    pub fn new() -> Self {
        Self
    }
}

impl Deployer for UsvfsDeployer {
    fn kind(&self) -> DeployerKind {
        DeployerKind::Usvfs
    }

    fn check_supported(&self, _plan: &DeployPlan) -> Result<(), DeployError> {
        todo!("USVFS deployment is not yet implemented")
    }

    fn deploy(
        &self,
        _record: &DeployRecord,
        _progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        todo!("USVFS deployment is not yet implemented")
    }

    fn undeploy(&self, _record: &DeployRecord, _progress: &dyn ProgressSink) -> ReversalReport {
        todo!("USVFS deployment is not yet implemented")
    }

    fn verify(&self, _record: &DeployRecord) -> VerifyReport {
        todo!("USVFS deployment is not yet implemented")
    }
}
