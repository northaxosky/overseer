//! Deployment backend trait and types.

use super::{
    DeployError, DeployPlan, DeployRecord, HardlinkDeployer, ProgressSink, ReversalReport,
    VerifyReport,
};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// A fully resolved thing to run: program, arguments, and directory
#[derive(Debug, Clone)]
pub struct LaunchTarget {
    pub program: Utf8PathBuf,
    pub args: Vec<String>,
    pub working_dir: Utf8PathBuf,
}

/// A mod deployment backend
pub trait Deployer {
    /// Which backend this is (used for journaling and display)
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

/// Identifies which deployment backend owns a record
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeployerKind {
    /// NTFS hard links
    #[default]
    HardLink,
    /// User-space virtual filesystem backend (planned)
    Usvfs,
    /// Windows Projected File System backend (planned)
    ProjFs,
}

impl std::fmt::Display for DeployerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::HardLink => "HardLink Deployer",
            Self::Usvfs => "USVFS Deployer",
            Self::ProjFs => "ProjFS Deployer",
        };
        f.write_str(name)
    }
}

/// Construct the deployment backend for a [`DeployerKind`]
pub fn deployer_for(kind: DeployerKind) -> Box<dyn Deployer> {
    match kind {
        DeployerKind::HardLink => Box::new(HardlinkDeployer::new()),
        DeployerKind::Usvfs | DeployerKind::ProjFs => Box::new(StubDeployer::new(kind)),
    }
}

/// Unimplemented backend: operations report [`DeployError::Unsupported`]; `verify` treats every entry as missing
#[derive(Debug, Clone)]
pub(crate) struct StubDeployer {
    kind: DeployerKind,
}

impl StubDeployer {
    fn new(kind: DeployerKind) -> Self {
        Self { kind }
    }

    fn unsupported(&self) -> DeployError {
        DeployError::Unsupported {
            deployer: self.kind,
        }
    }
}

impl Deployer for StubDeployer {
    fn kind(&self) -> DeployerKind {
        self.kind
    }

    fn check_supported(&self, _plan: &DeployPlan) -> Result<(), DeployError> {
        Err(self.unsupported())
    }

    fn deploy(
        &self,
        _record: &DeployRecord,
        _progress: &dyn ProgressSink,
    ) -> Result<(), DeployError> {
        Err(self.unsupported())
    }

    fn undeploy(&self, _record: &DeployRecord, _progress: &dyn ProgressSink) -> ReversalReport {
        ReversalReport {
            unresolved: vec![self.unsupported()],
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

    fn launch(&self, _target: &LaunchTarget) -> Result<(), DeployError> {
        Err(self.unsupported())
    }
}
