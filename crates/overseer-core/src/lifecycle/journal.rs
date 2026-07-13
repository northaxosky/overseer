//! Lifecycle journal schema and fixed paths

use super::LifecycleError;
use crate::instance::Instance;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

const JOURNAL: &str = "lifecycle.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Phase {
    Active,
    RolledBack,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Operation {
    Rename { old: String, new: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProfileSnapshot {
    pub(super) profile: String,
    pub(super) original: Option<String>,
    pub(super) intended: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Journal {
    pub(super) version: u8,
    pub(super) transaction: String,
    pub(super) phase: Phase,
    pub(super) operation: Operation,
    pub(super) profiles: Vec<ProfileSnapshot>,
}

/// Load the current lifecycle journal when one exists
pub(super) fn load(instance: &Instance) -> Result<Option<Journal>, LifecycleError> {
    let path = journal_path(instance);

    let Some(bytes) = crate::fs::read_opt(&path)? else {
        return Ok(None);
    };

    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| corrupt(instance, error.to_string()))
}

/// Atomically persist the lifecycle journal
pub(super) fn save(instance: &Instance, journal: &Journal) -> Result<(), LifecycleError> {
    let path = journal_path(instance);
    let bytes =
        serde_json::to_vec(journal).map_err(|error| corrupt(instance, error.to_string()))?;
    crate::fs::write_atomic(&path, &bytes)?;
    #[cfg(test)]
    if journal.phase == Phase::Committed {
        super::tests::committed_write_visible_error(&path)?;
    }
    Ok(())
}

/// Validate all persisted values before recovery uses them
pub(super) fn validate(instance: &Instance, journal: &Journal) -> Result<(), LifecycleError> {
    let invalid = |reason: &str| corrupt(instance, reason.to_owned());

    if journal.version != 1 || !valid_transaction(&journal.transaction) {
        return Err(invalid("unsupported version or transaction"));
    }

    let Operation::Rename { old, new } = &journal.operation;
    crate::instance::validate_mod_name(old).map_err(|_| invalid("invalid operation name"))?;
    crate::instance::validate_mod_name(new).map_err(|_| invalid("invalid operation name"))?;
    for snapshot in &journal.profiles {
        crate::instance::validate_profile_name(&snapshot.profile)
            .map_err(|_| invalid("invalid profile name"))?;
    }
    Ok(())
}

/// Return the fixed lifecycle journal path
pub(super) fn journal_path(instance: &Instance) -> Utf8PathBuf {
    instance.state_dir().join(JOURNAL)
}

fn valid_transaction(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn corrupt(instance: &Instance, reason: String) -> LifecycleError {
    LifecycleError::CorruptJournal {
        path: journal_path(instance),
        reason,
    }
}
