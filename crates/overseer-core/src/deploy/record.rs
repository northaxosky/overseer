//! Plan-derived record of a deployment transaction, acts as the source for reversing it

use super::error::io_err;
use super::{DeployError, DeployPlan, DeployerKind};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One file to deploy: where it lands, and the source it is linked from
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployEntry {
    /// Path relative to the target root
    pub relative: Utf8PathBuf,
    /// Absolute path to the source file in the winning mod's staging dir
    pub source: Utf8PathBuf,
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
            entries.push(DeployEntry {
                relative: file.relative.clone(),
                source: file.source.clone(),
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
            Err(e) => return Err(io_err(&abs, e).into()),
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
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Outcome of reversing a transaction
#[derive(Debug, Default)]
pub struct ReversalReport {
    /// Owned links removed from the target
    pub removed: Vec<Utf8PathBuf>,
    /// Original files restored from backup
    pub restored: Vec<Utf8PathBuf>,
    /// Foreign paths preserved instead of being removed
    pub preserved_conflicts: Vec<PreservedConflict>,
    /// Paths whose state could not be resolved
    pub unresolved: Vec<ReversalIssue>,
}

impl ReversalReport {
    pub fn is_fully_resolved(&self) -> bool {
        self.unresolved.is_empty()
            && !self
                .preserved_conflicts
                .iter()
                .any(|conflict| conflict.blocking)
    }
}

/// A foreign path preserved during capture or reversal
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedConflict {
    /// Logical target path involved in the conflict
    pub path: Utf8PathBuf,
    /// Physical location left untouched
    pub preserved_at: Utf8PathBuf,
    /// User-facing explanation of why the path was preserved
    pub reason: String,
    /// Whether this conflict prevents journal removal
    pub blocking: bool,
}

/// A path-aware issue that prevented reversal from finishing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReversalIssue {
    /// Physical path that could not be resolved
    pub path: Utf8PathBuf,
    /// User-facing failure detail
    pub reason: String,
}

impl ReversalIssue {
    pub(crate) fn new(path: impl Into<Utf8PathBuf>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
#[path = "tests/record.rs"]
mod tests;
