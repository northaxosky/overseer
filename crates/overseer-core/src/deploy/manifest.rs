//! Deployment manifest types for tracking and verifying deployed files.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Identifies which deployment backend produced a manifest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeployerKind {
    /// NTFS hard links
    HardLink,
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
}
