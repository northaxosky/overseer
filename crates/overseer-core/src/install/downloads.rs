//! Listing the per-instance `downloads/` directory of installable archives

use super::archive::ArchiveFormat;
use super::error::InstallError;
use crate::error::io_err;
use crate::instance::Instance;
use camino::Utf8PathBuf;
use std::time::SystemTime;

/// One installable archive sitting in an instance's `downloads/` directory
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadEntry {
    /// The archive's file name, e.g. `Mod-1.2.7z`
    pub name: String,
    /// Absolute path to the archive in `downloads/`
    pub path: Utf8PathBuf,
    /// Whether `mods/<file stem>/` already exists; likely a prior install
    pub installed: bool,
    /// Archive size in bytes, or 0 when filesystem metadata is unavailable
    pub size: u64,
    /// Archive modification time, or [`SystemTime::UNIX_EPOCH`] when unavailable
    pub modified: SystemTime,
}

/// List the installable archives in `instance.downloads_dir()`, sorted by name
pub fn list_downloads(instance: &Instance) -> Result<Vec<DownloadEntry>, InstallError> {
    let dir = instance.downloads_dir();
    let Some(entries) = crate::fs::read_dir_opt(&dir)? else {
        return Ok(Vec::new());
    };

    let mut downloads = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(&dir, e))?;
        if !entry.file_type().map_err(|e| io_err(&dir, e))?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = dir.join(&name);

        // Only files with a supported extension are installable
        if ArchiveFormat::from_path(&path).is_none() {
            continue;
        }
        let (size, modified) = match entry.metadata() {
            Ok(metadata) => {
                let modified = metadata.modified().unwrap_or_else(|e| {
                    tracing::debug!(path = %path, error = %e, "download mtime unavailable; using epoch");
                    SystemTime::UNIX_EPOCH
                });
                (metadata.len(), modified)
            }
            Err(e) => {
                tracing::debug!(path = %path, error = %e, "download metadata unavailable; using defaults");
                (0, SystemTime::UNIX_EPOCH)
            }
        };
        // The default install name is the file stem, matching `install`/CLI
        let installed = path
            .file_stem()
            .is_some_and(|stem| instance.mods_dir().join(stem).is_dir());
        downloads.push(DownloadEntry {
            name,
            path,
            installed,
            size,
            modified,
        });
    }
    downloads.sort_by_cached_key(|e| e.name.to_ascii_lowercase());
    Ok(downloads)
}

#[cfg(test)]
#[path = "tests/downloads.rs"]
mod tests;
