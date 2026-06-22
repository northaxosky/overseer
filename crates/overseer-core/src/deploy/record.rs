//! Plan-derived record of a deployment transaction, acts as the source for reversing it

use super::error::io_err;
use super::{DeployError, DeployPlan, Deployer, HardlinkDeployer, ProjFsDeployer, UsvfsDeployer};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Identifies which deployment backend owns a record
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeployerKind {
    /// NTFS hard links
    #[default]
    HardLink,
    /// TODO: User space virtual filesystem (MO2)
    Usvfs,
    /// TODO: ProjFS
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
        DeployerKind::Usvfs => Box::new(UsvfsDeployer::new()),
        DeployerKind::ProjFs => Box::new(ProjFsDeployer::new()),
    }
}

/// One file to deploy: where it lands, and the source it is linked from
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployEntry {
    /// Path relative to the target root
    pub relative: Utf8PathBuf,
    /// Absolute path to the source file in the winning mod's staging dir
    pub source: Utf8PathBuf,
    /// Whether a real file already occupied this destination
    pub preexisting: bool,
}

/// The authoritative record of a deployment transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRecord {
    /// Backend that produced the transaction
    pub deployer: DeployerKind,
    /// Directory the entries are deployed into
    pub target_root: Utf8PathBuf,
    /// Directory pre-existing files are moved aside to
    pub backup_root: Utf8PathBuf,
    /// Files to deploy, in order
    pub entries: Vec<DeployEntry>,
    /// Directories that must be created under the target root
    pub created_dirs: Vec<Utf8PathBuf>,
}

impl DeployRecord {
    /// Derive a record from a plan
    pub fn from_plan(
        plan: &DeployPlan,
        backup_root: impl Into<Utf8PathBuf>,
        kind: DeployerKind,
    ) -> Result<Self, DeployError> {
        let target_root = plan.target_root.clone();
        let mut entries = Vec::with_capacity(plan.len());
        let mut created_dirs = Vec::new();
        let mut seen: BTreeSet<Utf8PathBuf> = BTreeSet::new();

        for file in plan.files() {
            let relative = file.relative.clone();

            // Prob once to see if dest is occupied
            let dest = target_root.join(&relative);
            let preexisting = match dest.symlink_metadata() {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(io_err(&dest, e)),
            };
            entries.push(DeployEntry {
                relative,
                source: file.source.clone(),
                preexisting,
            });
            if let Some(parent) = file.relative.parent() {
                collect_missing_dirs(&target_root, parent, &mut seen, &mut created_dirs)?;
            }
        }

        Ok(Self {
            deployer: kind,
            target_root,
            backup_root: backup_root.into(),
            entries,
            created_dirs,
        })
    }
}

/// Record each ancestor of `relative_dir` that does not yet exist
fn collect_missing_dirs(
    target_root: &Utf8Path,
    relative_dir: &Utf8Path,
    seen: &mut BTreeSet<Utf8PathBuf>,
    created_dirs: &mut Vec<Utf8PathBuf>,
) -> Result<(), DeployError> {
    let mut current = Utf8PathBuf::new();
    for component in relative_dir.components() {
        current.push(component.as_str());
        if !seen.insert(current.clone()) {
            continue;
        }
        let abs = target_root.join(&current);
        match abs.symlink_metadata() {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                created_dirs.push(current.clone());
            }
            Err(e) => return Err(io_err(&abs, e)),
        }
    }
    Ok(())
}

/// Result of checking that a record's files are still present on disk
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

/// Outcome of reversing a transaction
#[derive(Debug, Default)]
pub struct ReversalReport {
    /// Paths the reversal could not bring back
    pub unresolved: Vec<DeployError>,
}

impl ReversalReport {
    pub fn is_fully_resolved(&self) -> bool {
        self.unresolved.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::ModSource;
    use tempfile::TempDir;

    fn temp() -> (TempDir, Utf8PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let base = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 temp path");
        (dir, base)
    }

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parents");
        }
        std::fs::write(path, contents).expect("write file");
    }

    #[test]
    fn vfs_stub_launch_is_unsupported() {
        use crate::deploy::LaunchTarget;
        let target = LaunchTarget {
            program: Utf8PathBuf::from("x.exe"),
            args: vec![],
            working_dir: Utf8PathBuf::from("."),
        };
        for kind in [DeployerKind::Usvfs, DeployerKind::ProjFs] {
            let err = deployer_for(kind)
                .launch(&target)
                .expect_err("stub launch must be unsupported");
            assert!(
                matches!(err, DeployError::Unsupported { deployer } if deployer == kind),
                "{kind:?} should report its own kind"
            );
        }
    }

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
    fn reversal_report_is_resolved_only_when_empty() {
        assert!(ReversalReport::default().is_fully_resolved());
        let bad = ReversalReport {
            unresolved: vec![DeployError::NonUtf8Path("x".into())],
        };
        assert!(!bad.is_fully_resolved());
    }

    #[test]
    fn record_survives_json_round_trip() {
        let record = DeployRecord {
            deployer: DeployerKind::HardLink,
            target_root: Utf8PathBuf::from("C:/Game/Data"),
            backup_root: Utf8PathBuf::from("C:/Game/.overseer-backup"),
            entries: vec![DeployEntry {
                relative: Utf8PathBuf::from("Textures/x.dds"),
                source: Utf8PathBuf::from("C:/mods/M/Textures/x.dds"),
                preexisting: false,
            }],
            created_dirs: vec![Utf8PathBuf::from("Textures")],
        };
        let json = serde_json::to_string(&record).expect("serialize");
        let back: DeployRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.deployer, DeployerKind::HardLink);
        assert_eq!(back.entries, record.entries);
        assert_eq!(back.created_dirs, record.created_dirs);
        assert_eq!(back.backup_root, record.backup_root);
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

    #[test]
    fn from_plan_copies_every_planned_file_as_an_entry() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("a.txt"), "a");
        write(&m.join("sub/b.txt"), "b");
        let data = base.join("Data");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
            .expect("record");
        assert_eq!(record.target_root, data);
        assert_eq!(record.entries.len(), 2);
        for entry in &record.entries {
            assert!(entry.source.starts_with(&m));
        }
    }

    #[test]
    fn from_plan_records_only_dirs_that_do_not_yet_exist() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("a/b/c.txt"), "deep");
        let data = base.join("Data");
        std::fs::create_dir_all(data.join("a")).expect("pre-existing dir");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
            .expect("record");
        assert_eq!(record.created_dirs, vec![Utf8PathBuf::from("a/b")]);
    }

    #[test]
    fn from_plan_orders_created_dirs_outermost_first_without_duplicates() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("a/b/one.txt"), "1");
        write(&m.join("a/b/two.txt"), "2");
        let data = base.join("Data");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
            .expect("record");
        assert_eq!(
            record.created_dirs,
            vec![Utf8PathBuf::from("a"), Utf8Path::new("a").join("b")]
        );
    }

    #[test]
    fn from_plan_records_no_dirs_for_top_level_files() {
        let (_tmp, base) = temp();
        let m = base.join("mods/A");
        write(&m.join("root.txt"), "r");
        let data = base.join("Data");
        let plan = DeployPlan::from_mods(&data, &[ModSource::new("A", &m)]).expect("plan");
        let record = DeployRecord::from_plan(&plan, base.join(".backup"), DeployerKind::HardLink)
            .expect("record");
        assert!(record.created_dirs.is_empty());
    }
}
