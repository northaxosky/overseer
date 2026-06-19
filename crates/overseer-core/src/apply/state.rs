//! Persisted record of an instance's single live deployment

use super::error::{ApplyError, io_err};
use crate::deploy::DeployManifest;
use crate::instance::Instance;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// The one live deployment for an instance, stored at `<instance>/state/deployment.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    /// Name of the profile that was deployed
    pub profile: String,
    /// What the file deploy wrote, so `purge` can remove them
    pub manifest: DeployManifest,
}

impl Deployment {
    /// Path to the state file for `instance`
    pub fn path(instance: &Instance) -> Utf8PathBuf {
        instance.state_dir().join("deployment.json")
    }

    /// Whether a deployment is currently recorded for `instance`
    pub fn exists(instance: &Instance) -> bool {
        Self::path(instance).exists()
    }

    /// Read the recorded deployment, or [`ApplyError::NotDeployed`] if there is none
    pub fn load(instance: &Instance) -> Result<Self, ApplyError> {
        let path = Self::path(instance);
        let text = std::fs::read_to_string(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ApplyError::NotDeployed { path: path.clone() }
            } else {
                io_err(&path, source)
            }
        })?;
        serde_json::from_str(&text).map_err(|source| ApplyError::State { path, source })
    }

    /// Write this deployment to the state file, creating `state/` if needed
    pub fn save(&self, instance: &Instance) -> Result<(), ApplyError> {
        let dir = instance.state_dir();
        std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let path = Self::path(instance);
        let text = serde_json::to_string_pretty(self).map_err(|source| ApplyError::State {
            path: path.clone(),
            source,
        })?;
        std::fs::write(&path, text).map_err(|e| io_err(&path, e))
    }

    /// Delete the state file, marking the instance as no longer deployed
    pub fn remove(instance: &Instance) -> Result<(), ApplyError> {
        let path = Self::path(instance);
        std::fs::remove_file(&path).map_err(|e| io_err(&path, e))
    }
}
