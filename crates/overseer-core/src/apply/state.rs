//! Persisted record of an instance's single live deployment

use super::error::{ApplyError, io_err};
use crate::deploy::DeployRecord;
use crate::fs;
use crate::instance::Instance;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// Where a deployment transaction stands, used in crash recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Journalled before the game target was mutated - reversible
    InProgress,
    /// Deployment completed & is live
    Committed,
    /// A reversal could not resolve every path - manual recovery may be needed
    RecoveryFailed,
}

/// The one live deployment for an instance, stored at `<instance>/state/deployment.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    /// Where this transaction stands
    pub status: Status,
    /// Whether this deployment ever reached Committed, or None for a legacy journal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed: Option<bool>,
    /// Name of the profile that was deployed
    pub profile: String,
    /// What the file deploy wrote, so `purge` can remove them
    pub record: DeployRecord,
    /// The user's original `Plugins.txt` bytes, if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugins_txt_backup: Option<Vec<u8>>,

    /// The `Plugins.txt` bytes this deployment wrote, so reversal can detect later user/tool changes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugins_txt_intended: Option<Vec<u8>>,

    /// The user's prior `SLocalSavePath`, captured when local saves are deployed so reversal can restore it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_redirect: Option<SaveRedirect>,
}

/// Journalled record that a deployment redirected the game's save path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveRedirect {
    /// The user's `SLocalSavePath` before we wrote ours, if they had one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
}

impl Deployment {
    /// Whether side-effect reversal must preserve post-commit external changes
    pub(crate) fn was_committed(&self) -> bool {
        self.committed.unwrap_or(self.status != Status::InProgress)
    }

    /// Path to the state file for `instance`
    pub(crate) fn path(instance: &Instance) -> Utf8PathBuf {
        instance.state_dir().join("deployment.json")
    }

    /// Path to the reversal-only pre-deployment path snapshot
    pub(crate) fn baseline_path(instance: &Instance) -> Utf8PathBuf {
        instance.state_dir().join("deployment-baseline.json")
    }

    /// Whether a deployment is currently recorded for `instance`
    pub(crate) fn exists(instance: &Instance) -> bool {
        Self::path(instance).exists()
    }

    /// Read the recorded deployment, or [`ApplyError::NotDeployed`] if there is none
    pub(crate) fn load(instance: &Instance) -> Result<Self, ApplyError> {
        let path = Self::path(instance);
        let text = std::fs::read_to_string(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ApplyError::NotDeployed { path: path.clone() }
            } else {
                io_err(&path, source).into()
            }
        })?;
        serde_json::from_str(&text).map_err(|source| ApplyError::State { path, source })
    }

    /// Write state file atomically
    pub(crate) fn save(&self, instance: &Instance) -> Result<(), ApplyError> {
        let dir = instance.state_dir();
        fs::ensure_dir(&dir)?;
        let path = Self::path(instance);
        let text = serde_json::to_string_pretty(self).map_err(|source| ApplyError::State {
            path: path.clone(),
            source,
        })?;
        fs::write_atomic(&path, text.as_bytes()).map_err(Into::into)
    }

    /// Persist the sorted pre-deployment path snapshot outside the main journal
    pub(crate) fn save_baseline(
        instance: &Instance,
        paths: &[Utf8PathBuf],
    ) -> Result<(), ApplyError> {
        let path = Self::baseline_path(instance);
        let text = serde_json::to_string(paths).map_err(|source| ApplyError::State {
            path: path.clone(),
            source,
        })?;
        fs::write_atomic(&path, text.as_bytes()).map_err(Into::into)
    }

    /// Load the optional path snapshot, treating its absence as a legacy journal
    pub(crate) fn load_baseline(
        instance: &Instance,
    ) -> Result<Option<Vec<Utf8PathBuf>>, ApplyError> {
        let path = Self::baseline_path(instance);
        let Some(text) = fs::read_to_string_opt(&path)? else {
            return Ok(None);
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|source| ApplyError::State { path, source })
    }

    /// Remove an orphan-free optional baseline sidecar
    pub(crate) fn remove_baseline(instance: &Instance) -> Result<(), ApplyError> {
        fs::remove_file_opt(&Self::baseline_path(instance)).map_err(Into::into)
    }

    /// Delete the state file, marking the instance as no longer deployed
    pub(crate) fn remove(instance: &Instance) -> Result<(), ApplyError> {
        Self::remove_baseline(instance)?;
        let path = Self::path(instance);
        std::fs::remove_file(&path).map_err(|e| io_err(&path, e).into())
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
