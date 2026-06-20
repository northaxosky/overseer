//! User-space virtual file system deployment backend - not yet implemented.

use super::{
    DeployError, DeployPlan, DeployRecord, Deployer, DeployerKind, ProgressSink, ReversalReport,
    VerifyReport,
};

/// Stub for the user space virtual file system backend - not yet implemented.
#[derive(Debug, Default, Clone)]
pub struct UsvfsDeployer;

impl UsvfsDeployer {
    pub fn new() -> Self {
        Self
    }

    /// The single typed error every operation returns, so selecting a `Usvfs`
    /// deployer can never panic (which would make a crash-recovery loop fatal).
    fn unsupported(&self) -> DeployError {
        DeployError::Unsupported {
            deployer: DeployerKind::Usvfs,
        }
    }
}

impl Deployer for UsvfsDeployer {
    fn kind(&self) -> DeployerKind {
        DeployerKind::Usvfs
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
        // Surface the failure through the report so recovery records it as
        // unresolved (and keeps the journal) instead of panicking.
        ReversalReport {
            unresolved: vec![self.unsupported()],
        }
    }

    fn verify(&self, record: &DeployRecord) -> VerifyReport {
        // An unimplemented backend has deployed nothing; report every entry missing.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::NullSink;
    use camino::Utf8PathBuf;

    fn empty_record() -> DeployRecord {
        DeployRecord {
            deployer: DeployerKind::Usvfs,
            target_root: Utf8PathBuf::from("C:/Game/Data"),
            backup_root: Utf8PathBuf::from("C:/Game/.overseer-backup"),
            entries: Vec::new(),
            created_dirs: Vec::new(),
        }
    }

    #[test]
    fn every_operation_reports_unsupported_instead_of_panicking() {
        let deployer = UsvfsDeployer::new();
        let record = empty_record();
        let plan = DeployPlan::from_mods("C:/Game/Data", &[]).expect("empty plan");

        assert!(matches!(
            deployer.check_supported(&plan),
            Err(DeployError::Unsupported { .. })
        ));
        assert!(matches!(
            deployer.deploy(&record, &NullSink),
            Err(DeployError::Unsupported { .. })
        ));

        let report = deployer.undeploy(&record, &NullSink);
        assert!(!report.is_fully_resolved());
        assert!(matches!(
            report.unresolved.first(),
            Some(DeployError::Unsupported { .. })
        ));

        // verify must not panic either.
        assert_eq!(deployer.verify(&record).expected, 0);
    }
}
