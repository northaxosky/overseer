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
        if entry.file_type().map_err(|e| io_err(&dir, e))?.is_dir() {
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

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{set_mtime, temp_instance, touch};
    use std::time::{Duration, SystemTime};

    #[test]
    fn missing_downloads_dir_is_an_empty_list() {
        let (_tmp, instance) = temp_instance();
        assert!(list_downloads(&instance).expect("list").is_empty());
    }

    #[test]
    fn lists_supported_archives_sorted_ignoring_other_entries() {
        let (_tmp, instance) = temp_instance();
        let downloads = instance.downloads_dir();
        touch(&downloads.join("Zeta.zip"));
        touch(&downloads.join("alpha.7z"));
        touch(&downloads.join("readme.txt")); // not an archive
        std::fs::create_dir_all(downloads.join("Nested.zip")).expect("subdir"); // a dir, ignored

        let names: Vec<String> = list_downloads(&instance)
            .expect("list")
            .into_iter()
            .map(|e| e.name)
            .collect();
        // Case-insensitive sort puts `alpha.7z` before `Zeta.zip`; non-archives gone
        assert_eq!(names, ["alpha.7z", "Zeta.zip"]);
    }

    #[test]
    fn installed_flag_tracks_the_mods_directory() {
        let (_tmp, instance) = temp_instance();
        touch(&instance.downloads_dir().join("CoolMod.zip"));
        touch(&instance.downloads_dir().join("Other.zip"));
        // A mods/<stem>/ folder marks the first archive as already installed
        std::fs::create_dir_all(instance.mods_dir().join("CoolMod")).expect("mkdir");

        let entries = list_downloads(&instance).expect("list");
        let installed: Vec<(&str, bool)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.installed))
            .collect();
        assert_eq!(installed, [("CoolMod.zip", true), ("Other.zip", false)]);
    }

    #[test]
    fn entries_include_size_and_modified_time() {
        let (_tmp, instance) = temp_instance();
        let archive = instance.downloads_dir().join("Sized.zip");
        std::fs::create_dir_all(archive.parent().expect("parent")).expect("mkdir");
        std::fs::write(&archive, b"abc").expect("write");
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        set_mtime(&archive, modified);

        let entries = list_downloads(&instance).expect("list");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 3);
        assert_eq!(entries[0].modified, modified);
    }
}
