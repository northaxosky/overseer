//! Deployment manifest types for tracking and verifying deployed files.

use super::{Deployer, HardlinkDeployer, UsvfsDeployer};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Identifies which deployment backend produced a manifest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeployerKind {
    /// NTFS hard links
    #[default]
    HardLink,
    /// TODO: User-space virtual filesystem (MO2)
    Usvfs,
}

impl std::fmt::Display for DeployerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            DeployerKind::HardLink => "HardLink Deployer",
            DeployerKind::Usvfs => "USVFS Deployer",
        };
        f.write_str(name)
    }
}

/// Construct the deployment backend for a [`DeployerKind`]
pub fn deployer_for(kind: DeployerKind) -> Box<dyn Deployer> {
    match kind {
        DeployerKind::HardLink => Box::new(HardlinkDeployer::new()),
        DeployerKind::Usvfs => Box::new(UsvfsDeployer::new()),
    }
}

/// Record of what a deployment actually wrote, so it can be reversed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployManifest {
    pub deployer: DeployerKind,
    pub target_root: Utf8PathBuf,

    /// Relative paths that were deployed, in order
    pub files: Vec<Utf8PathBuf>,
    /// Directories created under the target root, so they can be removed in reverse
    pub created_dirs: Vec<Utf8PathBuf>,
}

/// Result of checking that a manifest's files are still present on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub expected: usize,
    pub missing: Vec<Utf8PathBuf>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_report_is_ok_only_when_nothing_missing() {
        let ok = VerifyReport {
            expected: 3,
            missing: vec![],
        };
        assert!(ok.is_ok());

        let bad = VerifyReport {
            expected: 3,
            missing: vec![Utf8PathBuf::from("x.txt")],
        };
        assert!(!bad.is_ok());
    }

    #[test]
    fn manifest_survives_json_round_trip() {
        let manifest = DeployManifest {
            deployer: DeployerKind::HardLink,
            target_root: Utf8PathBuf::from("C:/Game/Data"),
            files: vec![Utf8PathBuf::from("Textures/x.dds")],
            created_dirs: vec![Utf8PathBuf::from("Textures")],
        };
        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: DeployManifest = serde_json::from_str(&json).expect("deserialize");

        // DeployManifest has no PartialEq, so compare via re-serialization.
        assert_eq!(json, serde_json::to_string(&back).expect("reserialize"));
        assert_eq!(back.deployer, DeployerKind::HardLink);
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.created_dirs.len(), 1);
    }

    #[test]
    fn deployer_kind_serializes_as_its_variant_name() {
        let json = serde_json::to_string(&DeployerKind::HardLink).expect("serialize");
        assert_eq!(json, "\"HardLink\"");
    }

    #[test]
    fn factory_builds_a_backend_for_each_kind() {
        assert_eq!(
            deployer_for(DeployerKind::HardLink).kind(),
            DeployerKind::HardLink
        );
        assert_eq!(
            deployer_for(DeployerKind::Usvfs).kind(),
            DeployerKind::Usvfs
        );
    }

    #[test]
    fn deployer_kind_display_is_human_readable() {
        assert_eq!(DeployerKind::HardLink.to_string(), "HardLink Deployer");
        assert_eq!(DeployerKind::Usvfs.to_string(), "USVFS Deployer");
    }
}
