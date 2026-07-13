//! Fixed pending-work bundle and its remove manifest

use std::io::ErrorKind;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use super::LifecycleError;
use crate::error::io_err;
use crate::fs;
use crate::instance::Instance;

const BUNDLE_DIR: &str = "pending-mod-operation";

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Operation {
    Remove,
}

#[derive(Debug, Serialize)]
pub(super) struct ManifestProfile {
    pub(super) profile: String,
    pub(super) original_modlist: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct Manifest {
    pub(super) operation: Operation,
    pub(super) mod_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) archive: Option<String>,

    pub(super) profiles: Vec<ManifestProfile>,
}

/// Return the one reserved pending-work path for an instance
pub(super) fn path(instance: &Instance) -> Utf8PathBuf {
    instance.state_dir().join(BUNDLE_DIR)
}

/// Serialize a manifest before creating its bundle
pub(super) fn serialize(path: &Utf8Path, manifest: &Manifest) -> Result<Vec<u8>, LifecycleError> {
    serde_json::to_vec_pretty(manifest).map_err(|source| LifecycleError::Manifest {
        path: path.join("manifest.json"),
        source,
    })
}

/// Create the previously probed empty bundle path
pub(super) fn create(path: &Utf8Path) -> Result<(), crate::IoError> {
    std::fs::create_dir(path).map_err(|source| crate::error::io_err(path, source))
}

/// Atomically publish a serialized manifest inside the bundle
pub(super) fn write_manifest(path: &Utf8Path, bytes: &[u8]) -> Result<(), crate::IoError> {
    fs::write_atomic(&path.join("manifest.json"), bytes)
}

/// Recursively remove a completed or fully rolled-back bundle
pub(super) fn cleanup(path: &Utf8Path) -> Result<(), crate::IoError> {
    #[cfg(test)]
    super::failpoint::check(super::failpoint::Point::Cleanup, path)?;

    fs::remove_dir_all_opt(path)
}

/// Probe a fixed path so only `NotFound` counts as absent
pub(super) fn occupied(path: &Utf8Path) -> Result<bool, crate::IoError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_err(path, error)),
    }
}

/// Rename a tree only when the destination has no filesystem occupant
pub(super) fn rename_tree(from: &Utf8Path, to: &Utf8Path) -> Result<(), crate::IoError> {
    #[cfg(test)]
    super::failpoint::check(super::failpoint::Point::Rename, from)?;

    if occupied(to)? {
        return Err(io_err(
            to,
            std::io::Error::new(
                ErrorKind::AlreadyExists,
                "rename destination already exists",
            ),
        ));
    }

    std::fs::rename(from, to).map_err(|error| io_err(from, error))
}
