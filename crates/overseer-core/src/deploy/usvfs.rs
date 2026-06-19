//! User-space virtual file system deployment backend - not yet implemented

use super::{
    DeployError, DeployManifest, DeployPlan, Deployer, DeployerKind, ProgressSink, VerifyReport,
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
        _plan: &DeployPlan,
        _progress: &dyn ProgressSink,
    ) -> Result<DeployManifest, DeployError> {
        todo!("USVFS deployment is not yet implemented")
    }

    fn undeploy(
        &self,
        _manifest: &DeployManifest,
        _progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        todo!("USVFS deployment is not yet implemented")
    }

    fn verify(&self, _manifest: &DeployManifest) -> VerifyReport {
        todo!("USVFS deployment is not yet implemented")
    }
}
