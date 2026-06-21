//! ProjFS deployment backend - not yet implemented

use super::{
    DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, ProgressSink, ReversalReport,
    VerifyReport,
};

/// Stub for the ProjFS deployment backend - not yet implemented
#[derive(Debug, Default, Clone)]
pub struct ProjFsDeployer;

impl ProjFsDeployer {
    pub fn new() -> Self {
        Self
    }

    /// Typed error every operation returns until implemented
    fn unsupported() -> DeployError {
        DeployError::Unsupported {
            deployer: DeployerKind::ProjFs,
        }
    }
}

impl Deployer for ProjFsDeployer {
    fn kind(&self) -> DeployerKind {
        DeployerKind::ProjFs
    }

    fn check_supported(&self, _plan: &DeployPlan) -> Result<(), DeployError> {
        Err(Self::unsupported())
    }

    fn deploy(
        &self,
        _record: &DeployRecord,
        _progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        Err(Self::unsupported())
    }

    fn undeploy(&self, _record: &DeployRecord, _progress: &dyn ProgressSink) -> ReversalReport {
        ReversalReport {
            unresolved: vec![Self::unsupported()],
        }
    }

    fn verify(&self, record: &DeployRecord) -> VerifyReport {
        VerifyReport {
            expected: record.entries.len(),
            missing: record
                .entries
                .iter()
                .map(|entry| entry.relative.clone())
                .collect(),
        }
    }
}
